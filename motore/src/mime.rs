//! Intestazioni, struttura e testo dei messaggi. Le intestazioni e i set di
//! caratteri li legge `mail-parser`; la struttura arriva già analizzata da IMAP
//! (BODYSTRUCTURE), così gli allegati si registrano senza scaricarli.

use std::borrow::Cow;

use imap_proto::{BodyStructure, ContentEncoding};
use mail_parser::{Address, HeaderValue, MessageParser};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Persona {
    pub nome: Option<String>,
    pub email: String,
}

#[derive(Debug, Default)]
pub struct Intestazioni {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub riferimenti: Vec<String>,
    pub oggetto: Option<String>,
    pub data: Option<i64>,
    pub da: Vec<Persona>,
    pub a: Vec<Persona>,
    pub cc: Vec<Persona>,
    pub ccn: Vec<Persona>,
    pub rispondi_a: Vec<Persona>,
    pub lista_id: Option<String>,
    /// Newsletter, notifiche, risposte automatiche.
    pub automatico: bool,
}

/// I campi chiesti al server con BODY.PEEK[HEADER.FIELDS (…)].
pub const CAMPI_INTESTAZIONE: &str = "DATE FROM SENDER REPLY-TO TO CC BCC SUBJECT MESSAGE-ID IN-REPLY-TO REFERENCES LIST-ID LIST-UNSUBSCRIBE AUTO-SUBMITTED PRECEDENCE";

fn persone(a: Option<&Address<'_>>) -> Vec<Persona> {
    let Some(a) = a else { return Vec::new() };
    a.iter()
        .filter_map(|x| {
            let email = x.address()?.trim().to_lowercase();
            if email.is_empty() {
                return None;
            }
            let nome = x
                .name()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty());
            Some(Persona { nome, email })
        })
        .collect()
}

fn lista_testi(v: &HeaderValue<'_>) -> Vec<String> {
    match v {
        HeaderValue::Text(t) => vec![t.to_string()],
        HeaderValue::TextList(l) => l.iter().map(|t| t.to_string()).collect(),
        _ => Vec::new(),
    }
}

pub fn leggi_intestazioni(grezze: &[u8]) -> Intestazioni {
    let mut h = Intestazioni::default();
    let Some(m) = MessageParser::default().parse_headers(grezze) else {
        return h;
    };
    h.message_id = m.message_id().map(str::to_string);
    h.in_reply_to = lista_testi(m.in_reply_to()).into_iter().next();
    h.riferimenti = lista_testi(m.references());
    h.oggetto = m.subject().map(str::to_string);
    h.data = m.date().map(|d| d.to_timestamp());
    h.da = persone(m.from());
    h.a = persone(m.to());
    h.cc = persone(m.cc());
    h.ccn = persone(m.bcc());
    h.rispondi_a = persone(m.reply_to());
    h.lista_id = lista_testi(m.list_id()).into_iter().next();
    let disiscrizione = !m.list_unsubscribe().is_empty();
    let auto = m
        .header_raw("Auto-Submitted")
        .is_some_and(|v| !v.trim().eq_ignore_ascii_case("no"));
    let massiva = m.header_raw("Precedence").is_some_and(|v| {
        let v = v.trim().to_ascii_lowercase();
        v == "bulk" || v == "list" || v == "junk"
    });
    h.automatico = h.lista_id.is_some() || disiscrizione || auto || massiva;
    h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ruolo {
    Testo,
    Html,
    Allegato,
    /// Immagine dentro il testo (per esempio nella firma), con Content-ID.
    Incorporato,
    Altro,
}

impl Ruolo {
    pub fn come_testo(self) -> &'static str {
        match self {
            Ruolo::Testo => "testo",
            Ruolo::Html => "html",
            Ruolo::Allegato => "allegato",
            Ruolo::Incorporato => "incorporato",
            Ruolo::Altro => "altro",
        }
    }
    pub fn da_testo(s: &str) -> Self {
        match s {
            "testo" => Ruolo::Testo,
            "html" => Ruolo::Html,
            "allegato" => Ruolo::Allegato,
            "incorporato" => Ruolo::Incorporato,
            _ => Ruolo::Altro,
        }
    }
}

