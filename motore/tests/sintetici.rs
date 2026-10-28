//! Il motore contro il finto server con tutta la casella sintetica del prototipo,
//! trasformata in `.eml` veri da `tools/sintetici-in-eml.mjs`: conteggi, testi
//! identici all'originale (impronte nei `.meta`), allegati scaricati e decodificati,
//! vista JSON, servizio con arrivi e cadute, e la guardia che non ha mai visto
//! comandi vietati. Serve `node` per generare i messaggi: senza, le prove si saltano.

#[path = "../examples/finto_server/finto.rs"]
mod finto;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use finto::{Casella, Messaggio, Opzioni as OpzioniServer, Server};
use motore::ascolta::OpzioniAscolto;
use motore::credenziali::{Credenziali, Segreto};
use motore::db::{Db, MisureParte};
use motore::guardia::Guardia;
use motore::mime::fnv64;
use motore::rete::{Contatori, Indirizzo, Sicurezza};
use motore::scaricamento::{self, Pausa};
use motore::servizio::{OpzioniServizio, Servizio};
use motore::sincronizza::{Collegamento, Config, Notizia, Opzioni, sincronizza};
use motore::su_richiesta;
use motore::vista::{self, Account, Casella as CasellaVista, OpzioniVista};

const PREFISSO: &str = "a10s://localhost/allegato/";

/// I messaggi generati una volta per tutto il processo delle prove.
fn sintetici() -> Option<&'static Path> {
    static CARTELLA: OnceLock<Option<PathBuf>> = OnceLock::new();
    CARTELLA
        .get_or_init(|| {
            // Sempre la stessa cartella dentro target/: lo script la svuota e la riscrive.
            let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sintetici");
            let script =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/sintetici-in-eml.mjs");
            let esito = Command::new("node")
                .arg(&script)
                .arg("--uscita")
                .arg(&dir)
                .output();
            match esito {
                Ok(o) if o.status.success() => Some(dir),
                Ok(o) => panic!(
                    "sintetici-in-eml.mjs non è riuscito:\n{}",
                    String::from_utf8_lossy(&o.stderr)
                ),
                Err(_) => {
                    eprintln!("node non c'è: salto le prove sulla casella sintetica");
                    None
                }
            }
        })
        .as_deref()
}

struct Cartella(PathBuf);

