//! Il finto server IMAP con la casella sintetica, per provare il motore e l'app
//! senza Gmail. Legge i file `.eml` e `.meta` scritti da `tools/sintetici-in-eml.mjs`,
//! calcola dal messaggio vero la BODYSTRUCTURE e le sezioni (`BODY[1.2]`,
//! `BODY[1]<0.100>`, `HEADER.FIELDS`…) e risponde ai comandi che il motore manda,
//! con le estensioni di Gmail (X-GM-EXT-1), CONDSTORE e IDLE.
//!
//! Sta in un file solo perché lo usano sia l'eseguibile di prova
//! (`examples/finto_server/main.rs`) sia le prove automatiche (`tests/sintetici.rs`).
//!
//! Può far arrivare posta nuova a intervalli regolari e far cadere le connessioni,
//! per vedere nell'app l'avanzamento e lo stato della rete. Registra ogni comando
//! ricevuto, così le prove possono controllare che non sia arrivato niente che scriva.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use motore::tempo;

pub const UTENTE_PREDEFINITO: &str = "alex.moro@esempio.it";
pub const PASSWORD_PREDEFINITA: &str = "finta-password";
pub const CARTELLA_TUTTI: &str = "[Gmail]/Tutti i messaggi";

// ---------------------------------------------------------------------------
// Struttura MIME
// ---------------------------------------------------------------------------

/// Una parte del messaggio, con le posizioni nel messaggio grezzo.
#[derive(Debug, Clone)]
pub struct Nodo {
    /// Intestazioni della parte, compresa la riga vuota che le chiude.
    pub intestazioni: Range<usize>,
    /// Il corpo così com'è (codificato), senza intestazioni.
    pub corpo: Range<usize>,
    pub tipo: String,
    pub sottotipo: String,
    pub parametri: Vec<(String, String)>,
    pub id: Option<String>,
    pub descrizione: Option<String>,
    pub codifica: Option<String>,
    pub disposizione: Option<(String, Vec<(String, String)>)>,
    pub lingua: Option<String>,
    pub posizione: Option<String>,
    pub figli: Vec<Nodo>,
}

fn trova(b: &[u8], ago: &[u8], da: usize) -> Option<usize> {
    if ago.is_empty() || da > b.len() {
        return None;
    }
    b[da..]
        .windows(ago.len())
        .position(|w| w == ago)
        .map(|p| p + da)
}

