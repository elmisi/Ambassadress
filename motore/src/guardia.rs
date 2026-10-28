//! La guardia della sola lettura.
//!
//! Ogni byte che il motore manda al server IMAP passa da `Guardia::verifica`
//! (la chiama `rete::Connessione::invia`, l'unico punto che scrive sul socket).
//! La guardia lavora su una lista di comandi ammessi: tutto quello che non è
//! nella lista viene fermato prima di uscire dal programma.
//!
//! Ammessi: CAPABILITY, ID, LOGIN, AUTHENTICATE, ENABLE (solo CONDSTORE e QRESYNC),
//! COMPRESS DEFLATE, NAMESPACE, LIST, STATUS, EXAMINE, FETCH e UID FETCH (con il
//! corpo solo tramite BODY.PEEK o BINARY.PEEK), SEARCH e UID SEARCH, IDLE e DONE,
//! NOOP, LOGOUT.
//!
//! Fermati, fra gli altri: SELECT, STORE, APPEND, COPY, MOVE, EXPUNGE, DELETE,
//! CREATE, RENAME, SUBSCRIBE, UID STORE/COPY/MOVE/EXPUNGE, e in FETCH gli
//! attributi BODY[…], BINARY[…], RFC822 e RFC822.TEXT, che segnerebbero il
//! messaggio come letto.

use crate::errore::Errore;

/// Attributi FETCH che non toccano lo stato del messaggio.
const ATTRIBUTI_SEMPLICI: &[&str] = &[
    "UID",
    "FLAGS",
    "INTERNALDATE",
    "RFC822.SIZE",
    // Equivale a BODY.PEEK[HEADER] (RFC 3501, 6.4.5): non imposta \Seen.
    "RFC822.HEADER",
    "ENVELOPE",
    "BODYSTRUCTURE",
    // BODY senza sezione è la struttura non estesa, non il contenuto.
    "BODY",
    "MODSEQ",
    "X-GM-MSGID",
    "X-GM-THRID",
    "X-GM-LABELS",
    "EMAILID",
    "THREADID",
    "SAVEDATE",
    // Macro di RFC 3501: nessuna contiene il corpo.
    "ALL",
    "FAST",
    "FULL",
];

#[derive(Debug, Default)]
pub struct Guardia {
    in_idle: bool,
}

fn rifiuta(motivo: impl Into<String>) -> Errore {
    Errore::Rifiutato(motivo.into())
}

impl Guardia {
    pub fn new() -> Self {
        Self::default()
    }

    /// Da chiamare se il server rifiuta IDLE: non si aspetta più un DONE.
    pub fn annulla_idle(&mut self) {
        self.in_idle = false;
    }

