//! Un finto server IMAP su localhost, per le prove senza rete. Risponde in modo
//! scriptato ai comandi che il motore usa, con dati inventati, e registra ogni
//! comando ricevuto: così le prove possono dimostrare che non è mai arrivato
//! niente che scriva. Può anche guastarsi a comando: connessione che cade a metà
//! FETCH, risposte malformate, IDLE che muore in silenzio, saluto che non arriva.

#![allow(dead_code)]

use std::collections::HashSet;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use motore::credenziali::{Credenziali, Segreto};
use motore::rete::{Indirizzo, Sicurezza};
use motore::sincronizza::{Config, Opzioni};

pub const UTENTE: &str = "prova@esempio.invalid";
pub const PASSWORD: &str = "segreto-di-prova";

#[derive(Clone, Debug)]
pub struct MessaggioFinto {
    pub uid: u32,
    pub msgid: u64,
    pub thrid: u64,
    pub etichette: Vec<String>,
    pub flags: Vec<String>,
    pub modseq: u64,
    pub intestazioni: String,
    /// Testo in quoted-printable, come sta sul server.
    pub testo: String,
    pub allegato: Option<String>,
}

impl MessaggioFinto {
    /// Un messaggio inventato. Ogni terzo ha un allegato, ogni quarto parla di «preventivo».
    pub fn inventato(n: u32) -> Self {
        let argomento = if n.is_multiple_of(4) {
            "Preventivo"
        } else {
            "Argomento"
        };
        let riferimenti = if n > 1 && n.is_multiple_of(2) {
            format!(
                "In-Reply-To: <m{}@esempio.invalid>\r\nReferences: <m{}@esempio.invalid>\r\n",
                n - 1,
                n - 1
            )
        } else {
            String::new()
        };
        let intestazioni = format!(
            "From: \"Persona {}\" <persona{}@esempio.invalid>\r\n\
             To: io@esempio.invalid\r\n\
             Subject: {argomento} di prova {n}\r\n\
             Message-ID: <m{n}@esempio.invalid>\r\n\
             {riferimenti}\
             Date: Tue, 27 Oct 2026 10:{:02}:00 +0100\r\n\r\n",
            n % 5,
            n % 5,
            n % 60
        );
        MessaggioFinto {
            uid: n,
            msgid: 1_000_000 + n as u64,
            thrid: 2_000_000 + n.div_ceil(2) as u64,
            etichette: vec!["\\Inbox".into(), "Prove &AOA-".into()],
            flags: if n.is_multiple_of(3) {
                vec![]
            } else {
                vec!["\\Seen".into()]
            },
            modseq: 100 + n as u64,
            intestazioni,
            testo: format!("Testo inventato numero {n}, parolarara{n}. Perch=C3=A9 no?\r\n"),
            allegato: if n.is_multiple_of(3) {
                Some(format!("documento-{n}.pdf"))
            } else {
                None
            },
        }
    }

    fn struttura(&self) -> String {
        let testo = format!(
            "(\"TEXT\" \"PLAIN\" (\"CHARSET\" \"UTF-8\") NIL NIL \"QUOTED-PRINTABLE\" {} 1 NIL NIL NIL NIL)",
            self.testo.len()
        );
        match &self.allegato {
            None => testo,
            Some(nome) => format!(
                "({testo}(\"APPLICATION\" \"PDF\" (\"NAME\" \"{nome}\") NIL NIL \"BASE64\" 5000 NIL (\"ATTACHMENT\" (\"FILENAME\" \"{nome}\")) NIL NIL) \"MIXED\" (\"BOUNDARY\" \"confine\") NIL NIL NIL)"
            ),
        }
    }

    fn dimensione(&self) -> usize {
        self.intestazioni.len() + self.testo.len() + if self.allegato.is_some() { 5200 } else { 0 }
    }
}

#[derive(Clone, Debug)]
pub struct Casella {
    pub uidvalidity: u32,
    pub prossimo_uid: u32,
    pub modseq: u64,
    pub messaggi: Vec<MessaggioFinto>,
    pub rimossi: Vec<(u32, u64)>,
}

impl Casella {
    pub fn con(n: u32) -> Self {
        let messaggi: Vec<_> = (1..=n).map(MessaggioFinto::inventato).collect();
        let modseq = messaggi.iter().map(|m| m.modseq).max().unwrap_or(1);
        Casella {
            uidvalidity: 4242,
            prossimo_uid: n + 1,
            modseq,
            messaggi,
            rimossi: Vec::new(),
        }
    }