/// Una parte foglia del messaggio, dalla BODYSTRUCTURE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parte {
    /// Numero della parte per BODY.PEEK[…]: "1", "1.2", "2.1.1".
    pub parte: String,
    /// Tipo MIME in minuscolo: "text/plain".
    pub tipo: String,
    pub charset: Option<String>,
    pub codifica: Option<String>,
    /// Byte della parte così com'è sul server (codificata).
    pub byte: u64,
    pub nome_file: Option<String>,
    pub disposizione: Option<String>,
    pub content_id: Option<String>,
    pub ruolo: Ruolo,
}

type Parametri<'a> = Option<Vec<(Cow<'a, str>, Cow<'a, str>)>>;

fn parametro(p: &Parametri<'_>, nome: &str) -> Option<String> {
    p.as_ref()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(nome))
        .map(|(_, v)| v.to_string())
}

/// Decodifica le parole codificate RFC 2047 (`=?UTF-8?Q?…?=`) passando da mail-parser.
fn decodifica_parole(v: &str) -> String {
    if !v.contains("=?") {
        return v.to_string();
    }
    let finto = format!("Subject: {v}\r\n\r\n");
    MessageParser::default()
        .parse_headers(finto.as_bytes())
        .and_then(|m| m.subject().map(str::to_string))
        .unwrap_or_else(|| v.to_string())
}

fn percento(s: &str) -> Vec<u8> {
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
    out
}

/// Nome di file da un parametro, anche nella forma RFC 2231 (`filename*=utf-8''…`,
/// `filename*0*=…`, `filename*1*=…`).
fn nome_da_parametri(p: &Parametri<'_>, chiave: &str) -> Option<String> {
    if let Some(v) = parametro(p, chiave) {
        return Some(decodifica_parole(&v));
    }
    let lista = p.as_ref()?;
    let mut pezzi: Vec<(u32, bool, String)> = Vec::new();
    for (k, v) in lista {
        let k = k.to_ascii_lowercase();
        let Some(resto) = k.strip_prefix(&format!("{chiave}*")) else {
            continue;
        };
        if resto.is_empty() {
            pezzi.push((0, true, v.to_string()));
        } else {
            let (num, codificato) = match resto.strip_suffix('*') {
                Some(n) => (n, true),
                None => (resto, false),
            };
            if let Ok(n) = num.parse() {
                pezzi.push((n, codificato, v.to_string()));
            }
        }
    }
    if pezzi.is_empty() {
        return None;
    }
    pezzi.sort_by_key(|x| x.0);
    let mut byte = Vec::new();
    for (i, (_, codificato, v)) in pezzi.iter().enumerate() {
        if *codificato {
            // Il primo pezzo codificato porta charset'lingua'
            let valore = if i == 0 {
                v.splitn(3, '\'').nth(2).unwrap_or(v)
            } else {
                v.as_str()
            };
            byte.extend(percento(valore));
        } else {
            byte.extend(v.as_bytes());
        }
    }
    Some(String::from_utf8_lossy(&byte).into_owned())
}

fn codifica(c: &ContentEncoding<'_>) -> Option<String> {
    Some(match c {
        ContentEncoding::SevenBit => "7bit".into(),
        ContentEncoding::EightBit => "8bit".into(),
        ContentEncoding::Binary => "binary".into(),
        ContentEncoding::Base64 => "base64".into(),
        ContentEncoding::QuotedPrintable => "quoted-printable".into(),
        ContentEncoding::Other(o) => o.to_ascii_lowercase(),
    })
}

pub fn parti_da_struttura(bs: &BodyStructure<'_>) -> Vec<Parte> {
    let mut out = Vec::new();
    match bs {
        BodyStructure::Multipart { bodies, .. } => {
            for (i, b) in bodies.iter().enumerate() {
                visita(b, (i + 1).to_string(), &mut out);
            }
        }
        altro => visita(altro, "1".into(), &mut out),
    }
    out
}

