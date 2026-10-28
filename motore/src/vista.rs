//! La vista per l'interfaccia: legge il database e scrive una stringa JSON nello
//! stesso formato di `window.AMB_DATA` del prototipo (`mockups/data/README.md`),
//! così l'interfaccia della v13 la usa senza cambiare.
//!
//! - `id` è l'identità stabile del messaggio (`gm:<X-GM-MSGID>` su Gmail), che non
//!   cambia con UIDVALIDITY; `localId` è l'id nel database, quello che vogliono
//!   `su_richiesta::parte` e `su_richiesta::scarica_testi`.
//! - L'`url` di un allegato è `prefisso + localId + "/" + numero di parte`; nell'HTML
//!   i riferimenti `cid:` alle immagini incorporate diventano lo stesso url.
//! - Per le parti già scaricate: `width` e `height` in pixel per le immagini che si
//!   leggono, `pages` per i PDF, `thumb` per i PDF con la miniatura della prima
//!   pagina (l'url dell'allegato con `miniatura/` al posto di `allegato/`). Mancano
//!   quando non si sanno.
//! - Un messaggio di cui non si è ancora scaricato il testo ha `text` vuoto e
//!   `"bodyMissing": true`.
//! - I messaggi eliminati altrove non ci sono.

use std::collections::HashMap;

use rusqlite::params;

use crate::db::Db;
use crate::errore::Risultato;
use crate::json::{Scrittore, stringa};
use crate::sincronizza::Config;
use crate::tempo;

/// L'account, dalla configurazione dell'app (non dal contenuto della posta).
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub indirizzo: String,
    pub nome: String,
    pub alias: Vec<String>,
}

/// Una casella collegata: `tipo` è "gmail" o "imap".
#[derive(Clone, Debug, Default)]
pub struct Casella {
    pub indirizzo: String,
    pub tipo: String,
    pub server: String,
}

#[derive(Clone, Debug, Default)]
pub struct OpzioniVista {
    pub account: Account,
    pub caselle: Vec<Casella>,
    /// Inizio dell'url degli allegati, per esempio `a10s://localhost/allegato/`.
    pub prefisso_allegati: String,
    /// Solo gli N messaggi più recenti.
    pub limite: Option<usize>,
    /// Solo questi messaggi (id locali), per aggiornare l'interfaccia dopo
    /// `scarica_testi` senza rileggere tutto.
    pub solo: Option<Vec<i64>>,
}

impl Account {
    /// L'account dalla configurazione: l'indirizzo è l'utente IMAP, il nome lo dà l'app.
    pub fn da_config(cfg: &Config, nome: &str, alias: &[String]) -> Account {
        Account {
            indirizzo: cfg.credenziali.utente.esponi().to_string(),
            nome: nome.to_string(),
            alias: alias.to_vec(),
        }
    }
}

impl Casella {
    /// La casella dalla configurazione: "gmail" se il server ha offerto X-GM-EXT-1
    /// all'ultima sincronizzazione.
    pub fn da_config(cfg: &Config, db: &Db) -> Risultato<Casella> {
        let gmail = db
            .meta("capacita")?
            .is_some_and(|c| c.split(' ').any(|x| x.eq_ignore_ascii_case("X-GM-EXT-1")));
        Ok(Casella {
            indirizzo: cfg.credenziali.utente.esponi().to_string(),
            tipo: if gmail { "gmail" } else { "imap" }.into(),
            server: cfg.indirizzo.host.clone(),
        })
    }
}

#[derive(Default)]
struct Persone {
    da: Vec<(String, String)>,
    a: Vec<(String, String)>,
    cc: Vec<(String, String)>,
    ccn: Vec<(String, String)>,
}

struct ParteVista {
    parte: String,
    tipo: String,
    byte: i64,
    codifica: Option<String>,
    nome_file: Option<String>,
    content_id: Option<String>,
    ruolo: String,
    scaricato: Option<i64>,
    larghezza: Option<i64>,
    altezza: Option<i64>,
    pagine: Option<i64>,
    miniatura: bool,
}