    pub fn aggiungi(&mut self) -> u32 {
        let uid = self.prossimo_uid;
        self.prossimo_uid += 1;
        self.modseq += 1;
        let mut m = MessaggioFinto::inventato(uid);
        m.modseq = self.modseq;
        self.messaggi.push(m);
        uid
    }

    pub fn cambia_flag(&mut self, uid: u32, flags: &[&str]) {
        self.modseq += 1;
        let modseq = self.modseq;
        if let Some(m) = self.messaggi.iter_mut().find(|m| m.uid == uid) {
            m.flags = flags.iter().map(|s| s.to_string()).collect();
            m.modseq = modseq;
        }
    }

    pub fn rimuovi(&mut self, uid: u32) {
        self.modseq += 1;
        self.messaggi.retain(|m| m.uid != uid);
        self.rimossi.push((uid, self.modseq));
    }

    /// Nuova UIDVALIDITY: stessi messaggi, UID tutti diversi.
    pub fn rinumera(&mut self, uidvalidity: u32) {
        self.uidvalidity = uidvalidity;
        let mut uid = 500;
        for m in &mut self.messaggi {
            m.uid = uid;
            uid += 1;
        }
        self.prossimo_uid = uid;
        self.rimossi.clear();
    }
}

#[derive(Debug, Default)]
pub struct Guasti {
    /// Chiude la connessione dopo questo numero di risposte in un FETCH di intestazioni (una volta).
    pub cadi_dopo: Option<usize>,
    /// UID le cui intestazioni arrivano malformate.
    pub malformati: HashSet<u32>,
    /// Quante connessioni IDLE resteranno mute dopo «+ idling».
    pub idle_muto: u32,
    /// Il saluto non arriva mai.
    pub saluto_muto: bool,
    /// EXAMINE risponde [READ-WRITE].
    pub examine_in_scrittura: bool,
}

#[derive(Debug)]
pub struct Stato {
    pub casella: Casella,
    pub guasti: Guasti,
    pub comandi: Vec<String>,
    pub connessioni: u32,
    pub gmail: bool,
    pub condstore: bool,
    pub qresync: bool,
}

impl Stato {
    pub fn gmail(n: u32) -> Self {
        Stato {
            casella: Casella::con(n),
            guasti: Guasti::default(),
            comandi: Vec::new(),
            connessioni: 0,
            gmail: true,
            condstore: true,
            qresync: false,
        }
    }

    fn capacita(&self) -> String {
        let mut c = String::from("IMAP4rev1 IDLE ID ENABLE NAMESPACE COMPRESS=DEFLATE");
        if self.condstore {
            c.push_str(" CONDSTORE");
        }
        if self.qresync {
            c.push_str(" QRESYNC");
        }
        if self.gmail {
            c.push_str(" X-GM-EXT-1");
        }
        c
    }
}

pub struct FintoServer {
    pub porta: u16,
    pub stato: Arc<Mutex<Stato>>,
}

impl FintoServer {
    pub fn avvia(stato: Stato) -> Self {
        let ascolto = TcpListener::bind("127.0.0.1:0").expect("porta libera");
        let porta = ascolto.local_addr().unwrap().port();
        let stato = Arc::new(Mutex::new(stato));
        let s = stato.clone();
        std::thread::spawn(move || {
            for c in ascolto.incoming() {
                let Ok(c) = c else { continue };
                let s = s.clone();
                std::thread::spawn(move || {
                    let _ = servi(c, s);
                });
            }
        });
        FintoServer { porta, stato }
    }

    pub fn config(&self) -> Config {
        Config {
            indirizzo: Indirizzo {
                host: "127.0.0.1".into(),
                porta: self.porta,
                sicurezza: Sicurezza::InChiaro,
            },
            credenziali: Credenziali {
                utente: Segreto::new(UTENTE.into()),
                password: Segreto::new(PASSWORD.into()),
            },
            timeout: Duration::from_secs(2),
        }
    }

    pub fn con<R>(&self, f: impl FnOnce(&mut Stato) -> R) -> R {
        f(&mut self.stato.lock().unwrap())
    }

    pub fn comandi(&self) -> Vec<String> {
        self.con(|s| s.comandi.clone())
    }

    pub fn svuota_comandi(&self) {
        self.con(|s| s.comandi.clear());
    }
}

pub fn opzioni_prova() -> Opzioni {
    Opzioni {
        limite: Some(20),
        corpi: 5,
        lotto: 7,
        lotto_corpi: 3,
        attesa_iniziale: Duration::from_millis(10),
        ..Opzioni::default()
    }
}