    /// Controlla un comando completo, così come sta per uscire (CRLF compreso).
    pub fn verifica(&mut self, comando: &[u8]) -> Result<(), Errore> {
        let Some(corpo) = comando.strip_suffix(b"\r\n") else {
            return Err(rifiuta("un comando deve finire con CRLF"));
        };
        // Una sola riga per invio: niente CR, LF o NUL in mezzo, quindi niente
        // letterali e niente secondo comando nascosto dentro un argomento.
        if corpo.iter().any(|&b| b == b'\r' || b == b'\n' || b == 0) {
            return Err(rifiuta("più di una riga in un solo invio"));
        }
        let testo =
            std::str::from_utf8(corpo).map_err(|_| rifiuta("byte non validi nel comando"))?;

        if self.in_idle {
            if testo.eq_ignore_ascii_case("DONE") {
                self.in_idle = false;
                return Ok(());
            }
            return Err(rifiuta("durante IDLE si può mandare solo DONE"));
        }
        if testo.eq_ignore_ascii_case("DONE") {
            return Err(rifiuta("DONE fuori da IDLE"));
        }

        let (tag, resto) = testo
            .split_once(' ')
            .ok_or_else(|| rifiuta("comando senza etichetta"))?;
        if tag.is_empty()
            || !tag
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
        {
            return Err(rifiuta("etichetta del comando non valida"));
        }
        let (nome, argomenti) = match resto.split_once(' ') {
            Some((n, a)) => (n, a.trim()),
            None => (resto, ""),
        };
        let nome = nome.to_ascii_uppercase();
        match nome.as_str() {
            "CAPABILITY" | "NOOP" | "LOGOUT" | "NAMESPACE" => {
                if argomenti.is_empty() {
                    Ok(())
                } else {
                    Err(rifiuta(format!("{nome} non vuole argomenti")))
                }
            }
            // Comandi che non cambiano niente sul server, qualunque sia l'argomento.
            "ID" | "LOGIN" | "AUTHENTICATE" | "LIST" | "STATUS" | "EXAMINE" | "SEARCH" => Ok(()),
            "ENABLE" => {
                if argomenti.is_empty() {
                    return Err(rifiuta("ENABLE senza argomenti"));
                }
                for estensione in argomenti.split(' ') {
                    let e = estensione.to_ascii_uppercase();
                    if e != "CONDSTORE" && e != "QRESYNC" {
                        return Err(rifiuta(format!(
                            "ENABLE ammesso solo per CONDSTORE e QRESYNC, non per {}",
                            pulito(&e)
                        )));
                    }
                }
                Ok(())
            }
            "COMPRESS" => {
                if argomenti.eq_ignore_ascii_case("DEFLATE") {
                    Ok(())
                } else {
                    Err(rifiuta("COMPRESS ammesso solo con DEFLATE"))
                }
            }
            "IDLE" => {
                if !argomenti.is_empty() {
                    return Err(rifiuta("IDLE non vuole argomenti"));
                }
                self.in_idle = true;
                Ok(())
            }
            "FETCH" => verifica_fetch(argomenti),
            "UID" => {
                let (sotto, resto) = match argomenti.split_once(' ') {
                    Some((s, r)) => (s.to_ascii_uppercase(), r.trim()),
                    None => (argomenti.to_ascii_uppercase(), ""),
                };
                match sotto.as_str() {
                    "FETCH" => verifica_fetch(resto),
                    "SEARCH" => Ok(()),
                    _ => Err(rifiuta(format!(
                        "comando non ammesso: UID {}",
                        pulito(&sotto)
                    ))),
                }
            }
            _ => Err(rifiuta(format!("comando non ammesso: {}", pulito(&nome)))),
        }
    }
}

/// Il nome di un comando o di un attributo, ripulito e accorciato, per i messaggi d'errore.
fn pulito(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '[' | ']' | '_'))
        .take(32)
        .collect()
}

/// `FETCH <insieme> <attributi> [<modificatori>]`
fn verifica_fetch(argomenti: &str) -> Result<(), Errore> {
    let (insieme, resto) = argomenti
        .split_once(' ')
        .ok_or_else(|| rifiuta("FETCH senza attributi"))?;
    if insieme.is_empty()
        || !insieme
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b':' || b == b',' || b == b'*')
    {
        return Err(rifiuta("FETCH con un insieme di messaggi non valido"));
    }
    let resto = resto.trim_start();
    let (attributi, modificatori) = if resto.starts_with('(') {
        let fine = chiusura(resto).ok_or_else(|| rifiuta("parentesi non chiuse in FETCH"))?;
        (&resto[1..fine], resto[fine + 1..].trim())
    } else {
        // Un solo attributo senza parentesi, eventualmente seguito dai modificatori.
        let fine = fine_elemento(resto);
        (&resto[..fine], resto[fine..].trim())
    };
    let elementi = elementi(attributi)?;
    if elementi.is_empty() {
        return Err(rifiuta("FETCH senza attributi"));
    }
    for e in elementi {
        verifica_attributo(e)?;
    }
    if !modificatori.is_empty() {
        verifica_modificatori(modificatori)?;
    }
    Ok(())
}

/// Indice della parentesi che chiude quella in posizione 0, tenendo conto delle quadre.
fn chiusura(s: &str) -> Option<usize> {
    let mut tonde = 0i32;
    let mut quadre = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => quadre += 1,
            ']' => quadre -= 1,
            '(' if quadre == 0 => tonde += 1,
            ')' if quadre == 0 => {
                tonde -= 1;
                if tonde == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Fine del primo elemento (spazio fuori da quadre e tonde).
fn fine_elemento(s: &str) -> usize {
    let mut profondita = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '[' | '(' => profondita += 1,
            ']' | ')' => profondita -= 1,
            ' ' if profondita == 0 => return i,
            _ => {}
        }
    }
    s.len()
}

fn elementi(s: &str) -> Result<Vec<&str>, Errore> {
    let mut out = Vec::new();
    let mut resto = s.trim();
    while !resto.is_empty() {
        let fine = fine_elemento(resto);
        let e = &resto[..fine];
        if e.contains('(') && !e.contains('[') {
            return Err(rifiuta("attributo FETCH non valido"));
        }
        out.push(e);
        resto = resto[fine..].trim_start();
    }
    Ok(out)
}