fn visita(bs: &BodyStructure<'_>, numero: String, out: &mut Vec<Parte>) {
    let (common, other) = match bs {
        BodyStructure::Multipart { bodies, .. } => {
            for (i, b) in bodies.iter().enumerate() {
                visita(b, format!("{numero}.{}", i + 1), out);
            }
            return;
        }
        BodyStructure::Basic { common, other, .. }
        | BodyStructure::Text { common, other, .. }
        | BodyStructure::Message { common, other, .. } => (common, other),
    };
    let tipo = format!("{}/{}", common.ty.ty, common.ty.subtype).to_ascii_lowercase();
    let disposizione = common
        .disposition
        .as_ref()
        .map(|d| d.ty.to_ascii_lowercase());
    let nome_file = common
        .disposition
        .as_ref()
        .and_then(|d| nome_da_parametri(&d.params, "filename"))
        .or_else(|| nome_da_parametri(&common.ty.params, "name"));
    let content_id = other
        .id
        .as_ref()
        .map(|s| s.trim().trim_matches(|c| c == '<' || c == '>').to_string());
    let charset = parametro(&common.ty.params, "charset");
    let ruolo = if disposizione.as_deref() == Some("attachment") {
        Ruolo::Allegato
    } else if tipo.starts_with("image/") && content_id.is_some() {
        Ruolo::Incorporato
    } else if nome_file.is_some() || tipo == "message/rfc822" {
        Ruolo::Allegato
    } else if tipo == "text/plain" {
        Ruolo::Testo
    } else if tipo == "text/html" {
        Ruolo::Html
    } else if tipo.starts_with("text/") {
        Ruolo::Altro
    } else {
        Ruolo::Allegato
    };
    out.push(Parte {
        parte: numero,
        tipo,
        charset,
        codifica: codifica(&other.transfer_encoding),
        byte: other.octets as u64,
        nome_file,
        disposizione,
        content_id,
        ruolo,
    });
}

/// Decodifica una parte di testo scaricata: restituisce (testo semplice, html).
/// Il testo semplice di una parte HTML è la sua versione senza marcatori.
pub fn decodifica_parte(parte: &Parte, dati: &[u8]) -> (Option<String>, Option<String>) {
    let mut finto = Vec::with_capacity(dati.len() + 128);
    finto.extend_from_slice(format!("Content-Type: {}", parte.tipo).as_bytes());
    if let Some(c) = &parte.charset {
        let c: String = c
            .chars()
            .filter(|c| c.is_ascii_graphic() && *c != '"')
            .collect();
        finto.extend_from_slice(format!("; charset=\"{c}\"").as_bytes());
    }
    finto.extend_from_slice(b"\r\n");
    if let Some(c) = &parte.codifica {
        finto.extend_from_slice(format!("Content-Transfer-Encoding: {c}\r\n").as_bytes());
    }
    finto.extend_from_slice(b"\r\n");
    finto.extend_from_slice(dati);
    let Some(m) = MessageParser::default().parse(&finto) else {
        return (None, None);
    };
    let testo = m.body_text(0).map(|t| t.into_owned());
    let html = if parte.ruolo == Ruolo::Html {
        m.body_html(0).map(|t| t.into_owned())
    } else {
        None
    };
    (testo, html)
}