struct Riga {
    id: i64,
    chiave: String,
    message_id: Option<String>,
    in_reply_to: Option<String>,
    riferimenti: Option<String>,
    oggetto: Option<String>,
    data: Option<i64>,
    data_interna: Option<i64>,
    letto: bool,
    importante: bool,
    corpo: i64,
    struttura: bool,
    thread: Option<String>,
    gm_thrid: Option<i64>,
    testo: Option<String>,
    html: Option<String>,
    troncato: bool,
}

/// Tra parentesi angolari, come nell'intestazione.
fn angolari(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('<') {
        s.to_string()
    } else {
        format!("<{s}>")
    }
}

/// Dimensione decodificata stimata dalla dimensione sul server.
pub(crate) fn decodificati(byte: i64, codifica: Option<&str>) -> i64 {
    match codifica {
        // 76 caratteri e CRLF per riga, 4 caratteri ogni 3 byte.
        Some(c) if c.eq_ignore_ascii_case("base64") => byte * 57 / 78,
        _ => byte,
    }
}

/// L'inizio dell'url delle miniature: quello degli allegati con `miniatura/` al
/// posto di `allegato/` finale (`a10s://localhost/miniatura/`). Con un prefisso
/// diverso le miniature non si possono indicare e `thumb` manca.
fn prefisso_miniature(allegati: &str) -> Option<String> {
    allegati
        .strip_suffix("allegato/")
        .map(|inizio| format!("{inizio}miniatura/"))
}

/// Sostituisce `cid:<id>` con l'url della parte, nell'HTML.
fn riscrivi_cid(html: &str, cid: &HashMap<String, String>) -> String {
    if cid.is_empty() {
        return html.to_string();
    }
    let basso = html.to_ascii_lowercase();
    if !basso.contains("cid:") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while let Some(p) = basso[i..].find("cid:") {
        let inizio = i + p;
        out.push_str(&html[i..inizio]);
        let resto = &html[inizio + 4..];
        let fine = resto
            .find(['"', '\'', ')', '>', ' ', '\t', '\r', '\n'])
            .unwrap_or(resto.len());
        let id = &resto[..fine];
        let decodificato = percento(id);
        match cid.get(id).or_else(|| cid.get(&decodificato)) {
            Some(url) => out.push_str(url),
            None => out.push_str(&html[inizio..inizio + 4 + fine]),
        }
        i = inizio + 4 + fine;
    }
    out.push_str(&html[i..]);
    out
}