fn verifica_attributo(attributo: &str) -> Result<(), Errore> {
    let su = attributo.to_ascii_uppercase();
    let Some(aperta) = su.find('[') else {
        if ATTRIBUTI_SEMPLICI.contains(&su.as_str()) {
            return Ok(());
        }
        return Err(match su.as_str() {
            "RFC822" | "RFC822.TEXT" => rifiuta(format!(
                "{su} segnerebbe il messaggio come letto: si usa solo BODY.PEEK[…]"
            )),
            _ => rifiuta(format!("attributo FETCH non ammesso: {}", pulito(&su))),
        });
    };
    let nome = &su[..aperta];
    let chiusa = su
        .rfind(']')
        .ok_or_else(|| rifiuta("sezione FETCH non chiusa"))?;
    let coda = &su[chiusa + 1..];
    if !coda.is_empty() && !parziale_valido(coda) {
        return Err(rifiuta("intervallo FETCH non valido"));
    }
    match nome {
        "BODY.PEEK" | "BINARY.PEEK" => Ok(()),
        "BINARY.SIZE" if coda.is_empty() => Ok(()),
        "BODY" | "BINARY" => Err(rifiuta(format!(
            "{nome}[…] segnerebbe il messaggio come letto: si usa solo {nome}.PEEK[…]"
        ))),
        _ => Err(rifiuta(format!(
            "attributo FETCH non ammesso: {}",
            pulito(nome)
        ))),
    }
}

/// `<inizio>` oppure `<inizio.quanti>`
fn parziale_valido(s: &str) -> bool {
    let Some(dentro) = s.strip_prefix('<').and_then(|s| s.strip_suffix('>')) else {
        return false;
    };
    let mut parti = dentro.split('.');
    let numero =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let primo = parti.next();
    match parti.next() {
        None => numero(primo),
        Some(q) => numero(primo) && numero(Some(q)) && parti.next().is_none(),
    }
}