/// FNV-1a a 64 bit: stabile fra le versioni di Rust, a differenza di DefaultHasher.
pub fn fnv64(dati: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in dati {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Identità di un messaggio quando il server non ha X-GM-MSGID.
pub fn chiave_generica(
    message_id: Option<&str>,
    data: Option<i64>,
    mittente: Option<&str>,
    oggetto: Option<&str>,
    dimensione: Option<u32>,
) -> String {
    let dim = dimensione.unwrap_or(0);
    match message_id {
        Some(m) if !m.trim().is_empty() => format!("mid:{}|{dim}", m.trim()),
        _ => {
            let impronta = format!(
                "{}\u{1}{}\u{1}{}",
                data.unwrap_or(0),
                mittente.unwrap_or(""),
                oggetto.unwrap_or("")
            );
            format!("h:{:016x}|{dim}", fnv64(impronta.as_bytes()))
        }
    }
}

/// Thread quando il server non ha X-GM-THRID: la radice dei riferimenti.
pub fn chiave_thread(h: &Intestazioni, chiave_messaggio: &str) -> String {
    if let Some(radice) = h.riferimenti.first() {
        return format!("ref:{radice}");
    }
    if let Some(r) = &h.in_reply_to {
        return format!("ref:{r}");
    }
    if let Some(m) = &h.message_id {
        return format!("ref:{m}");
    }
    format!("solo:{chiave_messaggio}")
}

/// Toglie i marcatori da un HTML, per la ricerca (quando mail-parser non dà testo).
pub fn senza_marcatori(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut dentro = false;
    for c in html.chars() {
        match c {
            '<' => dentro = true,
            '>' => {
                dentro = false;
                out.push(' ');
            }
            _ if !dentro => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn intestazioni_inventate() {
        let h = leggi_intestazioni(
            b"From: \"Lucia Ferrante\" <Lucia@Esempio.invalid>\r\n\
To: io@esempio.invalid, Marco Neri <marco@esempio.invalid>\r\n\
Subject: =?UTF-8?Q?Preventivo_n=C2=B0_3?=\r\n\
Message-ID: <m3@esempio.invalid>\r\n\
In-Reply-To: <m2@esempio.invalid>\r\n\
References: <m1@esempio.invalid> <m2@esempio.invalid>\r\n\
Date: Tue, 27 Oct 2026 10:00:00 +0100\r\n\
List-Unsubscribe: <mailto:via@esempio.invalid>\r\n\r\n",
        );
        assert_eq!(h.oggetto.as_deref(), Some("Preventivo n° 3"));
        assert_eq!(h.da[0].email, "lucia@esempio.invalid");
        assert_eq!(h.da[0].nome.as_deref(), Some("Lucia Ferrante"));
        assert_eq!(h.a.len(), 2);
        assert_eq!(h.message_id.as_deref(), Some("m3@esempio.invalid"));
        assert_eq!(
            h.riferimenti,
            vec!["m1@esempio.invalid", "m2@esempio.invalid"]
        );
        assert_eq!(h.in_reply_to.as_deref(), Some("m2@esempio.invalid"));
        assert_eq!(h.data, Some(1793091600));
        assert!(h.automatico);
        assert_eq!(chiave_thread(&h, "x"), "ref:m1@esempio.invalid");
    }

    #[test]
    fn intestazioni_rotte() {
        let h = leggi_intestazioni(b"\x00\xff\xfe questo non e' un'intestazione");
        assert!(h.da.is_empty());
        let h = leggi_intestazioni(b"");
        assert!(h.oggetto.is_none());
    }

    #[test]
    fn parte_quoted_printable() {
        let p = Parte {
            parte: "1".into(),
            tipo: "text/plain".into(),
            charset: Some("utf-8".into()),
            codifica: Some("quoted-printable".into()),
            byte: 10,
            nome_file: None,
            disposizione: None,
            content_id: None,
            ruolo: Ruolo::Testo,
        };
        let (t, h) = decodifica_parte(&p, b"Perch=C3=A9 no?");
        assert_eq!(t.as_deref(), Some("Perché no?"));
        assert!(h.is_none());
    }

    #[test]
    fn nomi_rfc2231() {
        let p: Parametri<'_> = Some(vec![
            (
                Cow::Borrowed("filename*0*"),
                Cow::Borrowed("utf-8''prev%C3%A9"),
            ),
            (Cow::Borrowed("filename*1"), Cow::Borrowed("ntivo.pdf")),
        ]);
        assert_eq!(
            nome_da_parametri(&p, "filename").as_deref(),
            Some("prevéntivo.pdf")
        );
    }
}