fn percento(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(n) =
                u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
        {
            out.push(n);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn persona(out: &mut String, nome: &str, indirizzo: &str) {
    let mut o = Scrittore::oggetto(out);
    o.testo("name", nome);
    o.testo("address", indirizzo);
    o.chiudi_oggetto();
}

fn lista_persone(out: &mut String, v: &[(String, String)]) {
    let mut l = Scrittore::lista(out);
    for (n, a) in v {
        persona(l.elemento(), n, a);
    }
    l.chiudi_lista();
}

/// La vista come stringa JSON (vedi la documentazione del modulo).
pub fn json(db: &Db, opz: &OpzioniVista) -> Risultato<String> {
    let conn = db.conn();
    let filtro = match &opz.solo {
        Some(ids) => {
            let lista: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
            format!(" AND m.id IN ({})", lista.join(","))
        }
        None => String::new(),
    };
    let sql = format!(
        "SELECT m.id, m.chiave, m.message_id, m.in_reply_to, m.riferimenti, m.oggetto, m.data,
                m.data_interna, m.letto, m.importante, m.corpo, m.struttura, t.chiave, t.gm_thrid,
                c.testo, c.html, c.troncato
         FROM messaggi m LEFT JOIN thread t ON t.id = m.thread_id
         LEFT JOIN corpi c ON c.messaggio_id = m.id
         WHERE m.eliminato IS NULL{filtro}
         ORDER BY COALESCE(m.data_interna, m.data) DESC, m.id DESC
         LIMIT ?1"
    );
    let limite = opz.limite.map_or(-1, |l| l as i64);
    let mut righe: Vec<Riga> = conn
        .prepare(&sql)?
        .query_map(params![limite], |r| {
            Ok(Riga {
                id: r.get(0)?,
                chiave: r.get(1)?,
                message_id: r.get(2)?,
                in_reply_to: r.get(3)?,
                riferimenti: r.get(4)?,
                oggetto: r.get(5)?,
                data: r.get(6)?,
                data_interna: r.get(7)?,
                letto: r.get(8)?,
                importante: r.get(9)?,
                corpo: r.get(10)?,
                struttura: r.get(11)?,
                thread: r.get(12)?,
                gm_thrid: r.get(13)?,
                testo: r.get(14)?,
                html: r.get(15)?,
                troncato: r.get::<_, Option<bool>>(16)?.unwrap_or(false),
            })
        })?
        .collect::<Result<_, _>>()?;
    // In ordine di data, dal più vecchio, come nei dati del prototipo.
    righe.sort_by_key(|r| (r.data.or(r.data_interna).unwrap_or(0), r.id));

    // Le tabelle collegate, solo per i messaggi scelti.
    let scelti = format!(
        "[{}]",
        righe
            .iter()
            .map(|r| r.id.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let dentro = "messaggio_id IN (SELECT value FROM json_each(?1))";

    let mut persone: HashMap<i64, Persone> = HashMap::new();
    {
        let mut st = conn.prepare(&format!(
            "SELECT p.messaggio_id, p.ruolo, p.nome, i.email FROM partecipanti p
             JOIN indirizzi i ON i.id = p.indirizzo_id WHERE p.{dentro}
             ORDER BY p.messaggio_id, p.ruolo, p.posizione"
        ))?;
        let mut q = st.query(params![scelti])?;
        while let Some(r) = q.next()? {
            let id: i64 = r.get(0)?;
            let ruolo: String = r.get(1)?;
            let nome: Option<String> = r.get(2)?;
            let email: String = r.get(3)?;
            let p = persone.entry(id).or_default();
            let v = match ruolo.as_str() {
                "da" => &mut p.da,
                "a" => &mut p.a,
                "cc" => &mut p.cc,
                "ccn" => &mut p.ccn,
                _ => continue,
            };
            v.push((nome.unwrap_or_default(), email));
        }
    }
    let mut etichette: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut st = conn.prepare(&format!(
            "SELECT messaggio_id, nome FROM etichette WHERE {dentro} ORDER BY messaggio_id, nome"
        ))?;
        let mut q = st.query(params![scelti])?;
        while let Some(r) = q.next()? {
            etichette.entry(r.get(0)?).or_default().push(r.get(1)?);
        }
    }
    let mut intestazioni: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    {
        let mut st = conn.prepare(&format!(
            "SELECT messaggio_id, nome, valore FROM intestazioni WHERE {dentro}
             ORDER BY messaggio_id, posizione"
        ))?;
        let mut q = st.query(params![scelti])?;
        while let Some(r) = q.next()? {
            intestazioni
                .entry(r.get(0)?)
                .or_default()
                .push((r.get(1)?, r.get(2)?));
        }
    }
    let mut parti: HashMap<i64, Vec<ParteVista>> = HashMap::new();
    {
        let mut st = conn.prepare(&format!(
            "SELECT p.messaggio_id, p.parte, p.tipo, p.byte, p.codifica, p.nome_file, p.content_id,
                    p.ruolo, a.byte, a.larghezza, a.altezza, a.pagine, a.miniatura
             FROM parti p LEFT JOIN allegati a ON a.messaggio_id = p.messaggio_id AND a.parte = p.parte
             WHERE p.{dentro} AND p.ruolo IN ('allegato', 'incorporato', 'altro')"
        ))?;
        let mut q = st.query(params![scelti])?;
        while let Some(r) = q.next()? {
            parti.entry(r.get(0)?).or_default().push(ParteVista {
                parte: r.get(1)?,
                tipo: r.get(2)?,
                byte: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                codifica: r.get(4)?,
                nome_file: r.get(5)?,
                content_id: r.get(6)?,
                ruolo: r.get(7)?,
                scaricato: r.get(8)?,
                larghezza: r.get(9)?,
                altezza: r.get(10)?,
                pagine: r.get(11)?,
                miniatura: r.get::<_, Option<bool>>(12)?.unwrap_or(false),
            });
        }
    }
    for v in parti.values_mut() {
        v.sort_by(|a, b| {
            let n = |s: &str| -> Vec<u32> { s.split('.').filter_map(|x| x.parse().ok()).collect() };
            n(&a.parte).cmp(&n(&b.parte))
        });
    }

    let corpi: usize = righe
        .iter()
        .map(|r| r.testo.as_ref().map_or(0, |t| t.len()) + r.html.as_ref().map_or(0, |h| h.len()))
        .sum();
    let mut out = String::with_capacity(corpi + righe.len() * 900 + 1024);
    let mut radice = Scrittore::oggetto(&mut out);
    radice.numero("schema", 1);
    {
        let mut m = Scrittore::oggetto(radice.chiave("meta"));
        m.testo("source", "imap");
        m.testo("exportedAt", &tempo::iso(tempo::adesso()));
        m.numero("count", righe.len() as i64);
        m.chiudi_oggetto();
    }
    {
        let mut a = Scrittore::oggetto(radice.chiave("account"));
        a.testo("address", &opz.account.indirizzo);
        a.testo("name", &opz.account.nome);
        let mut l = Scrittore::lista(a.chiave("aliases"));
        for x in &opz.account.alias {
            stringa(l.elemento(), x);
        }
        l.chiudi_lista();
        a.chiudi_oggetto();
    }
    {
        let mut l = Scrittore::lista(radice.chiave("accounts"));
        for c in &opz.caselle {
            let mut o = Scrittore::oggetto(l.elemento());
            o.testo("address", &c.indirizzo);
            o.testo("kind", &c.tipo);
            o.testo("server", &c.server);
            o.chiudi_oggetto();
        }
        l.chiudi_lista();
    }
    let vuote = Persone::default();
    let prefisso_miniature = prefisso_miniature(&opz.prefisso_allegati);
    let mut lista = Scrittore::lista(radice.chiave("messages"));
    for r in &righe {
        let mut m = Scrittore::oggetto(lista.elemento());
        m.testo("id", &r.chiave);
        m.numero("localId", r.id);
        m.testo_o_null("date", r.data.map(tempo::iso).as_deref());
        m.testo_o_null("internalDate", r.data_interna.map(tempo::iso).as_deref());
        let p = persone.get(&r.id).unwrap_or(&vuote);
        match p.da.first() {
            Some((n, a)) => persona(m.chiave("from"), n, a),
            None => m.chiave("from").push_str("null"),
        }
        lista_persone(m.chiave("to"), &p.a);
        lista_persone(m.chiave("cc"), &p.cc);
        lista_persone(m.chiave("bcc"), &p.ccn);
        m.testo_o_null("subject", r.oggetto.as_deref());
        m.testo_o_null(
            "messageId",
            r.message_id.as_deref().map(angolari).as_deref(),
        );
        m.testo_o_null(
            "inReplyTo",
            r.in_reply_to.as_deref().map(angolari).as_deref(),
        );
        {
            let mut l = Scrittore::lista(m.chiave("references"));
            for x in r.riferimenti.as_deref().unwrap_or("").split_whitespace() {
                stringa(l.elemento(), &angolari(x));
            }
            l.chiudi_lista();
        }

        let mie = parti.get(&r.id).map(Vec::as_slice).unwrap_or(&[]);
        let url = |parte: &str| format!("{}{}/{}", opz.prefisso_allegati, r.id, parte);
        let url_miniatura = |parte: &str| {
            prefisso_miniature
                .as_ref()
                .map(|p| format!("{p}{}/{parte}", r.id))
        };
        let cid: HashMap<String, String> = mie
            .iter()
            .filter_map(|p| Some((p.content_id.clone()?, url(&p.parte))))
            .collect();
        let mancante = r.corpo == 0;
        let testo = if mancante {
            String::new()
        } else {
            r.testo.as_deref().unwrap_or("").replace("\r\n", "\n")
        };
        m.testo("text", &testo);
        match (&r.html, mancante) {
            (Some(h), false) => m.testo("html", &riscrivi_cid(h, &cid)),
            _ => m.chiave("html").push_str("null"),
        }
        if mancante {
            m.booleano("bodyMissing", true);
        }
        if r.troncato {
            m.booleano("truncated", true);
        }
        {
            let mut l = Scrittore::lista(m.chiave("attachments"));
            for p in mie.iter().filter(|p| p.ruolo != "altro") {
                let mut a = Scrittore::oggetto(l.elemento());
                a.testo("filename", p.nome_file.as_deref().unwrap_or(""));
                a.testo("mimeType", &p.tipo);
                a.numero(
                    "size",
                    p.scaricato
                        .unwrap_or_else(|| decodificati(p.byte, p.codifica.as_deref())),
                );
                a.booleano("inline", p.ruolo == "incorporato");
                if let Some(c) = &p.content_id {
                    a.testo("contentId", c);
                }
                a.testo("part", &p.parte);
                a.testo("url", &url(&p.parte));
                if let (Some(l), Some(h)) = (p.larghezza, p.altezza)
                    && l > 0
                    && h > 0
                {
                    a.numero("width", l);
                    a.numero("height", h);
                }
                if let Some(n) = p.pagine.filter(|&n| n > 0) {
                    a.numero("pages", n);
                }
                if p.miniatura
                    && let Some(u) = url_miniatura(&p.parte)
                {
                    a.testo("thumb", &u);
                }
                a.chiudi_oggetto();
            }
            l.chiudi_lista();
        }
        {
            let mut l = Scrittore::lista(m.chiave("headers"));
            for (n, v) in intestazioni.get(&r.id).map(Vec::as_slice).unwrap_or(&[]) {
                let mut h = Scrittore::lista(l.elemento());
                stringa(h.elemento(), n);
                stringa(h.elemento(), v);
                h.chiudi_lista();
            }
            l.chiudi_lista();
        }
        {
            let mut f = Scrittore::oggetto(m.chiave("flags"));
            f.booleano("seen", r.letto);
            f.booleano("flagged", r.importante);
            f.chiudi_oggetto();
        }
        {
            let mut g = Scrittore::oggetto(m.chiave("gmail"));
            let thread = match r.gm_thrid {
                Some(t) => format!("{:016x}", t as u64),
                None => r.thread.clone().unwrap_or_default(),
            };
            g.testo("threadId", &thread);
            let mut l = Scrittore::lista(g.chiave("labels"));
            for e in etichette.get(&r.id).map(Vec::as_slice).unwrap_or(&[]) {
                stringa(l.elemento(), e);
            }
            l.chiudi_lista();
            g.chiudi_oggetto();
        }
        {
            let mut l = Scrittore::lista(m.chiave("problems"));
            if !r.struttura {
                stringa(l.elemento(), "struttura non letta dal server");
            }
            l.chiudi_lista();
        }
        {
            let mut l = Scrittore::lista(m.chiave("otherParts"));
            let mut visti: Vec<&str> = Vec::new();
            for p in mie.iter().filter(|p| p.ruolo == "altro") {
                if !visti.contains(&p.tipo.as_str()) {
                    visti.push(&p.tipo);
                    stringa(l.elemento(), &p.tipo);
                }
            }
            l.chiudi_lista();
        }
        m.chiudi_oggetto();
    }
    lista.chiudi_lista();
    radice.chiudi_oggetto();
    Ok(out)
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn cid_riscritti() {
        let mut m = HashMap::new();
        m.insert(
            "logo@x".to_string(),
            "a10s://localhost/allegato/7/2".to_string(),
        );
        assert_eq!(
            riscrivi_cid(
                r#"<img src="cid:logo@x"><img src='CID:logo%40x'> cid:altro"#,
                &m
            ),
            r#"<img src="a10s://localhost/allegato/7/2"><img src='a10s://localhost/allegato/7/2'> cid:altro"#
        );
        assert_eq!(riscrivi_cid("niente", &m), "niente");
    }

    #[test]
    fn piccole_cose() {
        assert_eq!(angolari("a@b"), "<a@b>");
        assert_eq!(angolari("<a@b>"), "<a@b>");
        assert_eq!(decodificati(78, Some("base64")), 57);
        assert_eq!(percento("a%40b%"), "a@b%");
        assert_eq!(
            prefisso_miniature("a10s://localhost/allegato/").as_deref(),
            Some("a10s://localhost/miniatura/")
        );
        assert_eq!(prefisso_miniature("http://x/file/"), None);
    }
}
