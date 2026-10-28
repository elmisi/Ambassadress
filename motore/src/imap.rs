//! Il dialogo IMAP: comandi con etichetta, risposte analizzate con `imap-proto`,
//! e le poche operazioni che servono al motore. Tutti i comandi passano da
//! `Sessione::esegui` e da lì da `Connessione::invia`, cioè dalla guardia.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use imap_proto::{
    AttributeValue, MailboxDatum, NameAttribute, Response, ResponseCode, SectionPath, Status,
};

use crate::errore::{Errore, Risultato};
use crate::rete::{Connessione, Contatori, Indirizzo};
use crate::utf7;

/// Una risposta del server durante un comando.
pub enum Elemento<'a> {
    Risposta(Response<'a>),
    /// Una risposta che il parser non riconosce: si conta e si va avanti.
    Malformata {
        byte: usize,
    },
}

/// L'esito positivo di un comando.
#[derive(Debug, Default)]
pub struct Completato {
    pub codice: Option<ResponseCode<'static>>,
}

#[derive(Debug, Clone)]
pub struct CartellaServer {
    pub nome: String,
    pub tutti: bool,
    pub selezionabile: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum ModoExamine {
    Semplice,
    Condstore,
    Qresync { uidvalidity: u32, modseq: u64 },
}

#[derive(Debug, Default)]
pub struct StatoCartella {
    pub esistenti: u32,
    pub uidvalidity: Option<u32>,
    pub uidnext: Option<u32>,
    pub highestmodseq: Option<u64>,
    pub nomodseq: bool,
    /// PERMANENTFLAGS come li dichiara il server dopo EXAMINE.
    pub permanentflags: Option<Vec<String>>,
    /// Intervalli di UID spariti (QRESYNC, VANISHED (EARLIER)).
    pub spariti: Vec<std::ops::RangeInclusive<u32>>,
    /// Flag cambiati dall'ultima volta (QRESYNC).
    pub aggiornamenti: Vec<DatiFetch>,
}

/// Quello che interessa di una risposta FETCH, già copiato.
#[derive(Debug, Default, Clone)]
pub struct DatiFetch {
    pub uid: Option<u32>,
    pub flags: Option<Vec<String>>,
    pub data_interna: Option<i64>,
    pub dimensione: Option<u32>,
    pub modseq: Option<u64>,
    pub gm_msgid: Option<u64>,
    pub gm_thrid: Option<u64>,
    pub etichette: Option<Vec<String>>,
    pub intestazioni: Option<Vec<u8>>,
    pub struttura: Option<Vec<crate::mime::Parte>>,
    /// Sezioni del corpo per numero di parte ("1", "1.2").
    pub sezioni: Vec<(String, Vec<u8>)>,
}

impl DatiFetch {
    pub fn da(attributi: &[AttributeValue<'_>]) -> Self {
        let mut d = DatiFetch::default();
        for a in attributi {
            match a {
                AttributeValue::Uid(u) => d.uid = Some(*u),
                AttributeValue::Flags(f) => {
                    d.flags = Some(f.iter().map(|x| x.to_string()).collect())
                }
                AttributeValue::InternalDate(s) => {
                    d.data_interna = crate::tempo::da_internaldate(s)
                }
                AttributeValue::Rfc822Size(n) => d.dimensione = Some(*n),
                AttributeValue::ModSeq(m) => d.modseq = Some(*m),
                AttributeValue::GmailMsgId(m) => d.gm_msgid = Some(*m),
                AttributeValue::GmailThrId(t) => d.gm_thrid = Some(*t),
                AttributeValue::GmailLabels(l) => {
                    d.etichette = Some(l.iter().map(|x| utf7::decodifica(x)).collect());
                }
                AttributeValue::BodyStructure(b) => {
                    d.struttura = Some(crate::mime::parti_da_struttura(b))
                }
                AttributeValue::Rfc822Header(h) => {
                    d.intestazioni = Some(h.as_deref().unwrap_or_default().to_vec());
                }
                AttributeValue::BodySection { section, data, .. } => {
                    let dati = data.as_deref().unwrap_or_default().to_vec();
                    match section {
                        Some(SectionPath::Full(imap_proto::MessageSection::Header)) => {
                            d.intestazioni = Some(dati)
                        }
                        Some(SectionPath::Part(numeri, None)) => {
                            let nome = numeri
                                .iter()
                                .map(|n| n.to_string())
                                .collect::<Vec<_>>()
                                .join(".");
                            d.sezioni.push((nome, dati));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        d
    }
}

/// Novità che arrivano durante IDLE o NOOP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Novita {
    Esistenti(u32),
    Cancellato,
    Cambiato,
}

fn novita_da(r: &Response<'_>) -> Option<Novita> {
    match r {
        Response::MailboxData(MailboxDatum::Exists(n)) => Some(Novita::Esistenti(*n)),
        Response::MailboxData(MailboxDatum::Recent(_)) => None,
        Response::Expunge(_) | Response::Vanished { .. } => Some(Novita::Cancellato),
        Response::Fetch(..) => Some(Novita::Cambiato),
        _ => None,
    }
}

/// Mette un argomento tra virgolette. Rifiuta ciò che richiederebbe un letterale.
pub fn quota(s: &str) -> Risultato<String> {
    if s.bytes()
        .any(|b| b == b'\r' || b == b'\n' || b == 0 || !b.is_ascii())
    {
        return Err(Errore::Uso(
            "argomento non rappresentabile tra virgolette (caratteri non ASCII o a capo)".into(),
        ));
    }
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// Comprime una lista di UID crescenti in un insieme IMAP: `1:5,7,9:12`.
pub fn insieme_uid(uid: &[u32]) -> String {
    let mut v: Vec<u32> = uid.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut out = String::new();
    let mut i = 0;
    while i < v.len() {
        let inizio = v[i];
        let mut fine = inizio;
        while i + 1 < v.len() && v[i + 1] == fine + 1 {
            i += 1;
            fine = v[i];
        }
        if !out.is_empty() {
            out.push(',');
        }
        if inizio == fine {
            out.push_str(&inizio.to_string());
        } else {
            out.push_str(&format!("{inizio}:{fine}"));
        }
        i += 1;
    }
    out
}

pub struct Sessione {
    conn: Connessione,
    etichetta: u32,
    pub timeout: Duration,
    capacita: Vec<String>,
    abilitate: HashSet<String>,
}

impl Sessione {
    /// Apre la connessione e legge il saluto del server.
    pub fn connetti(
        indirizzo: &Indirizzo,
        timeout: Duration,
        contatori: Arc<Contatori>,
    ) -> Risultato<Self> {
        let mut conn = Connessione::apri(indirizzo, timeout, contatori)?;
        let saluto = conn.leggi_risposta(Instant::now() + timeout)?;
        let mut capacita = Vec::new();
        match Response::parse(&saluto) {
            Ok((
                _,
                Response::Data {
                    status: Status::Ok | Status::PreAuth,
                    outcome,
                },
            )) => {
                if let Some(ResponseCode::Capabilities(c)) = outcome.code {
                    capacita = c.iter().map(nome_capacita).collect();
                }
            }
            Ok((
                _,
                Response::Data {
                    status: Status::Bye,
                    ..
                },
            )) => return Err(Errore::Chiusa),
            _ => {
                return Err(Errore::Protocollo(
                    "saluto del server non riconosciuto".into(),
                ));
            }
        }
        Ok(Sessione {
            conn,
            etichetta: 0,
            timeout,
            capacita,
            abilitate: HashSet::new(),
        })
    }

    pub fn tls(&self) -> Option<&str> {
        self.conn.tls.as_deref()
    }

    pub fn capacita(&self) -> &[String] {
        &self.capacita
    }

    pub fn ha(&self, c: &str) -> bool {
        self.capacita.iter().any(|x| x.eq_ignore_ascii_case(c))
    }

    /// Vero se l'estensione è attiva in questa sessione.
    pub fn abilitata(&self, c: &str) -> bool {
        let c = c.to_ascii_uppercase();
        self.abilitate.contains(&c) || (c == "CONDSTORE" && self.ha("CONDSTORE"))
    }

    fn prossima_etichetta(&mut self) -> String {
        self.etichetta += 1;
        format!("a{}", self.etichetta)
    }

    /// Manda un comando e passa ogni risposta senza etichetta a `f`, fino alla
    /// risposta con la sua etichetta. NO e BAD diventano `Errore::Server`.
    pub fn esegui<F>(&mut self, comando: &str, mut f: F) -> Risultato<Completato>
    where
        F: FnMut(Elemento<'_>) -> Risultato<()>,
    {
        let etichetta = self.prossima_etichetta();
        let nome_comando = nome_del_comando(comando);
        self.conn
            .invia(format!("{etichetta} {comando}\r\n").as_bytes())?;
        let mut bye = false;
        loop {
            let byte = match self.conn.leggi_risposta(Instant::now() + self.timeout) {
                Ok(b) => b,
                Err(Errore::Chiusa) if bye => return Err(Errore::Chiusa),
                Err(e) => return Err(e),
            };
            match Response::parse(&byte) {
                Ok((
                    _,
                    Response::Done {
                        tag,
                        status,
                        outcome,
                    },
                )) => {
                    if tag.0 != etichetta {
                        continue;
                    }
                    return match status {
                        Status::Ok => Ok(Completato {
                            codice: outcome.code.map(|c| c.into_owned()),
                        }),
                        altro => Err(Errore::Server {
                            comando: nome_comando,
                            stato: format!("{altro:?}").to_uppercase(),
                            testo: outcome
                                .information
                                .map(|s| s.to_string())
                                .unwrap_or_default(),
                        }),
                    };
                }
                Ok((
                    _,
                    Response::Data {
                        status: Status::Bye,
                        ..
                    },
                )) => {
                    bye = true;
                }
                Ok((_, r)) => f(Elemento::Risposta(r))?,
                Err(_) => f(Elemento::Malformata { byte: byte.len() })?,
            }
        }
    }

    pub fn aggiorna_capacita(&mut self) -> Risultato<()> {
        let mut nuove = Vec::new();
        self.esegui("CAPABILITY", |e| {
            if let Elemento::Risposta(Response::Capabilities(c)) = e {
                nuove = c.iter().map(nome_capacita).collect();
            }
            Ok(())
        })?;
        self.capacita = nuove;
        Ok(())
    }

    /// LOGIN. Un rifiuto diventa `Errore::Accesso`, senza ripetere il tentativo.
    pub fn login(&mut self, utente: &str, password: &str) -> Risultato<()> {
        let comando = format!("LOGIN {} {}", quota(utente)?, quota(password)?);
        match self.esegui(&comando, |_| Ok(())) {
            Ok(c) => {
                if let Some(ResponseCode::Capabilities(c)) = c.codice {
                    self.capacita = c.iter().map(nome_capacita).collect();
                } else {
                    self.aggiorna_capacita()?;
                }
                Ok(())
            }
            Err(Errore::Server { testo, .. }) => Err(Errore::Accesso(testo)),
            Err(e) => Err(e),
        }
    }

    pub fn id(&mut self) -> Risultato<()> {
        if self.ha("ID") {
            let versione = env!("CARGO_PKG_VERSION");
            self.esegui(
                &format!("ID (\"name\" \"Ambassadress\" \"version\" \"{versione}\")"),
                |_| Ok(()),
            )?;
        }
        Ok(())
    }

    /// ENABLE QRESYNC se c'è, altrimenti ENABLE CONDSTORE.
    pub fn abilita_estensioni(&mut self) -> Risultato<()> {
        if !self.ha("ENABLE") {
            return Ok(());
        }
        let richiesta = if self.ha("QRESYNC") {
            "ENABLE QRESYNC"
        } else if self.ha("CONDSTORE") {
            "ENABLE CONDSTORE"
        } else {
            return Ok(());
        };
        let mut abilitate = HashSet::new();
        self.esegui(richiesta, |e| {
            if let Elemento::Risposta(Response::Capabilities(c)) = e {
                abilitate.extend(c.iter().map(nome_capacita).map(|s| s.to_ascii_uppercase()));
            }
            Ok(())
        })?;
        if abilitate.contains("QRESYNC") {
            abilitate.insert("CONDSTORE".into());
        }
        self.abilitate = abilitate;
        Ok(())
    }

    pub fn elenca_cartelle(&mut self) -> Risultato<Vec<CartellaServer>> {
        let mut out = Vec::new();
        self.esegui("LIST \"\" \"*\"", |e| {
            if let Elemento::Risposta(Response::MailboxData(MailboxDatum::List(l))) = e {
                out.push(CartellaServer {
                    nome: l.name.to_string(),
                    tutti: l.name_attributes.contains(&NameAttribute::All),
                    selezionabile: !l.name_attributes.contains(&NameAttribute::NoSelect),
                });
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Apre la cartella in sola lettura. Se il server la apre in scrittura ci si ferma.
    pub fn examine(&mut self, cartella: &str, modo: ModoExamine) -> Risultato<StatoCartella> {
        let nome = quota(cartella)?;
        let comando = match modo {
            ModoExamine::Semplice => format!("EXAMINE {nome}"),
            ModoExamine::Condstore => format!("EXAMINE {nome} (CONDSTORE)"),
            ModoExamine::Qresync {
                uidvalidity,
                modseq,
            } => {
                format!("EXAMINE {nome} (QRESYNC ({uidvalidity} {modseq}))")
            }
        };
        let mut st = StatoCartella::default();
        let esito = self.esegui(&comando, |e| {
            if let Elemento::Risposta(r) = e {
                match r {
                    Response::MailboxData(MailboxDatum::Exists(n)) => st.esistenti = n,
                    Response::Data {
                        status: Status::Ok,
                        outcome,
                    } => match outcome.code {
                        Some(ResponseCode::UidValidity(v)) => st.uidvalidity = Some(v),
                        Some(ResponseCode::UidNext(n)) => st.uidnext = Some(n),
                        Some(ResponseCode::HighestModSeq(m)) => st.highestmodseq = Some(m),
                        Some(ResponseCode::PermanentFlags(f)) => {
                            st.permanentflags = Some(f.iter().map(|x| x.to_string()).collect());
                        }
                        None => {
                            if outcome
                                .information
                                .is_some_and(|t| t.to_ascii_uppercase().starts_with("[NOMODSEQ]"))
                            {
                                st.nomodseq = true;
                            }
                        }
                        _ => {}
                    },
                    Response::Vanished { uids, .. } => st.spariti.extend(uids),
                    Response::Fetch(_, attributi) => {
                        st.aggiornamenti.push(DatiFetch::da(&attributi))
                    }
                    _ => {}
                }
            }
            Ok(())
        })?;
        if matches!(esito.codice, Some(ResponseCode::ReadWrite)) {
            return Err(Errore::Protocollo(
                "il server ha aperto la cartella in scrittura dopo EXAMINE".into(),
            ));
        }
        Ok(st)
    }

    pub fn uid_search_tutti(&mut self) -> Risultato<Vec<u32>> {
        let mut uid = Vec::new();
        self.esegui("UID SEARCH ALL", |e| {
            if let Elemento::Risposta(Response::MailboxData(MailboxDatum::Search(v))) = e {
                uid.extend(v);
            }
            Ok(())
        })?;
        uid.sort_unstable();
        uid.dedup();
        Ok(uid)
    }

    /// UID FETCH. Ogni risposta passa a `f` come `DatiFetch`, oppure come `None` se malformata.
    pub fn uid_fetch<F>(
        &mut self,
        insieme: &str,
        attributi: &str,
        modificatori: Option<&str>,
        mut f: F,
    ) -> Risultato<()>
    where
        F: FnMut(Option<DatiFetch>),
    {
        let comando = match modificatori {
            Some(m) => format!("UID FETCH {insieme} {attributi} {m}"),
            None => format!("UID FETCH {insieme} {attributi}"),
        };
        self.esegui(&comando, |e| {
            match e {
                Elemento::Risposta(Response::Fetch(_, attributi)) => {
                    f(Some(DatiFetch::da(&attributi)))
                }
                Elemento::Malformata { .. } => f(None),
                _ => {}
            }
            Ok(())
        })?;
        Ok(())
    }

    pub fn noop(&mut self) -> Risultato<Vec<Novita>> {
        let mut novita = Vec::new();
        self.esegui("NOOP", |e| {
            if let Elemento::Risposta(r) = e
                && let Some(n) = novita_da(&r)
            {
                novita.push(n);
            }
            Ok(())
        })?;
        Ok(novita)
    }

    /// Manda IDLE e aspetta la continuazione `+`. Restituisce l'etichetta.
    pub fn idle_inizia(&mut self) -> Risultato<(String, Vec<Novita>)> {
        let etichetta = self.prossima_etichetta();
        self.conn
            .invia(format!("{etichetta} IDLE\r\n").as_bytes())?;
        let mut novita = Vec::new();
        loop {
            let byte = self.conn.leggi_risposta(Instant::now() + self.timeout)?;
            match Response::parse(&byte) {
                Ok((_, Response::Continue(_))) => return Ok((etichetta, novita)),
                Ok((
                    _,
                    Response::Done {
                        tag,
                        status,
                        outcome,
                    },
                )) if tag.0 == etichetta => {
                    self.conn.annulla_idle();
                    return Err(Errore::Server {
                        comando: "IDLE".into(),
                        stato: format!("{status:?}").to_uppercase(),
                        testo: outcome
                            .information
                            .map(|s| s.to_string())
                            .unwrap_or_default(),
                    });
                }
                Ok((
                    _,
                    Response::Data {
                        status: Status::Bye,
                        ..
                    },
                )) => return Err(Errore::Chiusa),
                Ok((_, r)) => novita.extend(novita_da(&r)),
                Err(_) => {}
            }
        }
    }

    /// Aspetta novità durante IDLE fino alla scadenza. Vuoto se non arriva niente.
    pub fn idle_attendi(&mut self, scadenza: Instant) -> Risultato<Vec<Novita>> {
        let mut novita = Vec::new();
        loop {
            let byte = match self.conn.leggi_risposta(scadenza) {
                Ok(b) => b,
                Err(Errore::Timeout) => return Ok(novita),
                Err(e) => return Err(e),
            };
            match Response::parse(&byte) {
                Ok((
                    _,
                    Response::Data {
                        status: Status::Bye,
                        ..
                    },
                )) => return Err(Errore::Chiusa),
                Ok((_, r)) => novita.extend(novita_da(&r)),
                Err(_) => {}
            }
            if !novita.is_empty() {
                return Ok(novita);
            }
        }
    }

    /// Manda DONE e aspetta la fine di IDLE entro `entro`: se non arriva, la connessione è morta.
    pub fn idle_termina(&mut self, etichetta: &str, entro: Duration) -> Risultato<Vec<Novita>> {
        self.conn.invia(b"DONE\r\n")?;
        let scadenza = Instant::now() + entro;
        let mut novita = Vec::new();
        loop {
            let byte = self.conn.leggi_risposta(scadenza)?;
            match Response::parse(&byte) {
                Ok((
                    _,
                    Response::Done {
                        tag,
                        status,
                        outcome,
                    },
                )) if tag.0 == etichetta => {
                    return match status {
                        Status::Ok => Ok(novita),
                        altro => Err(Errore::Server {
                            comando: "IDLE".into(),
                            stato: format!("{altro:?}").to_uppercase(),
                            testo: outcome
                                .information
                                .map(|s| s.to_string())
                                .unwrap_or_default(),
                        }),
                    };
                }
                Ok((
                    _,
                    Response::Data {
                        status: Status::Bye,
                        ..
                    },
                )) => return Err(Errore::Chiusa),
                Ok((_, r)) => novita.extend(novita_da(&r)),
                Err(_) => {}
            }
        }
    }

    /// LOGOUT di cortesia: gli errori non contano.
    pub fn logout(mut self) {
        let vecchio = self.timeout;
        self.timeout = vecchio.min(Duration::from_secs(5));
        let _ = self.esegui("LOGOUT", |_| Ok(()));
        self.conn.chiudi();
    }

    /// Chiude il socket senza salutare (connessione già rotta).
    pub fn abbandona(mut self) {
        self.conn.chiudi();
    }
}

fn nome_capacita(c: &imap_proto::Capability<'_>) -> String {
    match c {
        imap_proto::Capability::Imap4rev1 => "IMAP4rev1".into(),
        imap_proto::Capability::Auth(a) => format!("AUTH={a}"),
        imap_proto::Capability::Atom(a) => a.to_string(),
    }
}

/// Il nome del comando (`UID FETCH`, `LOGIN`…), mai gli argomenti.
fn nome_del_comando(comando: &str) -> String {
    let mut parole = comando.split(' ');
    let primo = parole.next().unwrap_or("").to_ascii_uppercase();
    if primo == "UID" {
        format!("UID {}", parole.next().unwrap_or("").to_ascii_uppercase())
    } else {
        primo
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn insiemi() {
        assert_eq!(insieme_uid(&[1, 2, 3, 5, 7, 8, 9, 20]), "1:3,5,7:9,20");
        assert_eq!(insieme_uid(&[4]), "4");
        assert_eq!(insieme_uid(&[9, 8, 7, 7]), "7:9");
    }

    #[test]
    fn virgolette() {
        assert_eq!(
            quota("[Gmail]/Tutti i messaggi").unwrap(),
            "\"[Gmail]/Tutti i messaggi\""
        );
        assert_eq!(quota("a\"b\\c").unwrap(), "\"a\\\"b\\\\c\"");
        assert!(quota("a\r\nb").is_err());
    }
}