/// Le stringhe tra virgolette di una riga, in ordine.
fn tra_virgolette(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut dentro = false;
    let mut escape = false;
    for c in s.chars() {
        if dentro {
            if escape {
                cur.push(c);
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                dentro = false;
                out.push(std::mem::take(&mut cur));
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            dentro = true;
        }
    }
    out
}

fn uid_nell_insieme(insieme: &str, uid: u32, massimo: u32) -> bool {
    insieme.split(',').any(|pezzo| {
        let num = |x: &str| {
            if x == "*" {
                massimo
            } else {
                x.parse().unwrap_or(0)
            }
        };
        match pezzo.split_once(':') {
            Some((a, b)) => {
                let (a, b) = (num(a), num(b));
                uid >= a.min(b) && uid <= a.max(b)
            }
            None => uid == num(pezzo),
        }
    })
}

fn flag(f: &[String]) -> String {
    format!("({})", f.join(" "))
}

fn etichette(e: &[String]) -> String {
    format!(
        "({})",
        e.iter()
            .map(|x| format!("\"{}\"", x.replace('\\', "\\\\")))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn servi(c: TcpStream, stato: Arc<Mutex<Stato>>) -> std::io::Result<()> {
    let mut w = c.try_clone()?;
    let mut r = BufReader::new(c);
    let (saluto_muto, capacita) = {
        let mut s = stato.lock().unwrap();
        s.connessioni += 1;
        (s.guasti.saluto_muto, s.capacita())
    };
    if saluto_muto {
        std::thread::sleep(Duration::from_secs(5));
        return Ok(());
    }
    write!(
        w,
        "* OK [CAPABILITY {capacita}] finto server IMAP pronto\r\n"
    )?;
    // Quanti messaggi conosce il client (dall'ultimo EXAMINE o EXISTS mandato).
    let mut conosciuti = 0usize;
    loop {
        let mut riga = String::new();
        if r.read_line(&mut riga)? == 0 {
            return Ok(());
        }
        let riga = riga.trim_end_matches(['\r', '\n']).to_string();
        stato.lock().unwrap().comandi.push(riga.clone());
        let Some((etichetta, resto)) = riga.split_once(' ') else {
            write!(w, "* BAD riga senza etichetta\r\n")?;
            continue;
        };
        let (comando, argomenti) = resto.split_once(' ').unwrap_or((resto, ""));
        let comando = comando.to_ascii_uppercase();
        let mut s = stato.lock().unwrap();
        match comando.as_str() {
            "CAPABILITY" => write!(
                w,
                "* CAPABILITY {}\r\n{etichetta} OK fatto\r\n",
                s.capacita()
            )?,
            "LOGIN" => {
                let v = tra_virgolette(argomenti);
                if v.len() == 2 && v[0] == UTENTE && v[1] == PASSWORD {
                    write!(w, "{etichetta} OK [CAPABILITY {}] dentro\r\n", s.capacita())?;
                } else {
                    write!(
                        w,
                        "{etichetta} NO [AUTHENTICATIONFAILED] Credenziali non valide (prova)\r\n"
                    )?;
                }
            }
            "ID" => write!(w, "* ID (\"name\" \"finto\")\r\n{etichetta} OK fatto\r\n")?,
            "ENABLE" => write!(w, "* ENABLED {argomenti}\r\n{etichetta} OK fatto\r\n")?,
            "NAMESPACE" => write!(
                w,
                "* NAMESPACE ((\"\" \"/\")) NIL NIL\r\n{etichetta} OK fatto\r\n"
            )?,
            "LIST" => write!(
                w,
                "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n\
                 * LIST (\\HasChildren \\Noselect) \"/\" \"[Gmail]\"\r\n\
                 * LIST (\\All \\HasNoChildren) \"/\" \"[Gmail]/Tutti i messaggi\"\r\n\
                 * LIST (\\HasNoChildren \\Sent) \"/\" \"[Gmail]/Posta inviata\"\r\n\
                 {etichetta} OK fatto\r\n"
            )?,
            "EXAMINE" => {
                let c = &s.casella;
                let mut out = format!(
                    "* FLAGS (\\Answered \\Flagged \\Draft \\Deleted \\Seen)\r\n\
                     * OK [PERMANENTFLAGS ()] sola lettura\r\n\
                     * {} EXISTS\r\n* 0 RECENT\r\n\
                     * OK [UIDVALIDITY {}] valido\r\n\
                     * OK [UIDNEXT {}] previsto\r\n",
                    c.messaggi.len(),
                    c.uidvalidity,
                    c.prossimo_uid
                );
                conosciuti = c.messaggi.len();
                if s.condstore {
                    out.push_str(&format!("* OK [HIGHESTMODSEQ {}] modseq\r\n", c.modseq));
                }
                if let Some(q) = argomenti.to_ascii_uppercase().find("(QRESYNC (") {
                    let numeri: Vec<u64> = argomenti[q + 10..]
                        .split([' ', ')'])
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if numeri.len() >= 2 && numeri[0] == c.uidvalidity as u64 {
                        let m = numeri[1];
                        let spariti: Vec<String> = c
                            .rimossi
                            .iter()
                            .filter(|(_, ms)| *ms > m)
                            .map(|(u, _)| u.to_string())
                            .collect();
                        if !spariti.is_empty() {
                            out.push_str(&format!(
                                "* VANISHED (EARLIER) {}\r\n",
                                spariti.join(",")
                            ));
                        }
                        for (i, msg) in c.messaggi.iter().enumerate() {
                            if msg.modseq > m {
                                out.push_str(&format!(
                                    "* {} FETCH (UID {} FLAGS {} MODSEQ ({}))\r\n",
                                    i + 1,
                                    msg.uid,
                                    flag(&msg.flags),
                                    msg.modseq
                                ));
                            }
                        }
                    }
                }
                let modo = if s.guasti.examine_in_scrittura {
                    "READ-WRITE"
                } else {
                    "READ-ONLY"
                };
                out.push_str(&format!("{etichetta} OK [{modo}] EXAMINE fatto\r\n"));
                w.write_all(out.as_bytes())?;
            }
            "NOOP" => {
                let n = s.casella.messaggi.len();
                if n != conosciuti {
                    write!(w, "* {n} EXISTS\r\n")?;
                    conosciuti = n;
                }
                write!(w, "{etichetta} OK fatto\r\n")?;
            }
            "IDLE" => {
                write!(w, "+ idling\r\n")?;
                let muto = if s.guasti.idle_muto > 0 {
                    s.guasti.idle_muto -= 1;
                    true
                } else {
                    false
                };
                drop(s);
                r.get_ref()
                    .set_read_timeout(Some(Duration::from_millis(50)))?;
                let mut riga = String::new();
                loop {
                    match r.read_line(&mut riga) {
                        Ok(0) => return Ok(()),
                        Ok(_) => {
                            if riga.ends_with('\n') {
                                let fatto = riga.trim().eq_ignore_ascii_case("DONE");
                                stato.lock().unwrap().comandi.push(riga.trim().to_string());
                                riga.clear();
                                if fatto {
                                    break;
                                }
                            }
                        }
                        Err(e)
                            if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                        Err(e) => return Err(e),
                    }
                    if !muto {
                        let n = stato.lock().unwrap().casella.messaggi.len();
                        if n != conosciuti {
                            write!(w, "* {n} EXISTS\r\n")?;
                            conosciuti = n;
                        }
                    }
                }
                if muto {
                    // Morto in silenzio: non risponde più a niente, ma non chiude.
                    loop {
                        let mut x = String::new();
                        match r.read_line(&mut x) {
                            Ok(0) => return Ok(()),
                            Ok(_) => {}
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    ErrorKind::WouldBlock | ErrorKind::TimedOut
                                ) => {}
                            Err(_) => return Ok(()),
                        }
                    }
                }
                r.get_ref().set_read_timeout(None)?;
                write!(w, "{etichetta} OK IDLE finito\r\n")?;
            }
            "LOGOUT" => {
                write!(w, "* BYE arrivederci\r\n{etichetta} OK fatto\r\n")?;
                return Ok(());
            }
            "UID" => {
                let (sotto, resto) = argomenti.split_once(' ').unwrap_or((argomenti, ""));
                match sotto.to_ascii_uppercase().as_str() {
                    "SEARCH" => {
                        let uid: Vec<String> = s
                            .casella
                            .messaggi
                            .iter()
                            .map(|m| m.uid.to_string())
                            .collect();
                        write!(w, "* SEARCH {}\r\n{etichetta} OK fatto\r\n", uid.join(" "))?;
                    }
                    "FETCH" => {
                        if !uid_fetch(&mut w, &mut s, etichetta, resto)? {
                            let _ = w.shutdown(Shutdown::Both);
                            return Ok(());
                        }
                    }
                    _ => write!(w, "{etichetta} BAD non previsto\r\n")?,
                }
            }
            _ => write!(
                w,
                "{etichetta} BAD comando non previsto dal finto server\r\n"
            )?,
        }
    }
}

/// Risponde a UID FETCH. Falso se il guasto «cadi_dopo» ha chiuso la connessione.
fn uid_fetch(
    w: &mut TcpStream,
    s: &mut Stato,
    etichetta: &str,
    argomenti: &str,
) -> std::io::Result<bool> {
    let (insieme, resto) = argomenti.split_once(' ').unwrap_or((argomenti, ""));
    let su = resto.to_ascii_uppercase();
    let da_quando: Option<u64> = su.find("CHANGEDSINCE ").and_then(|i| {
        su[i + 13..]
            .split([' ', ')'])
            .next()
            .and_then(|x| x.parse().ok())
    });
    let vuole = |x: &str| su.contains(x);
    let intestazioni = su.find("BODY.PEEK[HEADER.FIELDS").map(|i| {
        let fine = su[i..].find(']').map(|f| i + f + 1).unwrap_or(su.len());
        resto[i..fine]
            .replacen("BODY.PEEK", "BODY", 1)
            .replacen("body.peek", "BODY", 1)
    });
    // Parti chieste: BODY.PEEK[1]<0.N>, BODY.PEEK[1.2]<0.N>
    let mut parti = Vec::new();
    let mut da = 0;
    while let Some(i) = su[da..].find("BODY.PEEK[") {
        let inizio = da + i + 10;
        let fine = su[inizio..]
            .find(']')
            .map(|f| inizio + f)
            .unwrap_or(su.len());
        let nome = &su[inizio..fine];
        if !nome.is_empty() && nome.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            parti.push(nome.to_string());
        }
        da = fine;
    }
    let strutture = vuole("BODYSTRUCTURE");
    let massimo = s.casella.messaggi.iter().map(|m| m.uid).max().unwrap_or(0);
    let mut emesse = 0;
    let mut out = Vec::new();
    for (i, m) in s.casella.messaggi.iter().enumerate() {
        if !uid_nell_insieme(insieme, m.uid, massimo) {
            continue;
        }
        if da_quando.is_some_and(|d| m.modseq <= d) {
            continue;
        }
        if strutture && s.guasti.cadi_dopo == Some(emesse) {
            s.guasti.cadi_dopo = None;
            w.write_all(&out)?;
            w.flush()?;
            return Ok(false);
        }
        if intestazioni.is_some() && s.guasti.malformati.contains(&m.uid) && strutture {
            out.extend_from_slice(
                format!("* {} FETCH (UID {} FLAGS (\\Seen \r\n", i + 1, m.uid).as_bytes(),
            );
            emesse += 1;
            continue;
        }
        let mut campi: Vec<Vec<u8>> = vec![format!("UID {}", m.uid).into_bytes()];
        if vuole("FLAGS") {
            campi.push(format!("FLAGS {}", flag(&m.flags)).into_bytes());
        }
        if vuole("INTERNALDATE") {
            campi.push(b"INTERNALDATE \"27-Oct-2026 10:00:00 +0100\"".to_vec());
        }
        if vuole("RFC822.SIZE") {
            campi.push(format!("RFC822.SIZE {}", m.dimensione()).into_bytes());
        }
        if vuole("MODSEQ") {
            campi.push(format!("MODSEQ ({})", m.modseq).into_bytes());
        }
        if s.gmail && vuole("X-GM-MSGID") {
            campi.push(format!("X-GM-MSGID {}", m.msgid).into_bytes());
        }
        if s.gmail && vuole("X-GM-THRID") {
            campi.push(format!("X-GM-THRID {}", m.thrid).into_bytes());
        }
        if s.gmail && vuole("X-GM-LABELS") {
            campi.push(format!("X-GM-LABELS {}", etichette(&m.etichette)).into_bytes());
        }
        if strutture {
            campi.push(format!("BODYSTRUCTURE {}", m.struttura()).into_bytes());
        }
        if let Some(sezione) = &intestazioni {
            let mut c = format!("{sezione} {{{}}}\r\n", m.intestazioni.len()).into_bytes();
            c.extend_from_slice(m.intestazioni.as_bytes());
            campi.push(c);
        }
        for p in &parti {
            let dati = if p == "1" || p == "1.1" {
                m.testo.clone()
            } else {
                "QUJD".into()
            };
            let mut c = format!("BODY[{p}]<0> {{{}}}\r\n", dati.len()).into_bytes();
            c.extend_from_slice(dati.as_bytes());
            campi.push(c);
        }
        out.extend_from_slice(format!("* {} FETCH (", i + 1).as_bytes());
        out.extend_from_slice(&campi.join(&b' '));
        out.extend_from_slice(b")\r\n");
        emesse += 1;
    }
    out.extend_from_slice(format!("{etichetta} OK FETCH fatto\r\n").as_bytes());
    w.write_all(&out)?;
    Ok(true)
}