/// Le intestazioni di un blocco, svolte: (nome in minuscolo, valore).
pub fn intestazioni(b: &[u8]) -> Vec<(String, String)> {
    let testo = String::from_utf8_lossy(b);
    let mut out: Vec<(String, String)> = Vec::new();
    for riga in testo.split('\n') {
        let riga = riga.trim_end_matches('\r');
        if riga.is_empty() {
            break;
        }
        if riga.starts_with([' ', '\t']) {
            if let Some(ultima) = out.last_mut() {
                ultima.1.push_str(riga);
            }
            continue;
        }
        if let Some((n, v)) = riga.split_once(':') {
            out.push((n.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    out
}

/// `tipo/sotto; k=v; k2="v 2"` → ("tipo/sotto", [(k, v), (k2, "v 2")]).
pub fn valore_e_parametri(v: &str) -> (String, Vec<(String, String)>) {
    let mut pezzi: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut virgolette = false;
    let mut escape = false;
    for c in v.chars() {
        if escape {
            cur.push(c);
            escape = false;
            continue;
        }
        match c {
            '\\' if virgolette => {
                cur.push(c);
                escape = true;
            }
            '"' => {
                virgolette = !virgolette;
                cur.push(c);
            }
            ';' if !virgolette => pezzi.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    pezzi.push(cur);
    let principale = pezzi[0].trim().to_string();
    let mut parametri = Vec::new();
    for p in &pezzi[1..] {
        let Some((k, v)) = p.split_once('=') else {
            continue;
        };
        let k = k.trim().to_string();
        let v = v.trim();
        let v = if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
            let mut s = String::new();
            let mut esc = false;
            for c in v[1..v.len() - 1].chars() {
                if esc {
                    s.push(c);
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else {
                    s.push(c);
                }
            }
            s
        } else {
            v.to_string()
        };
        if !k.is_empty() {
            parametri.push((k, v));
        }
    }
    (principale, parametri)
}

/// Analizza la parte che va da `inizio` a `fine` nel messaggio grezzo.
pub fn analizza(g: &[u8], inizio: usize, fine: usize, predefinito: (&str, &str)) -> Nodo {
    let (fine_int, inizio_corpo) = if g[inizio..fine].starts_with(b"\r\n") {
        (inizio + 2, inizio + 2)
    } else if g[inizio..fine].starts_with(b"\n") {
        (inizio + 1, inizio + 1)
    } else {
        match (
            trova(&g[..fine], b"\r\n\r\n", inizio),
            trova(&g[..fine], b"\n\n", inizio),
        ) {
            (Some(a), Some(b)) if b + 2 < a + 4 => (b + 2, b + 2),
            (Some(a), _) => (a + 4, a + 4),
            (None, Some(b)) => (b + 2, b + 2),
            (None, None) => (fine, fine),
        }
    };
    let h = intestazioni(&g[inizio..fine_int]);
    let campo = |n: &str| h.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let (tipo, sottotipo, parametri) = match campo("content-type") {
        Some(ct) => {
            let (principale, parametri) = valore_e_parametri(&ct);
            match principale.split_once('/') {
                Some((t, s)) if !t.is_empty() && !s.is_empty() => (
                    t.trim().to_ascii_lowercase(),
                    s.trim().to_ascii_lowercase(),
                    parametri,
                ),
                _ => ("text".into(), "plain".into(), parametri),
            }
        }
        None => (
            predefinito.0.to_string(),
            predefinito.1.to_string(),
            if predefinito.0 == "text" {
                vec![("charset".into(), "us-ascii".into())]
            } else {
                Vec::new()
            },
        ),
    };
    let disposizione = campo("content-disposition").map(|d| {
        let (t, p) = valore_e_parametri(&d);
        (t.to_ascii_lowercase(), p)
    });
    let mut nodo = Nodo {
        intestazioni: inizio..fine_int,
        corpo: inizio_corpo..fine,
        tipo,
        sottotipo,
        parametri,
        id: campo("content-id"),
        descrizione: campo("content-description"),
        codifica: campo("content-transfer-encoding").map(|c| c.to_ascii_lowercase()),
        disposizione,
        lingua: campo("content-language"),
        posizione: campo("content-location"),
        figli: Vec::new(),
    };
    if nodo.tipo == "multipart" {
        let confine = nodo
            .parametri
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("boundary"))
            .map(|(_, v)| v.clone());
        if let Some(c) = confine {
            let predef = if nodo.sottotipo == "digest" {
                ("message", "rfc822")
            } else {
                ("text", "plain")
            };
            nodo.figli = dividi(g, inizio_corpo, fine, &c)
                .into_iter()
                .map(|(a, b)| analizza(g, a, b, predef))
                .collect();
        }
    }
    nodo
}

/// Le parti di un corpo multipart: intervalli tra un delimitatore e il successivo.
/// Il CRLF prima del delimitatore appartiene al delimitatore.
fn dividi(g: &[u8], inizio: usize, fine: usize, confine: &str) -> Vec<(usize, usize)> {
    let delim = format!("--{confine}");
    let d = delim.as_bytes();
    let mut posizioni = Vec::new();
    let mut i = inizio;
    while i < fine {
        let a_capo = i == inizio || g[i - 1] == b'\n';
        if a_capo && g[i..fine].starts_with(d) {
            let dopo = i + d.len();
            let chiusura = g[dopo..fine].starts_with(b"--");
            let fine_riga = trova(&g[..fine], b"\n", dopo).map_or(fine, |p| p + 1);
            posizioni.push((i, fine_riga, chiusura));
            if chiusura {
                break;
            }
            i = fine_riga;
            continue;
        }
        i = trova(&g[..fine], b"\n", i).map_or(fine, |p| p + 1);
    }
    let mut parti = Vec::new();
    for w in posizioni.windows(2) {
        let (_, inizio_parte, chiusura) = w[0];
        if chiusura {
            break;
        }
        let mut fine_parte = w[1].0;
        if fine_parte >= 2 && &g[fine_parte - 2..fine_parte] == b"\r\n" {
            fine_parte -= 2;
        } else if fine_parte >= 1 && g[fine_parte - 1] == b'\n' {
            fine_parte -= 1;
        }
        parti.push((inizio_parte, fine_parte.max(inizio_parte)));
    }
    parti
}

/// Una stringa IMAP: tra virgolette se si può, altrimenti letterale.
fn stringa(s: &str, out: &mut Vec<u8>) {
    if s.bytes().all(|b| (0x20..0x7f).contains(&b)) && s.len() < 1000 {
        out.push(b'"');
        for b in s.bytes() {
            if b == b'"' || b == b'\\' {
                out.push(b'\\');
            }
            out.push(b);
        }
        out.push(b'"');
    } else {
        out.extend_from_slice(format!("{{{}}}\r\n", s.len()).as_bytes());
        out.extend_from_slice(s.as_bytes());
    }
}

fn nstringa(s: &Option<String>, out: &mut Vec<u8>) {
    match s {
        Some(s) => stringa(s, out),
        None => out.extend_from_slice(b"NIL"),
    }
}

fn parametri_imap(p: &[(String, String)], out: &mut Vec<u8>) {
    if p.is_empty() {
        out.extend_from_slice(b"NIL");
        return;
    }
    out.push(b'(');
    for (i, (k, v)) in p.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        stringa(&k.to_ascii_uppercase(), out);
        out.push(b' ');
        stringa(v, out);
    }
    out.push(b')');
}

fn estensione(n: &Nodo, out: &mut Vec<u8>) {
    out.push(b' ');
    match &n.disposizione {
        Some((t, p)) => {
            out.push(b'(');
            stringa(&t.to_ascii_uppercase(), out);
            out.push(b' ');
            parametri_imap(p, out);
            out.push(b')');
        }
        None => out.extend_from_slice(b"NIL"),
    }
    out.push(b' ');
    nstringa(&n.lingua, out);
    out.push(b' ');
    nstringa(&n.posizione, out);
}

/// La BODYSTRUCTURE estesa (RFC 3501, 7.4.2), calcolata dal messaggio.
pub fn struttura(n: &Nodo, g: &[u8], out: &mut Vec<u8>) {
    out.push(b'(');
    if !n.figli.is_empty() {
        for f in &n.figli {
            struttura(f, g, out);
        }
        out.push(b' ');
        stringa(&n.sottotipo.to_ascii_uppercase(), out);
        out.push(b' ');
        parametri_imap(&n.parametri, out);
        estensione(n, out);
        out.push(b')');
        return;
    }
    // Un multipart senza figli leggibili si descrive come testo vuoto.
    let (tipo, sotto) = if n.tipo == "multipart" {
        ("TEXT".to_string(), "PLAIN".to_string())
    } else {
        (
            n.tipo.to_ascii_uppercase(),
            n.sottotipo.to_ascii_uppercase(),
        )
    };
    stringa(&tipo, out);
    out.push(b' ');
    stringa(&sotto, out);
    out.push(b' ');
    parametri_imap(&n.parametri, out);
    out.push(b' ');
    nstringa(&n.id, out);
    out.push(b' ');
    nstringa(&n.descrizione, out);
    out.push(b' ');
    stringa(
        &n.codifica.as_deref().unwrap_or("7bit").to_ascii_uppercase(),
        out,
    );
    let corpo = &g[n.corpo.clone()];
    out.extend_from_slice(format!(" {}", corpo.len()).as_bytes());
    if tipo == "TEXT" {
        let righe = corpo.iter().filter(|&&b| b == b'\n').count()
            + usize::from(!corpo.is_empty() && !corpo.ends_with(b"\n"));
        out.extend_from_slice(format!(" {righe}").as_bytes());
    }
    out.extend_from_slice(b" NIL");
    estensione(n, out);
    out.push(b')');
}

/// La parte con quel numero (`[1, 2]` per "1.2"). Per un messaggio non multipart la parte 1 è il corpo.
pub fn parte<'a>(radice: &'a Nodo, numeri: &[u32]) -> Option<&'a Nodo> {
    let mut cur = radice;
    for (i, &n) in numeri.iter().enumerate() {
        if n == 0 {
            return None;
        }
        if !cur.figli.is_empty() {
            cur = cur.figli.get(n as usize - 1)?;
        } else if n == 1 && i == 0 && std::ptr::eq(cur, radice) {
            // il corpo di un messaggio di una parte sola
        } else {
            return None;
        }
    }
    Some(cur)
}

/// Le righe delle intestazioni indicate (con le righe di continuazione), più la riga vuota.
pub fn campi_intestazione(
    g: &[u8],
    blocco: Range<usize>,
    nomi: &[String],
    escludi: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    let b = &g[blocco];
    let mut dentro = false;
    for riga in b.split_inclusive(|&c| c == b'\n') {
        let vuota = riga == b"\r\n" || riga == b"\n";
        if vuota {
            break;
        }
        if riga.starts_with(b" ") || riga.starts_with(b"\t") {
            if dentro {
                out.extend_from_slice(riga);
            }
            continue;
        }
        let nome = riga
            .split(|&c| c == b':')
            .next()
            .map(|n| String::from_utf8_lossy(n).trim().to_ascii_uppercase())
            .unwrap_or_default();
        let c = nomi.iter().any(|x| x.eq_ignore_ascii_case(&nome));
        dentro = c != escludi;
        if dentro {
            out.extend_from_slice(riga);
        }
    }
    out.extend_from_slice(b"\r\n");
    out
}

/// Sostituisce (o aggiunge in cima) un'intestazione del messaggio.
pub fn sostituisci_intestazione(g: &[u8], nome: &str, valore: &str) -> Vec<u8> {
    let fine = trova(g, b"\r\n\r\n", 0).map_or(g.len(), |p| p + 2);
    let mut out = Vec::with_capacity(g.len() + valore.len());
    let nuova = format!("{nome}: {valore}\r\n");
    let mut messa = false;
    let mut salta = false;
    for riga in g[..fine].split_inclusive(|&c| c == b'\n') {
        if riga.starts_with(b" ") || riga.starts_with(b"\t") {
            if !salta {
                out.extend_from_slice(riga);
            }
            continue;
        }
        let n = riga.split(|&c| c == b':').next().unwrap_or_default();
        salta = String::from_utf8_lossy(n).trim().eq_ignore_ascii_case(nome);
        if salta {
            if !messa {
                out.extend_from_slice(nuova.as_bytes());
                messa = true;
            }
        } else {
            out.extend_from_slice(riga);
        }
    }
    if !messa {
        let mut con = nuova.into_bytes();
        con.extend_from_slice(&out);
        out = con;
    }
    out.extend_from_slice(&g[fine..]);
    out
}

/// Base64 a righe di 76 caratteri, come lo scrivono i programmi di posta.
pub fn base64_a_righe(d: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(d.len() * 4 / 3 + d.len() / 38 + 4);
    let mut riga = 0;
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
        riga += 4;
        if riga == 76 {
            s.push_str("\r\n");
            riga = 0;
        }
    }
    if !s.ends_with("\r\n") {
        s.push_str("\r\n");
    }
    s
}

/// Un messaggio con un testo e allegati veri in base64 (PDF, immagini…), per le
/// prove su miniature, misure e messaggio intero. `allegati`: (nome, tipo, contenuto).
pub fn messaggio_con_allegati(
    oggetto: &str,
    message_id: &str,
    allegati: &[(&str, &str, &[u8])],
) -> Vec<u8> {
    let mut m = format!(
        "From: Studio Esempio <studio@esempio.invalid>\r\n\
To: Alex Moro <alex.moro@esempio.it>\r\n\
Subject: {oggetto}\r\n\
Date: Tue, 27 Oct 2026 10:00:00 +0100\r\n\
Message-ID: {message_id}\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"confine-allegati\"\r\n\
\r\n\
--confine-allegati\r\n\
Content-Type: text/plain; charset=UTF-8\r\n\
\r\n\
In allegato i documenti.\r\n"
    )
    .into_bytes();
    for (nome, tipo, dati) in allegati {
        m.extend_from_slice(
            format!(
                "--confine-allegati\r\n\
Content-Type: {tipo}; name=\"{nome}\"\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-Disposition: attachment; filename=\"{nome}\"\r\n\
\r\n"
            )
            .as_bytes(),
        );
        m.extend_from_slice(base64_a_righe(dati).as_bytes());
    }
    m.extend_from_slice(b"--confine-allegati--\r\n");
    m
}

// ---------------------------------------------------------------------------
// La casella
// ---------------------------------------------------------------------------

/// Quello che le prove si aspettano di ritrovare nel motore (dal file .meta).
#[derive(Debug, Clone, Default)]
pub struct Attese {
    pub id: String,
    /// Lunghezza in byte e impronta FNV-1a del testo normalizzato.
    pub testo: Option<(usize, u64)>,
    pub html: Option<(usize, u64)>,
    pub allegati: Vec<AllegatoAtteso>,
}

#[derive(Debug, Clone)]
pub struct AllegatoAtteso {
    pub parte: String,
    pub byte: usize,
    pub impronta: u64,
    pub tipo: String,
    pub nome: String,
    pub incorporato: bool,
}

#[derive(Debug, Clone)]
pub struct Messaggio {
    pub uid: u32,
    pub msgid: u64,
    pub thrid: u64,
    pub etichette: Vec<String>,
    pub flags: Vec<String>,
    pub modseq: u64,
    pub data_interna: i64,
    pub grezzo: Arc<Vec<u8>>,
    pub albero: Arc<Nodo>,
    pub attese: Arc<Attese>,
}

impl Messaggio {
    pub fn nuovo(grezzo: Vec<u8>) -> Self {
        let albero = analizza(&grezzo, 0, grezzo.len(), ("text", "plain"));
        Messaggio {
            uid: 0,
            msgid: 0,
            thrid: 0,
            etichette: Vec::new(),
            flags: Vec::new(),
            modseq: 1,
            data_interna: tempo::adesso(),
            grezzo: Arc::new(grezzo),
            albero: Arc::new(albero),
            attese: Arc::new(Attese::default()),
        }
    }

    fn riscrivi(&mut self, nome: &str, valore: &str) {
        let g = sostituisci_intestazione(&self.grezzo, nome, valore);
        self.albero = Arc::new(analizza(&g, 0, g.len(), ("text", "plain")));
        self.grezzo = Arc::new(g);
    }

    fn escluso_da_tutti(&self) -> bool {
        self.etichette
            .iter()
            .any(|e| e.eq_ignore_ascii_case("\\Spam") || e.eq_ignore_ascii_case("\\Trash"))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Account {
    pub indirizzo: String,
    pub nome: String,
    pub alias: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Casella {
    pub uidvalidity: u32,
    pub prossimo_uid: u32,
    pub modseq: u64,
    /// I messaggi di «Tutti i messaggi», in ordine di UID.
    pub messaggi: Vec<Messaggio>,
    /// Messaggi tenuti da parte: arrivano uno alla volta con `arriva`.
    pub riserva: VecDeque<Messaggio>,
    /// Spam e cestino: su Gmail non stanno in «Tutti i messaggi».
    pub esclusi: Vec<Messaggio>,
    prossimo_msgid: u64,
    copiati: usize,
}

fn leggi_meta(testo: &str, m: &mut Messaggio) -> Result<(), String> {
    let mut a = Attese::default();
    let numero = |v: &str, k: &str| -> Result<u64, String> {
        v.trim()
            .parse::<u64>()
            .map_err(|_| format!("{k}: numero non valido"))
    };
    let lung_impronta = |v: &str| -> Option<(usize, u64)> {
        let (l, h) = v.trim().split_once(' ')?;
        Some((l.parse().ok()?, u64::from_str_radix(h, 16).ok()?))
    };
    for riga in testo.lines() {
        let Some((k, v)) = riga.split_once(": ") else {
            continue;
        };
        match k {
            "id" => a.id = v.to_string(),
            "uid" => m.uid = numero(v, k)? as u32,
            "x-gm-msgid" => m.msgid = numero(v, k)?,
            "x-gm-thrid" => m.thrid = numero(v, k)?,
            "internaldate" => m.data_interna = numero(v, k)? as i64,
            "flag" => m.flags.push(v.to_string()),
            "etichetta" => m.etichette.push(v.to_string()),
            "testo" => a.testo = lung_impronta(v),
            "html" => a.html = lung_impronta(v),
            "allegato" | "incorporato" => {
                let mut p = v.splitn(5, ' ');
                let (Some(parte), Some(byte), Some(imp), Some(tipo), Some(nome)) =
                    (p.next(), p.next(), p.next(), p.next(), p.next())
                else {
                    return Err(format!("{k}: riga incompleta"));
                };
                a.allegati.push(AllegatoAtteso {
                    parte: parte.to_string(),
                    byte: byte.parse().map_err(|_| "byte non validi".to_string())?,
                    impronta: u64::from_str_radix(imp, 16)
                        .map_err(|_| "impronta non valida".to_string())?,
                    tipo: tipo.to_string(),
                    nome: nome.to_string(),
                    incorporato: k == "incorporato",
                });
            }
            _ => {}
        }
    }
    m.attese = Arc::new(a);
    Ok(())
}

impl Casella {
    /// Legge una cartella di `.eml` con i loro `.meta`.
    pub fn carica(cartella: &Path) -> Result<(Casella, Account), String> {
        let mut nomi: Vec<_> = std::fs::read_dir(cartella)
            .map_err(|e| format!("non riesco a leggere {}: {e}", cartella.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "eml"))
            .collect();
        nomi.sort();
        if nomi.is_empty() {
            return Err(format!(
                "{} non contiene file .eml: lancia prima `node tools/sintetici-in-eml.mjs`",
                cartella.display()
            ));
        }
        let mut tutti = Vec::new();
        for p in &nomi {
            let grezzo = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let mut m = Messaggio::nuovo(grezzo);
            let meta = p.with_extension("meta");
            let testo = std::fs::read_to_string(&meta).unwrap_or_default();
            leggi_meta(&testo, &mut m).map_err(|e| format!("{}: {e}", meta.display()))?;
            tutti.push(m);
        }
        let mut account = Account::default();
        if let Ok(t) = std::fs::read_to_string(cartella.join("account.meta")) {
            for riga in t.lines() {
                match riga.split_once(": ") {
                    Some(("indirizzo", v)) => account.indirizzo = v.to_string(),
                    Some(("nome", v)) => account.nome = v.to_string(),
                    Some(("alias", v)) => account.alias.push(v.to_string()),
                    _ => {}
                }
            }
        }
        Ok((Casella::con(tutti), account))
    }

    /// Una casella dai messaggi dati: UID mancanti assegnati in ordine.
    pub fn con(mut tutti: Vec<Messaggio>) -> Casella {
        let mut uid = tutti.iter().map(|m| m.uid).max().unwrap_or(0);
        for m in tutti.iter_mut().filter(|m| m.uid == 0) {
            uid += 1;
            m.uid = uid;
        }
        tutti.sort_by_key(|m| m.uid);
        let mut msgid = 0x1800000000000000u64;
        for m in &mut tutti {
            if m.msgid == 0 {
                msgid += 1;
                m.msgid = msgid;
            }
            if m.thrid == 0 {
                m.thrid = m.msgid;
            }
            m.modseq = 100 + m.uid as u64;
        }
        let prossimo_msgid = tutti.iter().map(|m| m.msgid).max().unwrap_or(0) + 1;
        let (esclusi, messaggi): (Vec<_>, Vec<_>) =
            tutti.into_iter().partition(|m| m.escluso_da_tutti());
        let massimo = messaggi
            .iter()
            .chain(&esclusi)
            .map(|m| m.uid)
            .max()
            .unwrap_or(0);
        let modseq = messaggi.iter().map(|m| m.modseq).max().unwrap_or(1);
        Casella {
            uidvalidity: 1_000_001,
            prossimo_uid: massimo + 1,
            modseq,
            messaggi,
            riserva: VecDeque::new(),
            esclusi,
            prossimo_msgid,
            copiati: 0,
        }
    }

    /// Mette da parte gli `n` messaggi più recenti: arriveranno con `arriva`.
    pub fn tieni_da_parte(&mut self, n: usize) {
        let n = n.min(self.messaggi.len());
        let via = self.messaggi.split_off(self.messaggi.len() - n);
        self.riserva.extend(via);
        self.modseq = self.messaggi.iter().map(|m| m.modseq).max().unwrap_or(1);
    }

    /// Fa arrivare un messaggio: il prossimo della riserva, oppure la copia di uno
    /// già presente con un'identità nuova. Data di adesso. Restituisce l'UID.
    pub fn arriva(&mut self, adesso: i64) -> u32 {
        let mut m = match self.riserva.pop_front() {
            Some(m) => m,
            None => {
                let i = self.copiati % self.messaggi.len().max(1);
                self.copiati += 1;
                let Some(origine) = self.messaggi.get(i) else {
                    return 0;
                };
                let mut m = origine.clone();
                m.msgid = 0;
                m.etichette = vec!["\\Inbox".into()];
                m.flags.clear();
                let id = format!("<arrivo-{}-{}@finto.invalid>", self.prossimo_uid, m.uid);
                m.riscrivi("Message-ID", &id);
                let mut a = (*m.attese).clone();
                a.id = format!("{}-copia{}", a.id, self.copiati);
                m.attese = Arc::new(a);
                m
            }
        };
        m.riscrivi("Date", &tempo::rfc5322(adesso));
        m.data_interna = adesso;
        m.uid = self.prossimo_uid;
        self.prossimo_uid += 1;
        if m.msgid == 0 || self.messaggi.iter().any(|x| x.msgid == m.msgid) {
            m.msgid = self.prossimo_msgid;
            self.prossimo_msgid += 1;
        }
        self.modseq += 1;
        m.modseq = self.modseq;
        let uid = m.uid;
        self.messaggi.push(m);
        uid
    }

    /// Moltiplica la casella: ogni messaggio ricopiato `volte - 1` volte, con UID,
    /// X-GM-MSGID, thread e Message-ID nuovi. Serve a misurare il motore su migliaia di messaggi.
    /// Come `moltiplica`, ma ogni copia va nel passato, una prima dell'altra, e gli UID seguono
    /// le date come su Gmail: una casella con anni di storico (giro 2).
    pub fn moltiplica_nel_passato(&mut self, volte: usize) {
        let n = self.messaggi.len();
        if n == 0 || volte < 2 {
            return;
        }
        let min = self.messaggi.iter().map(|m| m.data_interna).min().unwrap_or(0);
        let max = self.messaggi.iter().map(|m| m.data_interna).max().unwrap_or(0);
        let passo = (max - min) + 30 * 86_400;
        self.moltiplica(volte);
        for (i, m) in self.messaggi.iter_mut().enumerate().skip(n) {
            m.data_interna -= (i / n) as i64 * passo;
            m.riscrivi("Date", &tempo::rfc5322(m.data_interna));
        }
        self.messaggi.sort_by_key(|m| (m.data_interna, m.uid));
        for (i, m) in self.messaggi.iter_mut().enumerate() {
            m.uid = i as u32 + 1;
        }
        self.prossimo_uid = n as u32 * volte as u32 + 1;
    }

    pub fn moltiplica(&mut self, volte: usize) {
        let originali = self.messaggi.clone();
        for k in 1..volte {
            for o in &originali {
                let mut m = o.clone();
                let id = intestazioni(&m.grezzo[m.albero.intestazioni.clone()])
                    .into_iter()
                    .find(|(k, _)| k == "message-id")
                    .map(|(_, v)| v);
                if let Some(id) = id {
                    let nuovo = id.replacen('<', &format!("<copia{k}-"), 1);
                    m.riscrivi("Message-ID", &nuovo);
                }
                m.uid = self.prossimo_uid;
                self.prossimo_uid += 1;
                m.msgid = self.prossimo_msgid;
                self.prossimo_msgid += 1;
                m.thrid = (o.thrid ^ ((k as u64) << 52)) & 0x7fff_ffff_ffff_ffff;
                self.modseq += 1;
                m.modseq = self.modseq;
                let mut a = (*m.attese).clone();
                a.id = format!("{}-copia{k}", a.id);
                m.attese = Arc::new(a);
                self.messaggi.push(m);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Il server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Opzioni {
    pub utente: String,
    pub password: String,
    /// Ogni quanto arriva un messaggio nuovo.
    pub arrivi: Option<Duration>,
    /// Ogni quanto cadono tutte le connessioni.
    pub cadute: Option<Duration>,
    /// Dopo una caduta, per quanto le connessioni nuove vengono chiuse subito.
    pub pausa: Duration,
    /// Stampa ogni comando ricevuto (la password di LOGIN no).
    pub verboso: bool,
    /// Tiene in memoria i comandi ricevuti (per le prove).
    pub registra: bool,
    /// Prima di rispondere a ogni comando aspetta tanto così: un server lento
    /// (anche il LOGOUT, per provare che la chiusura non lo aspetta).
    pub ritardo: Duration,
    /// Chiude, con `* BYE` come Gmail, le connessioni senza comandi da tanto così
    /// (anche quelle in IDLE da tanto così senza un IDLE nuovo).
    pub chiudi_ferme: Option<Duration>,
}

impl Default for Opzioni {
    fn default() -> Self {
        Opzioni {
            utente: UTENTE_PREDEFINITO.into(),
            password: PASSWORD_PREDEFINITA.into(),
            arrivi: None,
            cadute: None,
            pausa: Duration::from_secs(5),
            verboso: false,
            registra: true,
            ritardo: Duration::ZERO,
            chiudi_ferme: None,
        }
    }
}

#[derive(Debug)]
pub struct Stato {
    pub casella: Casella,
    pub comandi: Vec<String>,
    pub connessioni: u32,
    pub cadute: u32,
    aperte: HashMap<u64, TcpStream>,
    rifiuta_fino: Option<Instant>,
}

pub struct Server {
    pub porta: u16,
    pub stato: Arc<Mutex<Stato>>,
    fermo: Arc<AtomicBool>,
}

fn ora() -> String {
    tempo::ora_utc()
}

impl Server {
    /// Avvia il server su `indirizzo` (per esempio `127.0.0.1:1143`, o `127.0.0.1:0` per una porta libera).
    pub fn avvia(casella: Casella, opz: Opzioni, indirizzo: &str) -> std::io::Result<Server> {
        let ascolto = TcpListener::bind(indirizzo)?;
        let porta = ascolto.local_addr()?.port();
        let stato = Arc::new(Mutex::new(Stato {
            casella,
            comandi: Vec::new(),
            connessioni: 0,
            cadute: 0,
            aperte: HashMap::new(),
            rifiuta_fino: None,
        }));
        let fermo = Arc::new(AtomicBool::new(false));
        let opz = Arc::new(opz);
        {
            let (stato, fermo, opz) = (stato.clone(), fermo.clone(), opz.clone());
            std::thread::spawn(move || {
                let numeri = AtomicU64::new(0);
                for c in ascolto.incoming() {
                    if fermo.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(c) = c else { continue };
                    let rifiuta = {
                        let s = stato.lock().unwrap();
                        s.rifiuta_fino.is_some_and(|t| Instant::now() < t)
                    };
                    if rifiuta {
                        let _ = c.shutdown(Shutdown::Both);
                        continue;
                    }
                    let n = numeri.fetch_add(1, Ordering::Relaxed);
                    let (stato, opz) = (stato.clone(), opz.clone());
                    std::thread::spawn(move || {
                        if let Ok(copia) = c.try_clone() {
                            stato.lock().unwrap().aperte.insert(n, copia);
                        }
                        let _ = servi(c, &stato, &opz);
                        stato.lock().unwrap().aperte.remove(&n);
                    });
                }
            });
        }
        if let Some(ogni) = opz.arrivi {
            let (stato, fermo) = (stato.clone(), fermo.clone());
            std::thread::spawn(move || {
                while !dormi(ogni, &fermo) {
                    let mut s = stato.lock().unwrap();
                    let uid = s.casella.arriva(tempo::adesso());
                    let n = s.casella.messaggi.len();
                    drop(s);
                    println!(
                        "[{}] arrivato un messaggio nuovo: UID {uid}, {n} in tutto",
                        ora()
                    );
                }
            });
        }
        if let Some(ogni) = opz.cadute {
            let (stato, fermo, pausa) = (stato.clone(), fermo.clone(), opz.pausa);
            std::thread::spawn(move || {
                while !dormi(ogni, &fermo) {
                    let mut s = stato.lock().unwrap();
                    let quante = s.aperte.len();
                    for c in s.aperte.values() {
                        let _ = c.shutdown(Shutdown::Both);
                    }
                    s.aperte.clear();
                    s.cadute += 1;
                    s.rifiuta_fino = Some(Instant::now() + pausa);
                    drop(s);
                    println!(
                        "[{}] faccio cadere {quante} connessioni; rifiuto le nuove per {} s",
                        ora(),
                        pausa.as_secs()
                    );
                }
            });
        }
        Ok(Server {
            porta,
            stato,
            fermo,
        })
    }

    pub fn con<R>(&self, f: impl FnOnce(&mut Stato) -> R) -> R {
        f(&mut self.stato.lock().unwrap())
    }

    pub fn comandi(&self) -> Vec<String> {
        self.con(|s| s.comandi.clone())
    }

    /// Quante connessioni dei client sono aperte adesso.
    pub fn connessioni_aperte(&self) -> usize {
        self.con(|s| s.aperte.len())
    }

    /// Fa cadere subito tutte le connessioni aperte e rifiuta le nuove per `pausa`.
    pub fn fai_cadere(&self, pausa: Duration) {
        self.con(|s| {
            for c in s.aperte.values() {
                let _ = c.shutdown(Shutdown::Both);
            }
            s.aperte.clear();
            s.cadute += 1;
            s.rifiuta_fino = Some(Instant::now() + pausa);
        });
    }

    pub fn ferma(&self) {
        self.fermo.store(true, Ordering::Relaxed);
        self.con(|s| {
            for c in s.aperte.values() {
                let _ = c.shutdown(Shutdown::Both);
            }
        });
        // Sveglia il ciclo di accept.
        let _ = TcpStream::connect(("127.0.0.1", self.porta));
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.ferma();
    }
}

/// Dorme `d`; vero se nel frattempo è arrivato lo stop.
fn dormi(d: Duration, fermo: &AtomicBool) -> bool {
    let fine = Instant::now() + d;
    while Instant::now() < fine {
        if fermo.load(Ordering::Relaxed) {
            return true;
        }
        std::thread::sleep((fine - Instant::now()).min(Duration::from_millis(100)));
    }
    fermo.load(Ordering::Relaxed)
}

const CAPACITA: &str =
    "IMAP4rev1 UNSELECT IDLE NAMESPACE ID CHILDREN X-GM-EXT-1 UIDPLUS ENABLE CONDSTORE SPECIAL-USE";

/// Le stringhe di una riga di argomenti: tra virgolette o atomi.
fn argomenti(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut dentro = false;
    let mut escape = false;
    let mut aperta = false;
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
                aperta = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            dentro = true;
            aperta = true;
        } else if c == ' ' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() || aperta {
        out.push(cur);
    }
    out
}

fn nell_insieme(insieme: &str, n: u32, massimo: u32) -> bool {
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
                n >= a.min(b) && n <= a.max(b)
            }
            None => n == num(pezzo),
        }
    })
}

/// Etichette di Gmail in UTF-7 modificato (RFC 3501, 5.1.3).
fn utf7(s: &str) -> String {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";
    let mut out = String::new();
    let mut fuori: Vec<u16> = Vec::new();
    let svuota = |fuori: &mut Vec<u16>, out: &mut String| {
        if fuori.is_empty() {
            return;
        }
        let byte: Vec<u8> = fuori.iter().flat_map(|u| u.to_be_bytes()).collect();
        out.push('&');
        let mut bits = 0u32;
        let mut n = 0;
        for b in byte {
            bits = (bits << 8) | b as u32;
            n += 8;
            while n >= 6 {
                n -= 6;
                out.push(B64[((bits >> n) & 63) as usize] as char);
            }
        }
        if n > 0 {
            out.push(B64[((bits << (6 - n)) & 63) as usize] as char);
        }
        out.push('-');
        fuori.clear();
    };
    for c in s.chars() {
        if (' '..='~').contains(&c) {
            svuota(&mut fuori, &mut out);
            if c == '&' {
                out.push_str("&-");
            } else {
                out.push(c);
            }
        } else {
            let mut buf = [0u16; 2];
            fuori.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    svuota(&mut fuori, &mut out);
    out
}

/// Un elemento della lista di attributi di FETCH.
#[derive(Debug, PartialEq)]
enum Attributo {
    Semplice(String),
    /// BODY[sezione]<inizio.quanti>, con l'indicazione PEEK.
    Sezione {
        sezione: String,
        parziale: Option<(usize, Option<usize>)>,
        peek: bool,
    },
}

fn attributi(lista: &str) -> Vec<Attributo> {
    let s = lista.trim();
    let s = s
        .strip_prefix('(')
        .and_then(|x| x.strip_suffix(')'))
        .unwrap_or(s);
    let mut out = Vec::new();
    let mut i = 0;
    let b = s.as_bytes();
    while i < b.len() {
        if b[i] == b' ' {
            i += 1;
            continue;
        }
        let inizio = i;
        let mut quadre = 0;
        while i < b.len() && (quadre > 0 || b[i] != b' ') {
            match b[i] {
                b'[' => quadre += 1,
                b']' => quadre -= 1,
                _ => {}
            }
            i += 1;
        }
        let e = &s[inizio..i];
        let su = e.to_ascii_uppercase();
        if let Some(aperta) = su.find('[') {
            let chiusa = su.rfind(']').unwrap_or(su.len());
            let sezione = su[aperta + 1..chiusa.max(aperta + 1)].to_string();
            let resto = &su[chiusa.min(su.len())..];
            let parziale = resto
                .find('<')
                .and_then(|p| resto[p + 1..].strip_suffix('>'))
                .and_then(|x| match x.split_once('.') {
                    Some((a, q)) => Some((a.parse().ok()?, Some(q.parse().ok()?))),
                    None => Some((x.parse().ok()?, None)),
                });
            out.push(Attributo::Sezione {
                sezione,
                parziale,
                peek: su.starts_with("BODY.PEEK") || su.starts_with("BINARY.PEEK"),
            });
        } else {
            out.push(Attributo::Semplice(su));
        }
    }
    out
}

/// Il contenuto di una sezione: `HEADER.FIELDS (…)`, `HEADER`, `TEXT`, `1.2`, `1.2.MIME`, vuota.
fn sezione(m: &Messaggio, sez: &str) -> Option<Vec<u8>> {
    let g = &m.grezzo[..];
    let r = &m.albero;
    if sez.is_empty() {
        return Some(g.to_vec());
    }
    if let Some(resto) = sez.strip_prefix("HEADER.FIELDS") {
        let (escludi, lista) = match resto.strip_prefix(".NOT") {
            Some(l) => (true, l),
            None => (false, resto),
        };
        let nomi: Vec<String> = lista
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split_whitespace()
            .map(|x| x.trim_matches('"').to_string())
            .collect();
        return Some(campi_intestazione(
            g,
            r.intestazioni.clone(),
            &nomi,
            escludi,
        ));
    }
    if sez == "HEADER" {
        return Some(g[r.intestazioni.clone()].to_vec());
    }
    if sez == "TEXT" {
        return Some(g[r.corpo.clone()].to_vec());
    }
    let (numeri, coda) = match sez.split_once(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(_) => {
            let pos = sez.find(".MIME").or_else(|| sez.find(".HEADER"))?;
            (&sez[..pos], &sez[pos + 1..])
        }
        None => (sez, ""),
    };
    let numeri: Vec<u32> = numeri
        .split('.')
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    let p = parte(r, &numeri)?;
    match coda {
        "" => Some(g[p.corpo.clone()].to_vec()),
        "MIME" | "HEADER" => Some(g[p.intestazioni.clone()].to_vec()),
        _ => None,
    }
}

/// Le righe di risposta a FETCH per un messaggio.
fn risposta_fetch(
    m: &Messaggio,
    seq: usize,
    attr: &[Attributo],
    uid: bool,
    condstore: bool,
) -> Vec<u8> {
    let mut campi: Vec<Vec<u8>> = Vec::new();
    let vuole = |n: &str| {
        attr.iter()
            .any(|a| matches!(a, Attributo::Semplice(x) if x == n))
    };
    if uid || vuole("UID") {
        campi.push(format!("UID {}", m.uid).into_bytes());
    }
    if vuole("FLAGS") || vuole("FAST") || vuole("ALL") || vuole("FULL") {
        campi.push(format!("FLAGS ({})", m.flags.join(" ")).into_bytes());
    }
    if vuole("INTERNALDATE") || vuole("FAST") || vuole("ALL") || vuole("FULL") {
        campi
            .push(format!("INTERNALDATE \"{}\"", tempo::internaldate(m.data_interna)).into_bytes());
    }
    if vuole("RFC822.SIZE") || vuole("FAST") || vuole("ALL") || vuole("FULL") {
        campi.push(format!("RFC822.SIZE {}", m.grezzo.len()).into_bytes());
    }
    if vuole("MODSEQ") || condstore {
        campi.push(format!("MODSEQ ({})", m.modseq).into_bytes());
    }
    if vuole("X-GM-MSGID") {
        campi.push(format!("X-GM-MSGID {}", m.msgid).into_bytes());
    }
    if vuole("X-GM-THRID") {
        campi.push(format!("X-GM-THRID {}", m.thrid).into_bytes());
    }
    if vuole("X-GM-LABELS") {
        let mut c = b"X-GM-LABELS (".to_vec();
        for (i, e) in m.etichette.iter().enumerate() {
            if i > 0 {
                c.push(b' ');
            }
            // Come Gmail: atomi quando si può (`\Inbox`, `Casa`), altrimenti tra virgolette.
            let e = utf7(e);
            let atomo = !e.is_empty()
                && e.char_indices().all(|(i, x)| {
                    x.is_ascii_graphic() && !"(){%*\"]".contains(x) && (x != '\\' || i == 0)
                });
            if atomo {
                c.extend_from_slice(e.as_bytes());
            } else {
                stringa(&e, &mut c);
            }
        }
        c.push(b')');
        campi.push(c);
    }
    if vuole("BODYSTRUCTURE") || vuole("FULL") {
        let mut c = b"BODYSTRUCTURE ".to_vec();
        struttura(&m.albero, &m.grezzo, &mut c);
        campi.push(c);
    }
    if vuole("RFC822.HEADER") {
        let h = &m.grezzo[m.albero.intestazioni.clone()];
        let mut c = format!("RFC822.HEADER {{{}}}\r\n", h.len()).into_bytes();
        c.extend_from_slice(h);
        campi.push(c);
    }
    for a in attr {
        if let Attributo::Sezione {
            sezione: sez,
            parziale,
            ..
        } = a
        {
            let mut dati = sezione(m, sez).unwrap_or_default();
            let chiave = match parziale {
                Some((inizio, quanti)) => {
                    let inizio = (*inizio).min(dati.len());
                    let fine = quanti.map_or(dati.len(), |q| (inizio + q).min(dati.len()));
                    dati = dati[inizio..fine].to_vec();
                    format!("BODY[{sez}]<{inizio}>")
                }
                None => format!("BODY[{sez}]"),
            };
            let mut c = format!("{chiave} {{{}}}\r\n", dati.len()).into_bytes();
            c.extend_from_slice(&dati);
            campi.push(c);
        }
    }
    let mut out = format!("* {seq} FETCH (").into_bytes();
    out.extend_from_slice(&campi.join(&b' '));
    out.extend_from_slice(b")\r\n");
    out
}

fn registra(stato: &Mutex<Stato>, opz: &Opzioni, riga: &str) {
    let mostrata = if riga
        .split(' ')
        .nth(1)
        .is_some_and(|c| c.eq_ignore_ascii_case("LOGIN"))
    {
        let mut p = riga.splitn(4, ' ');
        format!(
            "{} {} {} ***",
            p.next().unwrap_or(""),
            p.next().unwrap_or(""),
            p.next().unwrap_or("")
        )
    } else {
        riga.to_string()
    };
    if opz.verboso {
        let breve: String = mostrata.chars().take(200).collect();
        println!("[{}] < {breve}", ora());
    }
    if opz.registra {
        stato.lock().unwrap().comandi.push(mostrata);
    }
}

fn servi(c: TcpStream, stato: &Mutex<Stato>, opz: &Opzioni) -> std::io::Result<()> {
    // Ogni risposta parte subito: senza, Nagle e l'ACK ritardato aggiungono 40 ms a giro.
    c.set_nodelay(true)?;
    let mut w = c.try_clone()?;
    let mut r = BufReader::new(c);
    stato.lock().unwrap().connessioni += 1;
    write!(
        w,
        "* OK [CAPABILITY {CAPACITA}] Finto server IMAP di Ambassadress pronto\r\n"
    )?;
    let mut autenticato = false;
    let mut aperta = false;
    let mut condstore = false;
    let mut conosciuti = 0usize;
    r.get_ref().set_read_timeout(opz.chiudi_ferme)?;
    let ferma = |w: &mut TcpStream| -> std::io::Result<()> {
        let _ = write!(w, "* BYE Autologout; idle for too long\r\n");
        println!(
            "[{}] chiudo una connessione ferma da {} s",
            ora(),
            opz.chiudi_ferme.unwrap_or_default().as_secs()
        );
        w.shutdown(Shutdown::Both)
    };
    loop {
        let mut riga = String::new();
        match r.read_line(&mut riga) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return ferma(&mut w);
            }
            Err(e) => return Err(e),
        }
        let riga = riga.trim_end_matches(['\r', '\n']).to_string();
        registra(stato, opz, &riga);
        if !opz.ritardo.is_zero() {
            std::thread::sleep(opz.ritardo);
        }
        let Some((etichetta, resto)) = riga.split_once(' ') else {
            write!(w, "* BAD riga senza etichetta\r\n")?;
            continue;
        };
        let (comando, args) = resto.split_once(' ').unwrap_or((resto, ""));
        let comando = comando.to_ascii_uppercase();
        let libero = matches!(
            comando.as_str(),
            "CAPABILITY" | "NOOP" | "LOGOUT" | "ID" | "LOGIN"
        );
        if !autenticato && !libero {
            write!(w, "{etichetta} NO prima serve LOGIN\r\n")?;
            continue;
        }
        match comando.as_str() {
            "CAPABILITY" => write!(w, "* CAPABILITY {CAPACITA}\r\n{etichetta} OK fatto\r\n")?,
            "LOGIN" => {
                let v = argomenti(args);
                if v.len() == 2 && v[0] == opz.utente && v[1] == opz.password {
                    autenticato = true;
                    write!(
                        w,
                        "{etichetta} OK [CAPABILITY {CAPACITA}] {} autenticato\r\n",
                        opz.utente
                    )?;
                } else {
                    write!(
                        w,
                        "{etichetta} NO [AUTHENTICATIONFAILED] Invalid credentials (Failure)\r\n"
                    )?;
                }
            }
            "ID" => write!(
                w,
                "* ID (\"name\" \"finto-server-a10s\")\r\n{etichetta} OK fatto\r\n"
            )?,
            "ENABLE" => {
                let su = args.to_ascii_uppercase();
                let mut abilitate = Vec::new();
                if su.split(' ').any(|x| x == "CONDSTORE") {
                    condstore = true;
                    abilitate.push("CONDSTORE");
                }
                write!(
                    w,
                    "* ENABLED {}\r\n{etichetta} OK fatto\r\n",
                    abilitate.join(" ")
                )?;
            }
            "NAMESPACE" => write!(
                w,
                "* NAMESPACE ((\"\" \"/\")) NIL NIL\r\n{etichetta} OK fatto\r\n"
            )?,
            "LIST" => write!(
                w,
                "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n\
                 * LIST (\\HasChildren \\Noselect) \"/\" \"[Gmail]\"\r\n\
                 * LIST (\\All \\HasNoChildren) \"/\" \"{CARTELLA_TUTTI}\"\r\n\
                 * LIST (\\HasNoChildren \\Sent) \"/\" \"[Gmail]/Posta inviata\"\r\n\
                 * LIST (\\HasNoChildren \\Junk) \"/\" \"[Gmail]/Spam\"\r\n\
                 * LIST (\\HasNoChildren \\Trash) \"/\" \"[Gmail]/Cestino\"\r\n\
                 {etichetta} OK fatto\r\n"
            )?,
            "EXAMINE" => {
                let v = argomenti(args);
                if v.first().map(String::as_str) != Some(CARTELLA_TUTTI) {
                    write!(
                        w,
                        "{etichetta} NO [CANNOT] questo finto server apre solo «{CARTELLA_TUTTI}»\r\n"
                    )?;
                    continue;
                }
                if args.to_ascii_uppercase().contains("(CONDSTORE)") {
                    condstore = true;
                }
                let s = stato.lock().unwrap();
                let c = &s.casella;
                conosciuti = c.messaggi.len();
                aperta = true;
                write!(
                    w,
                    "* FLAGS (\\Answered \\Flagged \\Draft \\Deleted \\Seen $NotPhishing $Phishing)\r\n\
                     * OK [PERMANENTFLAGS ()] Flags permitted.\r\n\
                     * OK [UIDVALIDITY {}] UIDs valid.\r\n\
                     * {} EXISTS\r\n* 0 RECENT\r\n\
                     * OK [UIDNEXT {}] Predicted next UID.\r\n\
                     * OK [HIGHESTMODSEQ {}]\r\n\
                     {etichetta} OK [READ-ONLY] {CARTELLA_TUTTI} selected. (Success)\r\n",
                    c.uidvalidity,
                    c.messaggi.len(),
                    c.prossimo_uid,
                    c.modseq
                )?;
            }
            "NOOP" => {
                let n = stato.lock().unwrap().casella.messaggi.len();
                if aperta && n != conosciuti {
                    write!(w, "* {n} EXISTS\r\n")?;
                    conosciuti = n;
                }
                write!(w, "{etichetta} OK fatto\r\n")?;
            }
            "IDLE" => {
                write!(w, "+ idling\r\n")?;
                r.get_ref()
                    .set_read_timeout(Some(Duration::from_millis(50)))?;
                let mut acc = String::new();
                let inizio_idle = Instant::now();
                loop {
                    if opz.chiudi_ferme.is_some_and(|f| inizio_idle.elapsed() >= f) {
                        return ferma(&mut w);
                    }
                    match r.read_line(&mut acc) {
                        Ok(0) => return Ok(()),
                        Ok(_) => {
                            if acc.ends_with('\n') {
                                let t = acc.trim().to_string();
                                acc.clear();
                                registra(stato, opz, &t);
                                if t.eq_ignore_ascii_case("DONE") {
                                    break;
                                }
                                write!(w, "* BAD durante IDLE si aspetta DONE\r\n")?;
                            }
                        }
                        Err(e)
                            if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                        Err(e) => return Err(e),
                    }
                    let n = stato.lock().unwrap().casella.messaggi.len();
                    if aperta && n != conosciuti {
                        write!(w, "* {n} EXISTS\r\n")?;
                        conosciuti = n;
                    }
                }
                r.get_ref().set_read_timeout(opz.chiudi_ferme)?;
                write!(w, "{etichetta} OK IDLE terminated (Success)\r\n")?;
            }
            "LOGOUT" => {
                write!(w, "* BYE LOGOUT Requested\r\n{etichetta} OK 73 GoodBye\r\n")?;
                return Ok(());
            }
            "UID" | "FETCH" | "SEARCH" => {
                if !aperta {
                    write!(w, "{etichetta} BAD nessuna cartella aperta\r\n")?;
                    continue;
                }
                let (uid, sotto, resto) = if comando == "UID" {
                    let (s, r) = args.split_once(' ').unwrap_or((args, ""));
                    (true, s.to_ascii_uppercase(), r)
                } else {
                    (false, comando.clone(), args)
                };
                match sotto.as_str() {
                    "SEARCH" => {
                        let s = stato.lock().unwrap();
                        let v: Vec<String> = s
                            .casella
                            .messaggi
                            .iter()
                            .enumerate()
                            .map(|(i, m)| if uid { m.uid } else { i as u32 + 1 }.to_string())
                            .collect();
                        let mut out = format!("* SEARCH {}\r\n", v.join(" ")).into_bytes();
                        if v.is_empty() {
                            out = b"* SEARCH\r\n".to_vec();
                        }
                        out.extend_from_slice(
                            format!("{etichetta} OK SEARCH completed (Success)\r\n").as_bytes(),
                        );
                        w.write_all(&out)?;
                    }
                    "FETCH" => {
                        let (insieme, resto) = resto.split_once(' ').unwrap_or((resto, ""));
                        // Lista degli attributi e modificatori (CHANGEDSINCE).
                        let resto = resto.trim();
                        let (lista, modificatori) = if resto.starts_with('(') {
                            let mut prof = 0i32;
                            let mut quadre = 0i32;
                            let mut fine = resto.len();
                            for (i, c) in resto.char_indices() {
                                match c {
                                    '[' => quadre += 1,
                                    ']' => quadre -= 1,
                                    '(' if quadre == 0 => prof += 1,
                                    ')' if quadre == 0 => {
                                        prof -= 1;
                                        if prof == 0 {
                                            fine = i + 1;
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            (&resto[..fine], resto[fine..].trim())
                        } else {
                            let fine = resto.find(' ').unwrap_or(resto.len());
                            (&resto[..fine], resto[fine..].trim())
                        };
                        let su = modificatori.to_ascii_uppercase();
                        let da_quando: Option<u64> = su.find("CHANGEDSINCE ").and_then(|i| {
                            su[i + 13..]
                                .split([' ', ')'])
                                .next()
                                .and_then(|x| x.parse().ok())
                        });
                        let attr = attributi(lista);
                        let s = stato.lock().unwrap();
                        let msgs = &s.casella.messaggi;
                        let massimo = if uid {
                            msgs.last().map_or(0, |m| m.uid)
                        } else {
                            msgs.len() as u32
                        };
                        let mut out = Vec::new();
                        for (i, m) in msgs.iter().enumerate() {
                            let n = if uid { m.uid } else { i as u32 + 1 };
                            if !nell_insieme(insieme, n, massimo) {
                                continue;
                            }
                            if da_quando.is_some_and(|d| m.modseq <= d) {
                                continue;
                            }
                            out.extend(risposta_fetch(
                                m,
                                i + 1,
                                &attr,
                                uid,
                                condstore && da_quando.is_some(),
                            ));
                        }
                        out.extend_from_slice(format!("{etichetta} OK Success\r\n").as_bytes());
                        drop(s);
                        w.write_all(&out)?;
                    }
                    _ => write!(
                        w,
                        "{etichetta} NO [CANNOT] sola lettura: UID {sotto} non si fa qui\r\n"
                    )?,
                }
            }
            "SELECT" | "STORE" | "APPEND" | "COPY" | "MOVE" | "EXPUNGE" | "DELETE" | "CREATE"
            | "RENAME" | "SUBSCRIBE" | "UNSUBSCRIBE" | "CLOSE" => write!(
                w,
                "{etichetta} NO [CANNOT] finto server di sola lettura\r\n"
            )?,
            "UNSELECT" => {
                aperta = false;
                write!(w, "{etichetta} OK fatto\r\n")?;
            }
            _ => write!(
                w,
                "{etichetta} BAD comando non previsto dal finto server\r\n"
            )?,
        }
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn utf7_etichette() {
        assert_eq!(utf7("Priorità"), "Priorit&AOA-");
        assert_eq!(utf7("A&B"), "A&-B");
        assert_eq!(utf7("日本語"), "&ZeVnLIqe-");
    }
}