/// `(CHANGEDSINCE <n>)`, `(CHANGEDSINCE <n> VANISHED)`
fn verifica_modificatori(s: &str) -> Result<(), Errore> {
    let dentro = s
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| rifiuta("modificatori FETCH non validi"))?;
    let mut parole = dentro.split(' ').filter(|p| !p.is_empty());
    while let Some(p) = parole.next() {
        match p.to_ascii_uppercase().as_str() {
            "CHANGEDSINCE" => {
                let n = parole.next().unwrap_or("");
                if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(rifiuta("CHANGEDSINCE senza numero"));
                }
            }
            "VANISHED" => {}
            altro => {
                return Err(rifiuta(format!(
                    "modificatore FETCH non ammesso: {}",
                    pulito(altro)
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod prove {
    use super::*;

    fn passa(c: &str) -> bool {
        Guardia::new()
            .verifica(format!("{c}\r\n").as_bytes())
            .is_ok()
    }

    #[test]
    fn ammessi() {
        for c in [
            "a1 CAPABILITY",
            "a2 LOGIN \"utente@example.invalid\" \"segreto\"",
            "a3 AUTHENTICATE PLAIN",
            "a4 ID (\"name\" \"Ambassadress\")",
            "a5 ENABLE CONDSTORE",
            "a6 ENABLE QRESYNC",
            "a7 enable condstore qresync",
            "a8 COMPRESS DEFLATE",
            "a9 NAMESPACE",
            "a10 LIST \"\" \"*\"",
            "a11 STATUS INBOX (MESSAGES UIDNEXT HIGHESTMODSEQ)",
            "a12 EXAMINE \"[Gmail]/Tutti i messaggi\"",
            "a13 EXAMINE INBOX (CONDSTORE)",
            "a14 EXAMINE INBOX (QRESYNC (67890007 20050715194045000))",
            "a15 UID SEARCH ALL",
            "a16 SEARCH UNSEEN",
            "a17 UID FETCH 1:* (UID FLAGS)",
            "a18 UID FETCH 1:* (UID FLAGS MODSEQ X-GM-LABELS) (CHANGEDSINCE 12345)",
            "a19 UID FETCH 300:400 (UID FLAGS INTERNALDATE RFC822.SIZE BODYSTRUCTURE X-GM-MSGID X-GM-THRID X-GM-LABELS BODY.PEEK[HEADER.FIELDS (DATE FROM TO CC SUBJECT MESSAGE-ID)])",
            "a20 UID FETCH 7 (UID BODY.PEEK[1.2]<0.1048576> BODY.PEEK[1.1]<0.1048576>)",
            "a21 FETCH 1:10 FLAGS",
            "a22 FETCH 1 BODY.PEEK[]",
            "a23 FETCH 1 (BINARY.PEEK[1] BINARY.SIZE[1])",
            "a24 FETCH 1:5 FAST",
            "a25 FETCH 1 (RFC822.HEADER RFC822.SIZE ENVELOPE BODY)",
            "a26 UID FETCH 1:* (UID FLAGS) (CHANGEDSINCE 1 VANISHED)",
            "a27 NOOP",
            "a28 IDLE",
            "a29 LOGOUT",
        ] {
            assert!(passa(c), "dovrebbe passare: {c}");
        }
    }

    #[test]
    fn rifiutati() {
        for c in [
            "a1 SELECT INBOX",
            "a2 STORE 1 +FLAGS (\\Seen)",
            "a3 UID STORE 1 +FLAGS.SILENT (\\Deleted)",
            "a4 APPEND INBOX {10}",
            "a5 COPY 1 Archivio",
            "a6 UID COPY 1 Archivio",
            "a7 MOVE 1 Archivio",
            "a8 UID MOVE 1 Archivio",
            "a9 EXPUNGE",
            "a10 UID EXPUNGE 1",
            "a11 DELETE Cartella",
            "a12 CREATE Cartella",
            "a13 RENAME A B",
            "a14 SUBSCRIBE A",
            "a15 UNSUBSCRIBE A",
            "a16 CLOSE",
            "a17 UNSELECT",
            "a18 SETMETADATA INBOX (/private/comment \"x\")",
            "a19 FETCH 1 BODY[]",
            "a20 UID FETCH 1 (UID BODY[TEXT])",
            "a21 UID FETCH 1 (UID BODY[HEADER.FIELDS (SUBJECT)])",
            "a22 FETCH 1 RFC822",
            "a23 FETCH 1 (FLAGS RFC822.TEXT)",
            "a24 FETCH 1 BINARY[1]",
            "a25 FETCH 1 (FLAGS X-STRANO)",
            "a26 FETCH 1 BODY.PEEK",
            "a27 UID FETCH 1 (UID) (UNCHANGEDSINCE 5)",
            "a28 ENABLE UTF8=ACCEPT",
            "a29 COMPRESS ALTRO",
            "a30 GETQUOTAROOT INBOX",
            "a31 XYZZY",
            "a32 uid store 1 flags ()",
            "a33 FETCH 1 (body[])",
            "a34 FETCH 1 BODY.PEEK[]<abc>",
        ] {
            assert!(!passa(c), "dovrebbe essere fermato: {c}");
        }
    }

    #[test]
    fn una_riga_sola() {
        let mut g = Guardia::new();
        // Un secondo comando nascosto dopo un CRLF interno.
        assert!(
            g.verifica(b"a1 EXAMINE \"x\"\r\na2 STORE 1 +FLAGS (\\Seen)\r\n")
                .is_err()
        );
        assert!(g.verifica(b"a1 NOOP\na2 EXPUNGE\r\n").is_err());
        assert!(g.verifica(b"a1 NOOP").is_err());
        assert!(g.verifica(b"a1 LOGIN {5}\r\n").is_ok()); // il letterale vero richiederebbe un secondo invio…
        assert!(g.verifica(b"utente\r\n").is_err()); // …che la guardia ferma
        assert!(g.verifica(b"a1 NOOP\0\r\n").is_err());
    }

    #[test]
    fn idle_e_done() {
        let mut g = Guardia::new();
        assert!(g.verifica(b"DONE\r\n").is_err());
        assert!(g.verifica(b"a1 IDLE\r\n").is_ok());
        assert!(g.verifica(b"a2 NOOP\r\n").is_err());
        assert!(g.verifica(b"a2 STORE 1 +FLAGS (\\Seen)\r\n").is_err());
        assert!(g.verifica(b"DONE\r\n").is_ok());
        assert!(g.verifica(b"DONE\r\n").is_err());
        assert!(g.verifica(b"a3 NOOP\r\n").is_ok());
    }

    #[test]
    fn errore_senza_argomenti() {
        // Il messaggio d'errore nomina il comando, mai gli argomenti.
        let e = Guardia::new()
            .verifica(b"a1 STORE 1 +FLAGS (\\Seen) testo-riservato\r\n")
            .unwrap_err()
            .to_string();
        assert!(e.contains("STORE"));
        assert!(!e.contains("riservato"));
    }
}