impl Cartella {
    fn nuova(nome: &str) -> Self {
        let p = std::env::temp_dir().join(format!("motore-sint-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Cartella(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("prova.sqlite")
    }
}

impl Drop for Cartella {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(server: &Server, password: &str) -> Config {
    Config {
        indirizzo: Indirizzo {
            host: "127.0.0.1".into(),
            porta: server.porta,
            sicurezza: Sicurezza::InChiaro,
        },
        credenziali: Credenziali {
            utente: Segreto::new(finto::UTENTE_PREDEFINITO.into()),
            password: Segreto::new(password.into()),
        },
        timeout: Duration::from_secs(5),
    }
}

fn avvia(casella: Casella, opz: OpzioniServer) -> Server {
    Server::avvia(casella, opz, "127.0.0.1:0").expect("porta libera")
}

fn tutto() -> Opzioni {
    Opzioni {
        limite: None,
        corpi: 1_000_000,
        attesa_iniziale: Duration::from_millis(10),
        ..Opzioni::default()
    }
}

fn sincronizza_tutto(server: &Server, db: &mut Db, opz: &Opzioni) -> motore::sincronizza::Rapporto {
    let mut col = Collegamento::new(
        config(server, finto::PASSWORD_PREDEFINITA),
        Arc::new(Contatori::default()),
    );
    let r = sincronizza(&mut col, db, opz);
    col.chiudi();
    if let Some(e) = &r.errore {
        panic!("sincronizzazione non riuscita: {e}");
    }
    r
}

/// Ogni comando arrivato al server deve passare la guardia, e nessuno legge il corpo
/// senza PEEK o apre una cartella in scrittura.
fn guardia_pulita(server: &Server) -> usize {
    let comandi = server.comandi();
    for c in &comandi {
        if c.eq_ignore_ascii_case("DONE") {
            continue;
        }
        Guardia::new()
            .verifica(format!("{c}\r\n").as_bytes())
            .unwrap_or_else(|e| {
                panic!("comando che la guardia non avrebbe lasciato uscire: {c}: {e}")
            });
        let su = c.to_ascii_uppercase().replace("BODY.PEEK[", "");
        for vietato in [
            "BODY[", "SELECT ", "STORE ", "APPEND ", "EXPUNGE", "COPY ", "MOVE ",
        ] {
            assert!(
                !su.contains(vietato),
                "comando vietato arrivato al server: {c}"
            );
        }
    }
    comandi.len()
}

fn normalizzato(s: &str) -> (usize, u64) {
    let t = s.replace("\r\n", "\n");
    let t = t.trim_end();
    (t.len(), fnv64(t.as_bytes()))
}

fn opzioni_vista() -> OpzioniVista {
    OpzioniVista {
        account: Account {
            indirizzo: "alex.moro@esempio.it".into(),
            nome: "Alex Moro".into(),
            alias: vec!["alex@studiomoro.esempio.it".into()],
        },
        caselle: vec![CasellaVista {
            indirizzo: "alex.moro@esempio.it".into(),
            tipo: "gmail".into(),
            server: "127.0.0.1".into(),
        }],
        prefisso_allegati: PREFISSO.into(),
        limite: None,
        solo: None,
    }
}

#[test]
fn tutta_la_casella_sintetica() {
    let Some(dir) = sintetici() else { return };
    let (casella, _) = Casella::carica(dir).unwrap();
    let attesi = casella.messaggi.clone();
    let server = avvia(casella, OpzioniServer::default());
    let cartella = Cartella::nuova("tutto");
    let mut db = Db::apri(&cartella.db()).unwrap();
    let (tx, rx) = mpsc::channel();
    let opz = Opzioni {
        notizie: Some(tx),
        ..tutto()
    };
    let r = sincronizza_tutto(&server, &mut db, &opz);
    drop(opz);

    // Conteggi
    let thread: HashSet<u64> = attesi.iter().map(|m| m.thrid).collect();
    let allegati: usize = attesi
        .iter()
        .map(|m| m.attese.allegati.iter().filter(|a| !a.incorporato).count())
        .sum();
    assert_eq!(r.nuovi, attesi.len());
    assert_eq!(r.allegati, allegati);
    assert_eq!(r.corpi, attesi.len());
    assert_eq!(r.malformati + r.senza_struttura + r.da_riprovare, 0);
    assert_eq!(
        db.conta("SELECT count(*) FROM messaggi").unwrap() as usize,
        attesi.len()
    );
    assert_eq!(
        db.conta("SELECT count(*) FROM thread").unwrap() as usize,
        thread.len()
    );
    assert_eq!(
        db.conta("SELECT count(*) FROM parti WHERE ruolo = 'allegato'")
            .unwrap() as usize,
        allegati
    );
    assert_eq!(
        db.conta("SELECT count(*) FROM messaggi WHERE corpo = 1")
            .unwrap() as usize,
        attesi.len()
    );
    eprintln!(
        "casella sintetica: {} messaggi, {} thread, {} allegati, {} testi in {:?}",
        attesi.len(),
        thread.len(),
        allegati,
        r.corpi,
        r.durata
    );

    // Notizie: inizio, passi crescenti per le due fasi, fine.
    let notizie: Vec<Notizia> = rx.try_iter().collect();
    assert_eq!(notizie.first(), Some(&Notizia::Inizio));
    assert!(matches!(notizie.last(), Some(Notizia::Fine { nuovi, .. }) if *nuovi == attesi.len()));
    assert!(notizie.iter().any(|n| matches!(n,
        Notizia::Passo { fase: motore::sincronizza::Fase::Testi, fatti, totale } if fatti == totale && *totale == attesi.len())));

    // Testi e HTML identici all'originale, e struttura numerata come la scrive lo script.
    let conn = rusqlite::Connection::open(cartella.db()).unwrap();
    let mut confrontati = (0, 0, 0);
    for m in &attesi {
        let (id, testo, html): (i64, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT m.id, c.testo, c.html FROM messaggi m LEFT JOIN corpi c ON c.messaggio_id = m.id
                 WHERE m.gm_msgid = ?1",
                [m.msgid as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap_or_else(|_| panic!("{} non è nel database", m.attese.id));
        if let Some(t) = m.attese.testo {
            assert_eq!(
                normalizzato(testo.as_deref().unwrap_or("")),
                t,
                "testo di {}",
                m.attese.id
            );
            confrontati.0 += 1;
        }
        if let Some(h) = m.attese.html {
            assert_eq!(
                normalizzato(html.as_deref().unwrap_or("")),
                h,
                "html di {}",
                m.attese.id
            );
            confrontati.1 += 1;
        }
        for a in &m.attese.allegati {
            let (tipo, nome): (String, Option<String>) = conn
                .query_row(
                    "SELECT tipo, nome_file FROM parti WHERE messaggio_id = ?1 AND parte = ?2",
                    rusqlite::params![id, a.parte],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or_else(|_| panic!("parte {} di {} mancante", a.parte, m.attese.id));
            assert_eq!(
                tipo, a.tipo,
                "tipo della parte {} di {}",
                a.parte, m.attese.id
            );
            assert_eq!(
                nome.as_deref(),
                Some(a.nome.as_str()),
                "nome della parte {} di {}",
                a.parte,
                m.attese.id
            );
            confrontati.2 += 1;
        }
    }
    eprintln!(
        "confrontati {} testi, {} HTML e {} allegati con l'originale",
        confrontati.0, confrontati.1, confrontati.2
    );
    assert!(confrontati.0 > 350 && confrontati.1 > 20);

    // Un secondo giro non trova niente da fare.
    let r = sincronizza_tutto(&server, &mut db, &tutto());
    assert!(r.invariata && r.nuovi == 0);

    let n = guardia_pulita(&server);
    eprintln!("{n} comandi ricevuti dal server, tutti ammessi dalla guardia");
}

#[test]
fn vista_json_come_il_prototipo() {
    let Some(dir) = sintetici() else { return };
    let (casella, _) = Casella::carica(dir).unwrap();
    let n = casella.messaggi.len();
    let server = avvia(casella, OpzioniServer::default());
    let cartella = Cartella::nuova("vista");
    let mut db = Db::apri(&cartella.db()).unwrap();
    // Testi solo dei 100 più recenti: gli altri devono avere bodyMissing.
    sincronizza_tutto(
        &server,
        &mut db,
        &Opzioni {
            corpi: 100,
            ..tutto()
        },
    );

    let t = Instant::now();
    let json = vista::json(&db, &opzioni_vista()).unwrap();
    eprintln!(
        "vista di {n} messaggi: {} KB in {:?}",
        json.len() / 1024,
        t.elapsed()
    );
    assert!(json.starts_with(r#"{"schema":1,"meta":{"source":"imap","exportedAt":""#));
    assert!(json.contains(&format!(r#""count":{n}}}"#)));
    assert_eq!(json.matches(r#""bodyMissing":true"#).count(), n - 100);
    assert!(json.contains(r#"["Sender","Google Calendar <calendar-notification@google.com>"]"#));
    assert!(json.contains(r#"["List-Unsubscribe","<https://nova.esempio.it/unsubscribe>"]"#));
    assert!(json.contains(r#""otherParts":["text/calendar"]"#));
    assert!(json.contains(&format!(r#""url":"{PREFISSO}"#)));
    assert!(json.contains(r#""filename":"Pianta misure umidità.pdf""#));

    // Con il limite: solo gli N più recenti.
    let breve = vista::json(
        &db,
        &OpzioniVista {
            limite: Some(10),
            ..opzioni_vista()
        },
    )
    .unwrap();
    assert!(breve.contains(r#""count":10}"#));

    // La stessa forma dei dati sintetici, controllata da JavaScript.
    let file = cartella.0.join("vista.json");
    std::fs::write(&file, &json).unwrap();
    let sintetici = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mockups/data/synthetic.js");
    let controllo = r#"
const fs = require('fs'), vm = require('vm');
const v = JSON.parse(fs.readFileSync(process.argv[1], 'utf8'));
const c = { window: {} }; vm.runInNewContext(fs.readFileSync(process.argv[2], 'utf8'), c);
const s = c.window.AMB_DATA;
const err = (m) => { console.error(m); process.exit(1); };
const veri = s.messages.filter((m) => !m.gmail.labels.includes('\\Spam'));
if (v.messages.length !== veri.length) err('messaggi: ' + v.messages.length + ' invece di ' + veri.length);
for (const k of ['schema', 'meta', 'account', 'accounts', 'messages']) if (!(k in v)) err('manca ' + k);
const chiavi = ['id', 'date', 'from', 'to', 'cc', 'bcc', 'subject', 'messageId', 'inReplyTo', 'references', 'text', 'html', 'attachments', 'headers', 'flags', 'gmail', 'problems', 'otherParts'];
for (const m of v.messages) for (const k of chiavi) if (!(k in m)) err('manca ' + k + ' in ' + m.id);
const conta = (l, f) => { const o = {}; for (const m of l) { const k = f(m); o[k] = (o[k] || 0) + 1; } return JSON.stringify(Object.entries(o).sort()); };
const diversi = (a, b) => { const x = new Set(a), y = new Set(b); return JSON.stringify([[...x].filter((k) => !y.has(k)).slice(0, 3), [...y].filter((k) => !x.has(k)).slice(0, 3)]); };
// Un oggetto vuoto arriva come null: per il prototipo è lo stesso.
const ogg = (m) => m.subject || '';
if (conta(v.messages, ogg) !== conta(veri, ogg)) err('oggetti diversi: ' + diversi(v.messages.map(ogg), veri.map(ogg)));
if (conta(v.messages, (m) => m.from.address) !== conta(veri, (m) => m.from.address)) err('mittenti diversi');
if (conta(v.messages, (m) => m.to.length + '/' + m.cc.length) !== conta(veri, (m) => m.to.length + '/' + m.cc.length)) err('destinatari diversi');
const et = (m) => m.gmail.labels.slice().sort().join(',');
if (conta(v.messages, et) !== conta(veri, et)) err('etichette diverse: ' + diversi(v.messages.map(et), veri.map(et)));
if (conta(v.messages, (m) => m.flags.seen) !== conta(veri, (m) => m.flags.seen)) err('letti diversi');
if (conta(v.messages, (m) => m.attachments.map((a) => a.filename + '|' + a.mimeType).join(';')) !== conta(veri, (m) => m.attachments.map((a) => a.filename + '|' + a.mimeType).join(';'))) err('allegati diversi');
if (conta(v.messages, (m) => m.messageId) !== conta(veri, (m) => m.messageId || null)) err('Message-ID diversi');
if (new Set(v.messages.map((m) => m.gmail.threadId)).size !== new Set(veri.map((m) => m.gmail.threadId)).size) err('thread diversi');
const giorno = (d) => new Date(d).toDateString();
const ultimo = v.messages.map((m) => m.date || m.internalDate).sort().pop();
console.log('ok, ultimo messaggio del ' + giorno(ultimo));
"#;
    match Command::new("node")
        .arg("-e")
        .arg(controllo)
        .arg(&file)
        .arg(&sintetici)
        .output()
    {
        Ok(o) => assert!(
            o.status.success(),
            "la vista non ha la forma dei dati sintetici: {}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(_) => eprintln!("node non c'è: salto il controllo della forma"),
    }
    guardia_pulita(&server);
}

#[test]
fn testi_e_allegati_su_richiesta() {
    let Some(dir) = sintetici() else { return };
    let (casella, _) = Casella::carica(dir).unwrap();
    let attesi = casella.messaggi.clone();
    let server = avvia(casella, OpzioniServer::default());
    let cartella = Cartella::nuova("richiesta");
    let mut db = Db::apri(&cartella.db()).unwrap();
    sincronizza_tutto(
        &server,
        &mut db,
        &Opzioni {
            corpi: 0,
            ..tutto()
        },
    );
    let cfg = config(&server, finto::PASSWORD_PREDEFINITA);
    let conn = rusqlite::Connection::open(cartella.db()).unwrap();
    let id_di = |m: &Messaggio| -> i64 {
        conn.query_row(
            "SELECT id FROM messaggi WHERE gm_msgid = ?1",
            [m.msgid as i64],
            |r| r.get(0),
        )
        .unwrap()
    };

    // Testi su richiesta
    let scelti: Vec<&Messaggio> = attesi
        .iter()
        .filter(|m| m.attese.testo.is_some())
        .take(12)
        .collect();
    let ids: Vec<i64> = scelti.iter().map(|m| id_di(m)).collect();
    assert_eq!(
        su_richiesta::scarica_testi(&cfg, &mut db, &ids).unwrap(),
        ids.len()
    );
    assert_eq!(
        su_richiesta::scarica_testi(&cfg, &mut db, &ids).unwrap(),
        0,
        "già scaricati"
    );
    let json = vista::json(
        &db,
        &OpzioniVista {
            solo: Some(ids.clone()),
            ..opzioni_vista()
        },
    )
    .unwrap();
    assert!(json.contains(&format!(r#""count":{}}}"#, ids.len())));
    assert!(!json.contains("bodyMissing"));
    for (m, id) in scelti.iter().zip(&ids) {
        let testo: String = conn
            .query_row(
                "SELECT testo FROM corpi WHERE messaggio_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            Some(normalizzato(&testo)),
            m.attese.testo,
            "testo di {}",
            m.attese.id
        );
    }

    // Tutti gli allegati, su una connessione riusata: contenuto decodificato identico all'originale.
    let accessi_prima = server.con(|s| s.connessioni);
    let mut richieste = su_richiesta::Richieste::new(cfg.clone());
    let t = Instant::now();
    let mut quanti = 0;
    let mut impronte = HashSet::new();
    let mut byte = 0u64;
    for m in &attesi {
        let id = id_di(m);
        for a in &m.attese.allegati {
            let al = richieste.parte(&mut db, id, &a.parte).unwrap();
            let dati = std::fs::read(&al.percorso).unwrap();
            assert_eq!(
                dati.len(),
                a.byte,
                "byte di {} parte {}",
                m.attese.id,
                a.parte
            );
            assert_eq!(
                fnv64(&dati),
                a.impronta,
                "contenuto di {} parte {}",
                m.attese.id,
                a.parte
            );
            assert_eq!(al.tipo, a.tipo);
            assert_eq!(al.nome_file.as_deref(), Some(a.nome.as_str()));
            assert_eq!(al.impronta, motore::impronta::sha256_esadecimale(&dati));
            impronte.insert(al.impronta.clone());
            quanti += 1;
            byte += dati.len() as u64;
        }
    }
    // Solo i file di contenuto: accanto ai PDF ci sono anche le miniature.
    let file = std::fs::read_dir(su_richiesta::cartella_allegati(&db))
        .unwrap()
        .filter(|v| {
            su_richiesta::nome_di_contenuto(&v.as_ref().unwrap().file_name().to_string_lossy())
        })
        .count();
    eprintln!(
        "{quanti} allegati scaricati in {:?} ({} KB decodificati), {} file nell'archivio",
        t.elapsed(),
        byte / 1024,
        file
    );
    assert_eq!(file, impronte.len(), "stesso contenuto, un file solo");
    assert_eq!(
        server.con(|s| s.connessioni) - accessi_prima,
        1,
        "una connessione per tutti"
    );
    // Se la connessione cade, la richiesta dopo si ricollega da sola.
    server.fai_cadere(Duration::ZERO);
    let m = attesi
        .iter()
        .find(|m| m.attese.testo.is_some() && m.attese.allegati.is_empty())
        .unwrap();
    let testo = richieste.parte(&mut db, id_di(m), "1").unwrap();
    assert!(testo.byte > 0);
    drop(richieste);
    assert!(
        impronte.len() < quanti,
        "nei sintetici ci sono allegati uguali"
    );

    // La seconda volta non si va in rete.
    let prima = server.comandi().len();
    let m = attesi
        .iter()
        .find(|m| !m.attese.allegati.is_empty())
        .unwrap();
    let al = su_richiesta::parte(&cfg, &mut db, id_di(m), &m.attese.allegati[0].parte).unwrap();
    assert!(al.percorso.exists());
    assert_eq!(server.comandi().len(), prima);
    assert!(su_richiesta::parte(&cfg, &mut db, id_di(m), "99").is_err());
    assert!(su_richiesta::parte(&cfg, &mut db, id_di(m), "1]<0.1>").is_err());

    // Dopo il download la vista dà la dimensione vera.
    let json = vista::json(
        &db,
        &OpzioniVista {
            solo: Some(vec![id_di(m)]),
            ..opzioni_vista()
        },
    )
    .unwrap();
    assert!(json.contains(&format!(r#""size":{}"#, m.attese.allegati[0].byte)));
    guardia_pulita(&server);
}

/// Un messaggio scritto a mano: HTML con un'immagine incorporata (cid:), parti
/// annidate su tre livelli, nome del file RFC 2231 spezzato, un allegato grande
/// che si scarica a pezzi.
fn messaggio_incorporato() -> Vec<u8> {
    let png = [0x89u8, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13];
    let b64 = |d: &[u8]| {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
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
        }
        s.as_bytes()
            .chunks(76)
            .map(|r| String::from_utf8_lossy(r).into_owned())
            .collect::<Vec<_>>()
            .join("\r\n")
    };
    let grande: Vec<u8> = (0..5_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
    format!(
        "From: Studio Esempio <studio@esempio.invalid>\r\n\
To: Alex Moro <alex.moro@esempio.it>\r\n\
Subject: Logo e preventivo\r\n\
Date: Tue, 27 Oct 2026 10:00:00 +0100\r\n\
Message-ID: <incorporato@esempio.invalid>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"esterno\"\r\n\
\r\n\
--esterno\r\n\
Content-Type: multipart/alternative; boundary=\"alt\"\r\n\
\r\n\
--alt\r\n\
Content-Type: text/plain; charset=UTF-8\r\n\
\r\n\
Ecco il logo.\r\n\
--alt\r\n\
Content-Type: multipart/related; boundary=\"rel\"\r\n\
\r\n\
--rel\r\n\
Content-Type: text/html; charset=UTF-8\r\n\
\r\n\
<p>Ecco il logo: <img src=\"cid:logo.png@01DB\"></p>\r\n\
--rel\r\n\
Content-Type: image/png; name=\"logo.png\"\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-ID: <logo.png@01DB>\r\n\
Content-Disposition: inline; filename=\"logo.png\"\r\n\
\r\n\
{}\r\n\
--rel--\r\n\
--alt--\r\n\
--esterno\r\n\
Content-Type: application/octet-stream\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-Disposition: attachment;\r\n filename*0*=UTF-8''Preventivo%20definitivo%20;\r\n filename*1*=n%C2%B0%203.bin\r\n\
\r\n\
{}\r\n\
--esterno--\r\n",
        b64(&png),
        b64(&grande)
    )
    .into_bytes()
}

#[test]
fn immagini_incorporate_e_parti_annidate() {
    let mut m = Messaggio::nuovo(messaggio_incorporato());
    m.etichette = vec!["\\Inbox".into()];
    let server = avvia(Casella::con(vec![m]), OpzioniServer::default());
    let cartella = Cartella::nuova("incorporate");
    let mut db = Db::apri(&cartella.db()).unwrap();
    sincronizza_tutto(&server, &mut db, &tutto());
    let conn = rusqlite::Connection::open(cartella.db()).unwrap();
    let parti: Vec<(String, String, String, Option<String>)> = conn
        .prepare("SELECT parte, tipo, ruolo, nome_file FROM parti ORDER BY parte")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        parti,
        vec![
            ("1.1".into(), "text/plain".into(), "testo".into(), None),
            ("1.2.1".into(), "text/html".into(), "html".into(), None),
            (
                "1.2.2".into(),
                "image/png".into(),
                "incorporato".into(),
                Some("logo.png".into())
            ),
            (
                "2".into(),
                "application/octet-stream".into(),
                "allegato".into(),
                Some("Preventivo definitivo n° 3.bin".into())
            ),
        ]
    );
    let json = vista::json(&db, &opzioni_vista()).unwrap();
    let id: i64 = conn
        .query_row("SELECT id FROM messaggi", [], |r| r.get(0))
        .unwrap();
    let url = format!("{PREFISSO}{id}/1.2.2");
    assert!(
        json.contains(&format!(r#"<img src=\"{url}\">"#)),
        "cid non riscritto: {json}"
    );
    assert!(json.contains(r#""inline":true,"contentId":"logo.png@01DB""#));

    let cfg = config(&server, finto::PASSWORD_PREDEFINITA);
    let logo = su_richiesta::parte(&cfg, &mut db, id, "1.2.2").unwrap();
    assert!(
        std::fs::read(&logo.percorso)
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    // L'allegato grande arriva in due pezzi da 4 MiB (base64 di 5 MB).
    server.con(|s| s.comandi.clear());
    let grande = su_richiesta::parte(&cfg, &mut db, id, "2").unwrap();
    assert_eq!(grande.byte, 5_000_000);
    let atteso: Vec<u8> = (0..5_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
    assert_eq!(std::fs::read(&grande.percorso).unwrap(), atteso);
    let pezzi = server
        .comandi()
        .iter()
        .filter(|c| c.contains("BODY.PEEK[2]<"))
        .count();
    assert_eq!(pezzi, 2);
    guardia_pulita(&server);
}

/// Aspetta una notizia che soddisfa la condizione, raccogliendo le altre.
fn aspetta(
    rx: &Receiver<Notizia>,
    viste: &mut Vec<Notizia>,
    entro: Duration,
    cosa: impl Fn(&Notizia) -> bool,
) -> Notizia {
    let fine = Instant::now() + entro;
    loop {
        let resta = fine.saturating_duration_since(Instant::now());
        match rx.recv_timeout(resta) {
            Ok(n) => {
                viste.push(n.clone());
                if cosa(&n) {
                    return n;
                }
            }
            Err(_) => panic!("notizia attesa non arrivata; viste: {viste:?}"),
        }
    }
}

#[test]
fn servizio_con_arrivi_cadute_e_password_sbagliata() {
    let Some(dir) = sintetici() else { return };
    let (mut casella, _) = Casella::carica(dir).unwrap();
    casella.tieni_da_parte(3);
    let iniziali = casella.messaggi.len();
    let server = avvia(
        casella,
        OpzioniServer {
            arrivi: Some(Duration::from_secs(2)),
            ..OpzioniServer::default()
        },
    );
    let cartella = Cartella::nuova("servizio");
    let (tx, rx) = mpsc::channel();
    let opz = OpzioniServizio {
        sincronizza: Opzioni {
            corpi: 50,
            attesa_iniziale: Duration::from_millis(10),
            ..tutto()
        },
        ascolto: OpzioniAscolto {
            controllo: Duration::from_secs(2),
            timeout_controllo: Duration::from_secs(2),
            attesa_minima: Duration::from_millis(200),
            attesa_massima: Duration::from_secs(2),
            ..OpzioniAscolto::default()
        },
        attesa_minima: Some(Duration::from_millis(200)),
        attesa_massima: Some(Duration::from_secs(2)),
    };
    let servizio = Servizio::avvia(
        config(&server, finto::PASSWORD_PREDEFINITA),
        cartella.db(),
        opz.clone(),
        tx,
    );
    let mut viste = Vec::new();
    let d = Duration::from_secs(20);
    aspetta(
        &rx,
        &mut viste,
        d,
        |n| matches!(n, Notizia::Fine { nuovi, .. } if *nuovi >= iniziali),
    );
    aspetta(&rx, &mut viste, d, |n| *n == Notizia::InLinea);
    // Posta nuova notificata in IDLE.
    aspetta(
        &rx,
        &mut viste,
        d,
        |n| matches!(n, Notizia::Fine { nuovi, .. } if *nuovi >= 1),
    );
    // Cadono tutte le connessioni: offline, poi di nuovo in linea e un giro.
    server.fai_cadere(Duration::from_secs(1));
    aspetta(&rx, &mut viste, d, |n| matches!(n, Notizia::Offline { .. }));
    aspetta(&rx, &mut viste, d, |n| *n == Notizia::InLinea);
    aspetta(&rx, &mut viste, d, |n| matches!(n, Notizia::Fine { .. }));
    servizio.ferma();
    let db = Db::apri(&cartella.db()).unwrap();
    let nel_db = db.conta("SELECT count(*) FROM messaggi").unwrap() as usize;
    let sul_server = server.con(|s| s.casella.messaggi.len());
    assert!(
        nel_db > iniziali && nel_db <= sul_server,
        "{nel_db} nel database, {sul_server} sul server"
    );
    eprintln!(
        "servizio: {} notizie, {} messaggi arrivati durante la prova",
        viste.len(),
        nel_db - iniziali
    );

    // Password sbagliata: una notizia sola e niente tentativi a ripetizione.
    server.con(|s| s.comandi.clear());
    let (tx, rx) = mpsc::channel();
    let servizio = Servizio::avvia(config(&server, "sbagliata"), cartella.db(), opz, tx);
    let mut viste = Vec::new();
    aspetta(&rx, &mut viste, d, |n| {
        matches!(n, Notizia::AccessoRifiutato { .. })
    });
    std::thread::sleep(Duration::from_secs(2));
    servizio.ferma();
    let login = server
        .comandi()
        .iter()
        .filter(|c| c.contains(" LOGIN "))
        .count();
    assert_eq!(login, 1, "un tentativo di accesso solo");
    assert!(
        !rx.try_iter()
            .any(|n| matches!(n, Notizia::AccessoRifiutato { .. }))
    );
    guardia_pulita(&server);
}

/// La vista su qualche migliaio di messaggi (la casella moltiplicata per 8).
/// `cargo test --release --test sintetici -- --nocapture migliaia` per i tempi veri.
#[test]
fn vista_su_migliaia_di_messaggi() {
    let Some(dir) = sintetici() else { return };
    let (mut casella, _) = Casella::carica(dir).unwrap();
    casella.moltiplica(8);
    let n = casella.messaggi.len();
    let server = avvia(
        casella,
        OpzioniServer {
            registra: false,
            ..OpzioniServer::default()
        },
    );
    let cartella = Cartella::nuova("migliaia");
    let mut db = Db::apri(&cartella.db()).unwrap();
    let r = sincronizza_tutto(&server, &mut db, &tutto());
    assert_eq!(r.nuovi, n);
    let t = Instant::now();
    let json = vista::json(&db, &opzioni_vista()).unwrap();
    let tutto_t = t.elapsed();
    let t = Instant::now();
    let breve = vista::json(
        &db,
        &OpzioniVista {
            limite: Some(500),
            ..opzioni_vista()
        },
    )
    .unwrap();
    let breve_t = t.elapsed();
    eprintln!(
        "migliaia: sincronizzati {n} messaggi in {:?}; vista intera {} KB in {:?}, ultimi 500 {} KB in {:?}",
        r.durata,
        json.len() / 1024,
        tutto_t,
        breve.len() / 1024,
        breve_t
    );
    assert!(json.contains(&format!(r#""count":{n}}}"#)));
}

// ---------------------------------------------------------------------------
// Misure delle immagini, PDF, messaggio intero, allegati in background, chiusura
// ---------------------------------------------------------------------------

fn id_di(db: &Db, msgid: u64) -> i64 {
    let conn = rusqlite::Connection::open(&db.percorso).unwrap();
    conn.query_row(
        "SELECT id FROM messaggi WHERE gm_msgid = ?1",
        [msgid as i64],
        |r| r.get(0),
    )
    .unwrap()
}

/// Le pagine dei PDF secondo `synthetic.js`: (id del messaggio, nome del file) → pagine.
fn pagine_dei_sintetici() -> Option<HashMap<(String, String), u32>> {
    let script = r#"
const fs = require('fs'), vm = require('vm');
const c = { window: {} }; vm.runInNewContext(fs.readFileSync(process.argv[1], 'utf8'), c);
for (const m of c.window.AMB_DATA.messages) for (const a of m.attachments || [])
  if (a.pages) console.log(m.id + '\t' + a.filename + '\t' + a.pages);
"#;
    let sintetici = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mockups/data/synthetic.js");
    let o = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(&sintetici)
        .output()
        .ok()?;
    assert!(o.status.success());
    Some(
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|r| {
                let mut p = r.split('\t');
                Some((
                    (p.next()?.to_string(), p.next()?.to_string()),
                    p.next()?.parse().ok()?,
                ))
            })
            .collect(),
    )
}

/// Il file dell'archivio senza i file temporanei: nessuno deve restare.
fn temporanei(db: &Db) -> Vec<String> {
    std::fs::read_dir(su_richiesta::cartella_allegati(db))
        .map(|d| {
            d.flatten()
                .map(|v| v.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn pdf_e_immagini_pagine_miniature_e_misure() {
    let Some(dir) = sintetici() else { return };
    let Some(pagine) = pagine_dei_sintetici() else {
        return;
    };
    if Command::new("pdfinfo").arg("-v").output().is_err() {
        eprintln!("poppler-utils non c'è: salto le prove dei PDF");
        return;
    }
    let (casella, _) = Casella::carica(dir).unwrap();
    let attesi = casella.messaggi.clone();
    let server = avvia(casella, OpzioniServer::default());
    let cartella = Cartella::nuova("pdf");
    let mut db = Db::apri(&cartella.db()).unwrap();
    sincronizza_tutto(
        &server,
        &mut db,
        &Opzioni {
            corpi: 0,
            ..tutto()
        },
    );
    let mut richieste = su_richiesta::Richieste::new(config(&server, finto::PASSWORD_PREDEFINITA));
    let t = Instant::now();
    let (mut pdf, mut immagini, mut miniature) = (0, 0, HashSet::new());
    let mut ids = Vec::new();
    for m in &attesi {
        let id = id_di(&db, m.msgid);
        ids.push(id);
        for a in &m.attese.allegati {
            let al = richieste.parte(&mut db, id, &a.parte).unwrap();
            let mis = db.misure(id, &a.parte).unwrap().unwrap();
            if a.tipo == "application/pdf" {
                pdf += 1;
                let attese = pagine
                    .get(&(m.attese.id.clone(), a.nome.clone()))
                    .unwrap_or_else(|| panic!("pagine di {} {}", m.attese.id, a.nome));
                assert_eq!(
                    mis.pagine,
                    Some(*attese),
                    "pagine di {} {}",
                    m.attese.id,
                    a.nome
                );
                assert_eq!(mis.miniatura, Some(true));
                let p = su_richiesta::miniatura(&db, id, &a.parte).unwrap().unwrap();
                assert_eq!(
                    p,
                    al.percorso
                        .with_file_name(su_richiesta::nome_miniatura(&al.impronta))
                );
                let jpeg = std::fs::read(&p).unwrap();
                let (l, h) = motore::misure::misure_immagine(&jpeg).expect("miniatura JPEG");
                assert_eq!(l.max(h), 360, "lato lungo della miniatura");
                miniature.insert(p);
            } else if a.tipo.starts_with("image/") {
                immagini += 1;
                let (l, h) = (mis.larghezza.unwrap(), mis.altezza.unwrap());
                assert!(l > 0 && h > 0, "misure di {} {}", m.attese.id, a.nome);
                assert_eq!(su_richiesta::miniatura(&db, id, &a.parte).unwrap(), None);
            } else {
                assert_eq!(mis, MisureParte::default(), "{} {}", m.attese.id, a.nome);
            }
        }
    }
    eprintln!(
        "{pdf} PDF (miniature {}) e {immagini} immagini scaricati e misurati in {:?}",
        miniature.len(),
        t.elapsed()
    );
    assert!(pdf > 50 && immagini > 10);
    assert!(temporanei(&db).is_empty());
    // Le miniature si fanno una volta per contenuto.
    let nell_archivio = std::fs::read_dir(su_richiesta::cartella_allegati(&db))
        .unwrap()
        .filter(|v| {
            v.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".miniatura.jpg")
        })
        .count();
    assert_eq!(nell_archivio, miniature.len());

    // La vista: pagine e miniatura per i PDF, misure per le immagini.
    let json = vista::json(&db, &opzioni_vista()).unwrap();
    assert_eq!(json.matches(r#""pages":"#).count(), pdf);
    assert_eq!(
        json.matches(r#""thumb":"a10s://localhost/miniatura/"#)
            .count(),
        pdf
    );
    assert!(json.matches(r#""width":"#).count() >= immagini);
    let m = attesi
        .iter()
        .find(|m| {
            m.attese
                .allegati
                .iter()
                .any(|a| a.tipo == "application/pdf")
        })
        .unwrap();
    let a = m
        .attese
        .allegati
        .iter()
        .find(|a| a.tipo == "application/pdf")
        .unwrap();
    let id = id_di(&db, m.msgid);
    let atteso = format!(
        r#""url":"{PREFISSO}{id}/{p}","pages":{},"thumb":"a10s://localhost/miniatura/{id}/{p}"}}"#,
        pagine[&(m.attese.id.clone(), a.nome.clone())],
        p = a.parte
    );
    assert!(json.contains(&atteso), "manca {atteso}");

    // Un database scaricato prima di questa versione: le misure si completano dopo.
    let conn = rusqlite::Connection::open(cartella.db()).unwrap();
    conn.execute_batch(
        "UPDATE allegati SET larghezza = NULL, altezza = NULL, pagine = NULL, miniatura = NULL",
    )
    .unwrap();
    for p in &miniature {
        std::fs::remove_file(p).unwrap();
    }
    let guardate = su_richiesta::completa_archivio(&db).unwrap();
    assert!(guardate >= pdf + immagini);
    let json2 = vista::json(&db, &opzioni_vista()).unwrap();
    assert_eq!(json2.matches(r#""pages":"#).count(), pdf);
    assert_eq!(json2.matches(r#""thumb":"#).count(), pdf);
    assert!(miniature.iter().all(|p| p.exists()));
    assert_eq!(
        su_richiesta::completa_archivio(&db).unwrap(),
        0,
        "niente da rifare"
    );
    drop(richieste);
    guardia_pulita(&server);
}

#[test]
fn messaggio_intero_per_salva_e_pdf_veri() {
    let allegati = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mockups/data/attachments");
    let pdf = std::fs::read(allegati.join("044.pdf")).unwrap();
    let jpeg = std::fs::read(allegati.join("002.jpg")).unwrap();
    let rotto = b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\nrotto\n".to_vec();
    let grande: Vec<u8> = (0..5_000_000u32).map(|i| (i * 13 % 251) as u8).collect();
    let mut m1 = Messaggio::nuovo(finto::messaggio_con_allegati(
        "Re: Contratto: versione firmata?",
        "<contratto@esempio.invalid>",
        &[
            ("Contratto.pdf", "application/pdf", &pdf),
            ("Pianta.jpg", "image/jpeg", &jpeg),
            ("Rotto.pdf", "application/pdf", &rotto),
            ("Archivio.bin", "application/octet-stream", &grande),
        ],
    ));
    m1.etichette = vec!["\\Inbox".into()];
    let piccolo = b"From: Ilaria <ilaria@esempio.invalid>\r\n\
To: Alex Moro <alex.moro@esempio.it>\r\n\
Subject: Preventivo impianto\r\n\
Date: Tue, 27 Oct 2026 11:00:00 +0100\r\n\
Message-ID: <case-inoltro-0@esempio.it>\r\n\
\r\n\
Ecco il preventivo.\r\n"
        .to_vec();
    let mut m2 = Messaggio::nuovo(piccolo.clone());
    m2.etichette = vec!["\\Inbox".into()];
    let grezzo1 = m1.grezzo.clone();
    let server = avvia(Casella::con(vec![m1, m2]), OpzioniServer::default());
    let cartella = Cartella::nuova("intero");
    let mut db = Db::apri(&cartella.db()).unwrap();
    sincronizza_tutto(&server, &mut db, &tutto());
    let conn = rusqlite::Connection::open(cartella.db()).unwrap();
    let id_oggetto = |o: &str| -> i64 {
        conn.query_row("SELECT id FROM messaggi WHERE oggetto = ?1", [o], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (id1, id2) = (
        id_oggetto("Re: Contratto: versione firmata?"),
        id_oggetto("Preventivo impianto"),
    );
    let cfg = config(&server, finto::PASSWORD_PREDEFINITA);
    let mut richieste = su_richiesta::Richieste::new(cfg);

    // Il messaggio piccolo arriva in una richiesta sola, uguale a com'è sul server.
    server.con(|s| s.comandi.clear());
    let intero = richieste.messaggio_intero(&mut db, id2).unwrap();
    assert_eq!(intero, piccolo);
    assert!(
        server
            .comandi()
            .iter()
            .any(|c| c.ends_with(" (UID BODY.PEEK[])"))
    );
    assert_eq!(
        su_richiesta::nome_eml_messaggio(&db, id2).unwrap(),
        "4f6f3f-Preventivo impianto.eml"
    );
    // Quello grande (più di 4 MB) a pezzi.
    server.con(|s| s.comandi.clear());
    let intero = richieste.messaggio_intero(&mut db, id1).unwrap();
    assert_eq!(intero.len(), grezzo1.len());
    assert!(
        intero == *grezzo1,
        "messaggio intero diverso dall'originale"
    );
    let pezzi = server
        .comandi()
        .iter()
        .filter(|c| c.contains("BODY.PEEK[]<"))
        .count();
    assert_eq!(pezzi, 2);
    let nome = su_richiesta::nome_eml_messaggio(&db, id1).unwrap();
    assert_eq!(
        nome,
        format!(
            "{}-Re Contratto versione firmata.eml",
            &motore::impronta::md5_esadecimale(b"contratto@esempio.invalid")[..6]
        )
    );
    let flag: String = conn
        .query_row(
            "SELECT altre_flag || letto FROM messaggi WHERE id = ?1",
            [id1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(flag, "0", "il messaggio resta da leggere");

    // Le parti: PDF di 5 pagine con la miniatura, JPEG 255×360, PDF rotto senza niente.
    if Command::new("pdfinfo").arg("-v").output().is_err() {
        eprintln!("poppler-utils non c'è: salto le prove dei PDF");
        return;
    }
    let parti: Vec<(String, String)> = conn
        .prepare(
            "SELECT parte, nome_file FROM parti WHERE messaggio_id = ?1 AND ruolo = 'allegato'",
        )
        .unwrap()
        .query_map([id1], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let parte = |n: &str| parti.iter().find(|(_, x)| x == n).unwrap().0.clone();
    let p = richieste
        .parte(&mut db, id1, &parte("Contratto.pdf"))
        .unwrap();
    assert_eq!(std::fs::read(&p.percorso).unwrap(), pdf);
    let mis = db.misure(id1, &parte("Contratto.pdf")).unwrap().unwrap();
    assert_eq!((mis.pagine, mis.miniatura), (Some(5), Some(true)));
    richieste.parte(&mut db, id1, &parte("Pianta.jpg")).unwrap();
    let mis = db.misure(id1, &parte("Pianta.jpg")).unwrap().unwrap();
    assert_eq!((mis.larghezza, mis.altezza), (Some(255), Some(360)));
    richieste.parte(&mut db, id1, &parte("Rotto.pdf")).unwrap();
    let mis = db.misure(id1, &parte("Rotto.pdf")).unwrap().unwrap();
    assert_eq!((mis.pagine, mis.miniatura), (Some(0), Some(false)));
    assert_eq!(
        su_richiesta::miniatura(&db, id1, &parte("Rotto.pdf")).unwrap(),
        None
    );
    let json = vista::json(
        &db,
        &OpzioniVista {
            solo: Some(vec![id1]),
            ..opzioni_vista()
        },
    )
    .unwrap();
    assert!(json.contains(&format!(
        r#""url":"{PREFISSO}{id1}/{c}","pages":5,"thumb":"a10s://localhost/miniatura/{id1}/{c}"}}"#,
        c = parte("Contratto.pdf")
    )));
    assert!(json.contains(&format!(
        r#""url":"{PREFISSO}{id1}/{c}","width":255,"height":360}}"#,
        c = parte("Pianta.jpg")
    )));
    assert!(json.contains(&format!(
        r#""url":"{PREFISSO}{id1}/{c}"}}"#,
        c = parte("Rotto.pdf")
    )));
    // Quota: messaggi interi e parti, byte come arrivano dal server.
    let q = scaricamento::quota(&db, scaricamento::QUOTA_CONSIGLIATA).unwrap();
    assert!(q.scaricati > grezzo1.len() as u64 + piccolo.len() as u64 + pdf.len() as u64);
    assert_eq!(q.restano, q.limite - q.scaricati);
    assert!(temporanei(&db).is_empty());
    drop(richieste);
    guardia_pulita(&server);
}

#[test]
fn coda_quota_pausa_e_totali() {
    let Some(dir) = sintetici() else { return };
    let (casella, _) = Casella::carica(dir).unwrap();
    let attesi = casella.messaggi.clone();
    let tutti_gli_allegati: usize = attesi.iter().map(|m| m.attese.allegati.len()).sum();
    let server = avvia(casella, OpzioniServer::default());
    let cartella = Cartella::nuova("coda");
    let mut db = Db::apri(&cartella.db()).unwrap();
    sincronizza_tutto(
        &server,
        &mut db,
        &Opzioni {
            corpi: 0,
            ..tutto()
        },
    );
    let conn = rusqlite::Connection::open(cartella.db()).unwrap();
    let data_di = |id: i64| -> i64 {
        conn.query_row(
            "SELECT COALESCE(data_interna, data) FROM messaggi WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    };

    // Totali prima: tutto da scaricare.
    let t = scaricamento::totali(&db).unwrap();
    assert_eq!(t.tutti.numero() as usize, tutti_gli_allegati);
    assert_eq!(t.tutti.scaricati, 0);
    assert_eq!(
        t.per_tipo.iter().map(|(_, c)| c.numero()).sum::<u64>(),
        t.tutti.numero()
    );
    assert_eq!(
        t.per_anno.iter().map(|(_, c)| c.numero()).sum::<u64>(),
        t.tutti.numero()
    );
    let pdf = attesi
        .iter()
        .flat_map(|m| &m.attese.allegati)
        .filter(|a| a.tipo == "application/pdf")
        .count() as u64;
    assert_eq!(
        t.per_tipo[0],
        (scaricamento::Categoria::Pdf, t.per_tipo[0].1)
    );
    assert_eq!(t.per_tipo[0].1.numero(), pdf);

    // La coda intera è in ordine dal più recente.
    let intera = scaricamento::coda(&db, 10_000, &[]).unwrap();
    assert_eq!(intera.len(), tutti_gli_allegati);
    assert!(
        intera
            .windows(2)
            .all(|w| data_di(w[0].messaggio) >= data_di(w[1].messaggio))
    );
    let cinque = scaricamento::coda(&db, 5, &[]).unwrap();
    assert_eq!(cinque[..], intera[..5]);
    // In testa quelle sullo schermo, nell'ordine dato; doppioni e parti inesistenti saltati.
    let vecchia = intera.last().unwrap().clone();
    let penultima = intera[intera.len() - 2].clone();
    let in_testa = vec![
        (vecchia.messaggio, vecchia.parte.clone()),
        (vecchia.messaggio, vecchia.parte.clone()),
        (999_999, "2".to_string()),
        (penultima.messaggio, penultima.parte.clone()),
    ];
    let c = scaricamento::coda(&db, 4, &in_testa).unwrap();
    assert_eq!(c[0], vecchia);
    assert_eq!(c[1], penultima);
    assert_eq!(c[2..], intera[..2]);
    assert_eq!(
        scaricamento::coda(&db, 1, &in_testa).unwrap(),
        vec![vecchia.clone()]
    );

    // Le prime tre scaricate escono dalla coda e passano nei totali.
    let mut richieste = su_richiesta::Richieste::new(config(&server, finto::PASSWORD_PREDEFINITA));
    let mut byte = 0;
    for d in &intera[..3] {
        byte += richieste
            .parte(&mut db, d.messaggio, &d.parte)
            .unwrap()
            .byte;
    }
    let dopo = scaricamento::coda(&db, 10_000, &[]).unwrap();
    assert_eq!(dopo[..], intera[3..]);
    let t = scaricamento::totali(&db).unwrap();
    assert_eq!((t.tutti.scaricati, t.tutti.byte_scaricati), (3, byte));
    assert_eq!(t.tutti.da_scaricare as usize, tutti_gli_allegati - 3);
    assert!(t.archivio > 0 && t.archivio <= byte);
    let q = scaricamento::quota(&db, 1).unwrap();
    assert!(q.scaricati >= byte, "base64 pesa più dei byte decodificati");
    assert_eq!(q.restano, 0);
    assert_eq!(scaricamento::pausa(&db, 1).unwrap(), Some(Pausa::Quota));
    // Con la quota libera ferma solo una connessione a consumo o il disco pieno.
    assert_ne!(
        scaricamento::pausa(&db, u64::MAX).unwrap(),
        Some(Pausa::Quota)
    );

    // Una parte che non arriva: fuori dalla coda fino al prossimo tentativo, poi di nuovo dentro.
    let d = dopo[0].clone();
    server.fai_cadere(Duration::from_secs(3));
    assert!(richieste.parte(&mut db, d.messaggio, &d.parte).is_err());
    let (tentativi, prossimo): (i64, i64) = conn
        .query_row(
            "SELECT tentativi, prossimo FROM parti_fallite WHERE messaggio_id = ?1 AND parte = ?2",
            rusqlite::params![d.messaggio, d.parte],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let ora = motore::tempo::adesso();
    assert_eq!(tentativi, 1);
    assert!((ora + 55..=ora + 61).contains(&prossimo));
    assert!(
        !scaricamento::coda(&db, 10_000, &[(d.messaggio, d.parte.clone())])
            .unwrap()
            .contains(&d)
    );
    assert_eq!(scaricamento::totali(&db).unwrap().in_attesa, 1);
    assert!(temporanei(&db).is_empty());
    // Passato il minuto (simulato) torna in coda; un secondo fallimento aspetta il doppio.
    conn.execute("UPDATE parti_fallite SET prossimo = ?1", [ora - 1])
        .unwrap();
    assert_eq!(scaricamento::coda(&db, 1, &[]).unwrap(), vec![d.clone()]);
    assert!(richieste.parte(&mut db, d.messaggio, &d.parte).is_err());
    let (tentativi, prossimo): (i64, i64) = conn
        .query_row("SELECT tentativi, prossimo FROM parti_fallite", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(tentativi, 2);
    assert!(prossimo >= ora + 115);
    // Quando arriva, non è più fra le fallite.
    std::thread::sleep(Duration::from_secs(3));
    richieste.parte(&mut db, d.messaggio, &d.parte).unwrap();
    assert_eq!(scaricamento::totali(&db).unwrap().in_attesa, 0);
    drop(richieste);

    // Il sottocomando dei totali: solo numeri, nessun nome, oggetto o indirizzo.
    let out = Command::new(env!("CARGO_BIN_EXE_motore"))
        .args(["allegati", "--db", cartella.db().to_str().unwrap()])
        .output()
        .unwrap();
    let testo =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{testo}");
    eprintln!("motore allegati:\n{testo}");
    assert!(testo.starts_with(&format!(
        "Allegati      {} ·",
        motore::tempo::numero(tutti_gli_allegati as u64)
    )));
    assert!(testo.contains("\n  PDF         "));
    assert!(testo.contains("\nPer anno\n"));
    assert!(!testo.contains('@'), "{testo}");
    for m in &attesi {
        for a in &m.attese.allegati {
            assert!(
                !testo.contains(&a.nome),
                "nome di un file nell'output: {testo}"
            );
        }
    }
    for o in ["Preventivo", "Fattura", "Contratto", "esempio"] {
        assert!(!testo.contains(o), "«{o}» nell'output: {testo}");
    }
    guardia_pulita(&server);
}

#[test]
fn download_interrotto_non_lascia_file_a_meta() {
    let grande: Vec<u8> = (0..6_000_000u32).map(|i| (i * 11 % 253) as u8).collect();
    let mut m = Messaggio::nuovo(finto::messaggio_con_allegati(
        "Archivio grande",
        "<grande@esempio.invalid>",
        &[("Archivio.bin", "application/octet-stream", &grande)],
    ));
    m.etichette = vec!["\\Inbox".into()];
    let server = avvia(
        Casella::con(vec![m]),
        OpzioniServer {
            ritardo: Duration::from_millis(300),
            ..OpzioniServer::default()
        },
    );
    let cartella = Cartella::nuova("interrotto");
    let mut db = Db::apri(&cartella.db()).unwrap();
    sincronizza_tutto(&server, &mut db, &tutto());
    let id: i64 = db.conta("SELECT id FROM messaggi").unwrap();
    let cfg = config(&server, finto::PASSWORD_PREDEFINITA);
    let percorso = cartella.db();
    let richieste = su_richiesta::Richieste::new(cfg.clone());
    let interruttore = richieste.interruttore();
    server.con(|s| s.comandi.clear());
    let filo = std::thread::spawn(move || {
        let mut richieste = richieste;
        let mut db = Db::apri(&percorso).unwrap();
        let t = Instant::now();
        let esito = richieste.parte(&mut db, id, "2");
        (esito.map(|a| a.byte), t.elapsed())
    });
    // Aspetta che il secondo pezzo sia stato chiesto (il primo è già arrivato), poi chiude.
    let fine = Instant::now() + Duration::from_secs(20);
    while !server
        .comandi()
        .iter()
        .any(|c| c.contains("BODY.PEEK[2]<4194304."))
    {
        assert!(
            Instant::now() < fine,
            "il secondo pezzo non è mai stato chiesto"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let t = Instant::now();
    interruttore.chiudi();
    let (esito, _) = filo.join().unwrap();
    let chiusura = t.elapsed();
    eprintln!("download interrotto: il thread è tornato {chiusura:?} dopo la chiusura");
    assert!(esito.is_err());
    assert!(chiusura < Duration::from_millis(500));
    // Niente nell'archivio, niente file temporanei, la parte è fra quelle da riprovare.
    let archivio = su_richiesta::cartella_allegati(&db);
    let file: Vec<_> = std::fs::read_dir(&archivio)
        .map(|d| d.flatten().map(|v| v.file_name()).collect())
        .unwrap_or_default();
    assert!(file.is_empty(), "{file:?}");
    assert_eq!(db.allegato_scaricato(id, "2").unwrap(), None);
    assert_eq!(db.conta("SELECT count(*) FROM parti_fallite").unwrap(), 1);
    assert!(scaricamento::coda(&db, 10, &[]).unwrap().is_empty());

    // Un clic lo scarica lo stesso, intero, e lo toglie dalle fallite.
    let a = su_richiesta::parte(&cfg, &mut db, id, "2").unwrap();
    assert_eq!(std::fs::read(&a.percorso).unwrap(), grande);
    assert_eq!(db.conta("SELECT count(*) FROM parti_fallite").unwrap(), 0);
    guardia_pulita(&server);
}

#[test]
fn servizio_contatto_aggiorna_in_idle_e_chiude_subito() {
    let Some(dir) = sintetici() else { return };
    let (casella, _) = Casella::carica(dir).unwrap();
    let server = avvia(casella.clone(), OpzioniServer::default());
    let cartella = Cartella::nuova("chiusura");
    let (tx, rx) = mpsc::channel();
    let opz = OpzioniServizio {
        sincronizza: Opzioni {
            corpi: 20,
            attesa_iniziale: Duration::from_millis(10),
            ..tutto()
        },
        ascolto: OpzioniAscolto {
            controllo: Duration::from_millis(700),
            timeout_controllo: Duration::from_secs(2),
            ..OpzioniAscolto::default()
        },
        attesa_minima: Some(Duration::from_millis(200)),
        attesa_massima: Some(Duration::from_secs(2)),
    };
    let prima = motore::tempo::adesso();
    let servizio = Servizio::avvia(
        config(&server, finto::PASSWORD_PREDEFINITA),
        cartella.db(),
        opz.clone(),
        tx,
    );
    let mut viste = Vec::new();
    let d = Duration::from_secs(20);
    aspetta(&rx, &mut viste, d, |n| matches!(n, Notizia::Fine { .. }));
    let contatto = aspetta(&rx, &mut viste, d, |n| {
        matches!(n, Notizia::Contatto { .. })
    });
    let Notizia::Contatto { alle } = contatto else {
        unreachable!()
    };
    assert!(alle >= prima && alle <= motore::tempo::adesso());
    aspetta(&rx, &mut viste, d, |n| *n == Notizia::InLinea);
    // Tre controlli periodici dell'IDLE (ogni 0,7 s): nessun altro Contatto entro il minuto.
    let fine = Instant::now() + Duration::from_millis(2600);
    while Instant::now() < fine {
        if let Ok(n) = rx.recv_timeout(Duration::from_millis(50)) {
            viste.push(n);
        }
    }
    let controlli = server
        .comandi()
        .iter()
        .filter(|c| c.ends_with(" NOOP"))
        .count();
    assert!(controlli >= 2, "controlli dell'IDLE: {controlli}");
    assert_eq!(
        viste
            .iter()
            .filter(|n| matches!(n, Notizia::Contatto { .. }))
            .count(),
        1,
        "al massimo un Contatto al minuto: {viste:?}"
    );
    // Aggiornare a mano mentre l'ascolto è in IDLE: un giro subito, sulla connessione di lavoro.
    let connessioni = server.con(|s| s.connessioni);
    let t = Instant::now();
    servizio.sincronizza_ora();
    aspetta(&rx, &mut viste, d, |n| *n == Notizia::Inizio);
    aspetta(&rx, &mut viste, d, |n| matches!(n, Notizia::Fine { .. }));
    eprintln!("aggiornamento a mano durante l'IDLE: {:?}", t.elapsed());
    assert!(t.elapsed() < Duration::from_secs(2));
    assert_eq!(
        server.con(|s| s.connessioni),
        connessioni,
        "nessuna connessione nuova"
    );

    // Chiusura: in meno di mezzo secondo, con DONE e LOGOUT mandati.
    server.con(|s| s.comandi.clear());
    let t = Instant::now();
    servizio.ferma();
    let ferma = t.elapsed();
    eprintln!("ferma con l'IDLE aperto: {ferma:?}");
    assert!(ferma < Duration::from_millis(500), "ferma: {ferma:?}");
    // Il server registra i comandi dal suo thread: gli si lascia un attimo.
    let fine = Instant::now() + Duration::from_secs(3);
    while (server.connessioni_aperte() > 0 || server.comandi().len() < 3) && Instant::now() < fine {
        std::thread::sleep(Duration::from_millis(20));
    }
    let comandi = server.comandi();
    // DONE e LOGOUT partono da tutte e due le connessioni, ma il socket si chiude
    // subito dopo: se nel frattempo è arrivata la risposta al DONE, il sistema chiude
    // con un RST e il server può perdere il LOGOUT dell'ascolto. Quello della
    // connessione di lavoro arriva sempre.
    assert!(comandi.iter().any(|c| c == "DONE"), "{comandi:?}");
    assert!(
        comandi.iter().any(|c| c.ends_with(" LOGOUT")),
        "LOGOUT: {comandi:?}"
    );
    assert_eq!(server.connessioni_aperte(), 0);
    assert!(!rx.try_iter().any(|n| matches!(n, Notizia::Offline { .. })));

    // Con un server lento (0,4 s per risposta), fermato a metà della prima sincronizzazione.
    let lento = avvia(
        casella,
        OpzioniServer {
            ritardo: Duration::from_millis(400),
            ..OpzioniServer::default()
        },
    );
    let prima = cartella;
    let cartella = Cartella::nuova("chiusura-lenta");
    let (tx, rx) = mpsc::channel();
    let servizio = Servizio::avvia(
        config(&lento, finto::PASSWORD_PREDEFINITA),
        cartella.db(),
        opz,
        tx,
    );
    let mut viste = Vec::new();
    aspetta(&rx, &mut viste, d, |n| *n == Notizia::Inizio);
    std::thread::sleep(Duration::from_millis(1000));
    let t = Instant::now();
    servizio.ferma();
    let ferma_lento = t.elapsed();
    eprintln!("ferma a metà sincronizzazione, server lento: {ferma_lento:?}");
    assert!(ferma_lento < Duration::from_millis(500));
    // Il server lento chiude le sue connessioni dopo aver risposto (0,4 s a comando).
    let fine = Instant::now() + Duration::from_secs(5);
    while lento.connessioni_aperte() > 0 && Instant::now() < fine {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(lento.connessioni_aperte(), 0);
    assert!(!rx.try_iter().any(|n| matches!(n, Notizia::Offline { .. })));

    // Richieste::chiudi con il server lento (stessa casella, stessi UID del primo
    // database): il LOGOUT parte, la risposta (0,4 s) non si aspetta oltre 0,3 s.
    let mut db = Db::apri(&prima.db()).unwrap();
    let mut richieste = su_richiesta::Richieste::new(config(&lento, finto::PASSWORD_PREDEFINITA));
    let id: i64 = db
        .conta("SELECT messaggio_id FROM parti WHERE ruolo = 'testo' LIMIT 1")
        .unwrap();
    let parte: String = rusqlite::Connection::open(prima.db())
        .unwrap()
        .query_row(
            "SELECT parte FROM parti WHERE messaggio_id = ?1 AND ruolo = 'testo'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    richieste.parte(&mut db, id, &parte).unwrap();
    lento.con(|s| s.comandi.clear());
    let t = Instant::now();
    richieste.chiudi();
    let chiudi = t.elapsed();
    eprintln!("Richieste::chiudi, server lento: {chiudi:?}");
    assert!(chiudi >= Duration::from_millis(250) && chiudi < Duration::from_millis(500));
    let fine = Instant::now() + Duration::from_secs(2);
    while !lento.comandi().iter().any(|c| c.ends_with(" LOGOUT")) && Instant::now() < fine {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(lento.comandi().iter().any(|c| c.ends_with(" LOGOUT")));
    guardia_pulita(&server);
    guardia_pulita(&lento);
}
