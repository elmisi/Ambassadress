//! Il collegamento fra l'app e il motore: accesso di prova, sincronizzazione in
//! background (il `Servizio` del motore), dati per l'interfaccia, allegati.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tauri::{Emitter, Manager};

use motore::db::Db;
use motore::errore::Errore;
use motore::imap::Sessione;
use motore::rete::Contatori;
use motore::servizio::{OpzioniServizio, Servizio};
use motore::sincronizza::{Notizia, Opzioni};
use motore::scaricamento::{self, Pausa, QUOTA_CONSIGLIATA};
use motore::storico::{self, Storico};
use motore::su_richiesta::{self, Allegato, Mantenuta, Richieste};
use motore::vista::{self, Account, OpzioniVista};

use crate::casella::Casella;
use crate::{Stato, manda_rete};

/// Quanti messaggi recenti tiene la sincronizzazione, con il testo, e quanti ne riceve la pagina
/// all'avvio (più gli altri dei loro thread). Il resto lo scarica lo storico (giro 2) e la
/// pagina lo chiede quando serve: una persona aperta, una ricerca.
const FINESTRA: usize = 3000;
/// Lo storico a richiesta: al massimo tanti messaggi per una persona, e per una ricerca.
const STORIA_PERSONA: usize = 1000;
const STORIA_RICERCA: usize = 300;
/// Il prefisso degli allegati: il protocollo `a10s` registrato in `main.rs`.
const PREFISSO_ALLEGATI: &str = "a10s://localhost/allegato/";
/// Ogni quanto, al massimo, l'interfaccia ricarica i dati mentre si scarica.
const RICARICA_OGNI: Duration = Duration::from_secs(3);
/// Sotto questo numero di messaggi da scaricare l'etichetta «Scarico la posta» non compare.
const AVANZAMENTO_DA: usize = 50;
/// Un giro ogni tanto anche senza novità dal server: i «letto» dati altrove (giro 2).
const GIRO_OGNI: Duration = Duration::from_secs(3 * 60);
/// Una caduta della rete si mostra solo se dura: Gmail chiude la connessione di ascolto
/// circa una volta all'ora e il motore si ricollega in pochi secondi (giro 2).
const OFFLINE_DOPO: Duration = Duration::from_secs(30);

/// Il database della casella, dentro la cartella dei dati.
pub fn percorso_db(dati: &Path) -> PathBuf {
    dati.join("posta.sqlite")
}

fn adesso_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Collegamento e accesso, poi si esce. `tappa` riceve «server» e «password» man mano.
pub fn prova_accesso(c: &Casella, tappa: impl Fn(&str)) -> Result<(), String> {
    let cfg = c.config();
    let mut s = Sessione::connetti(&cfg.indirizzo, Duration::from_secs(20), Arc::new(Contatori::default()))
        .map_err(|e| messaggio_errore(&e, c))?;
    tappa("server");
    s.login(cfg.credenziali.utente.esponi(), cfg.credenziali.password.esponi())
        .map_err(|e| messaggio_errore(&e, c))?;
    tappa("password");
    s.logout();
    Ok(())
}

/// Un errore del motore detto all'utente, senza mai la password.
pub fn messaggio_errore(e: &Errore, c: &Casella) -> String {
    match e {
        Errore::Accesso(_) if c.tipo == "gmail" => "Gmail rifiuta la password. Serve una password per app, quella di 16 lettere: la password con cui entri in Gmail qui non funziona.".into(),
        Errore::Accesso(_) => "Il server rifiuta l’indirizzo o la password.".into(),
        Errore::Rete(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => {
            format!("Il server {} non accetta connessioni: controlla il nome.", c.imap)
        }
        Errore::Rete(io) if io.to_string().contains("resolve") || io.to_string().contains("lookup") || io.to_string().contains("not known") => {
            format!("Non trovo il server {}: controlla il nome, o la connessione a internet.", c.imap)
        }
        Errore::Rete(_) => format!("Non riesco a collegarmi a {}: controlla la connessione a internet.", c.imap),
        Errore::Timeout => format!("Il server {} non risponde.", c.imap),
        Errore::Tls(_) => format!("La connessione cifrata con {} non riesce.", c.imap),
        Errore::Chiusa => format!("Il server {} ha chiuso la connessione.", c.imap),
        // giro 1: ogni errore ha la sua frase, invece di «qualcosa non va»
        Errore::Db(e) if e.to_string().contains("locked") || e.to_string().contains("busy") => {
            "Il database è occupato dalla sincronizzazione: riprovo tra poco.".into()
        }
        Errore::Db(_) => "Non riesco a leggere il database della posta.".into(),
        Errore::Server { stato, .. } => format!("Il server {} ha risposto {stato} alla richiesta.", c.imap),
        Errore::Uso(_) => format!("La cartella sul server {} è cambiata: aggiorno e riprovo.", c.imap),
        Errore::Protocollo(_) => format!("Il server {} ha risposto in un modo che non capisco.", c.imap),
        _ => format!("Qualcosa non va con il server {}.", c.imap),
    }
}

/// La sincronizzazione in background: il servizio del motore più un thread che
/// traduce le sue notizie in eventi per l'interfaccia.
pub struct Sincronia {
    servizio: Option<Servizio>,
    traduttore: Option<JoinHandle<()>>,
}

impl Sincronia {
    pub fn avvia(app: tauri::AppHandle, dati: PathBuf, c: Casella) -> Self {
        let (tx, rx) = mpsc::channel();
        let opz = OpzioniServizio {
            sincronizza: Opzioni { limite: Some(FINESTRA), corpi: FINESTRA, ..Opzioni::default() },
            giro_ogni: Some(GIRO_OGNI),
            ..OpzioniServizio::default()
        };
        let servizio = Servizio::avvia(c.config(), percorso_db(&dati), opz, tx);
        let traduttore = std::thread::Builder::new()
            .name("a10s-notizie".into())
            .spawn(move || traduci(app, rx))
            .expect("thread delle notizie");
        Sincronia { servizio: Some(servizio), traduttore: Some(traduttore) }
    }

    /// Sincronizza subito, anche se il motore è in ascolto.
    pub fn aggiorna(&self) {
        if let Some(s) = &self.servizio {
            s.sincronizza_ora();
        }
    }

    pub fn ferma(mut self) {
        // fermando il servizio si chiude il canale, e il traduttore finisce da solo
        if let Some(s) = self.servizio.take() {
            s.ferma();
        }
        if let Some(t) = self.traduttore.take() {
            let _ = t.join();
        }
    }
}

fn traduci(app: tauri::AppHandle, rx: mpsc::Receiver<Notizia>) {
    let mut ultima_ok: Option<u64> = None;
    let mut ultima_ricarica = Instant::now() - RICARICA_OGNI;
    let rete = |stato: &str, extra: serde_json::Value, ultima_ok: Option<u64>| {
        let mut v = json!({ "stato": stato });
        if let Some(t) = ultima_ok {
            v["alle"] = json!(t);
        }
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    };
    // giro 2: da quando la rete è caduta, e se l'interfaccia lo sa già
    let mut caduta: Option<Instant> = None;
    let mut mostrata = false;
    loop {
        let n = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(n) => Some(n),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if let Some(da) = caduta
            && !mostrata
            && da.elapsed() >= OFFLINE_DOPO
        {
            mostrata = true;
            diario!("[motore] offline da {}: lo dico all'interfaccia", motore::tempo::durata(da.elapsed()));
            manda_rete(&app, rete("offline", json!({}), ultima_ok));
        }
        let Some(n) = n else { continue };
        // il server ha risposto: la caduta, se c'era, è finita
        if matches!(n, Notizia::Fine { .. } | Notizia::Contatto { .. } | Notizia::InLinea | Notizia::Passo { .. })
            && let Some(da) = caduta.take()
        {
            diario!("[motore] di nuovo in linea dopo {}", motore::tempo::durata(da.elapsed()));
            mostrata = false;
        }
        match n {
            Notizia::Inizio => {}
            Notizia::Passo { fase: _, fatti, totale } => {
                if totale >= AVANZAMENTO_DA {
                    manda_rete(&app, rete("sync", json!({ "fatti": fatti, "totale": totale }), ultima_ok));
                }
                // i primi messaggi si vedono mentre arrivano gli altri
                if fatti > 0 && ultima_ricarica.elapsed() >= RICARICA_OGNI {
                    ultima_ricarica = Instant::now();
                    let _ = app.emit("aggiornato", json!({}));
                    let _ = app.emit("primo-avvio", json!({ "tappa": "posta" }));
                }
            }
            Notizia::Fine { nuovi, aggiornati, spariti, testi } => {
                if nuovi + aggiornati + spariti + testi > 0 {
                    diario!("[motore] giro: {nuovi} nuovi, {aggiornati} cambiati (letto, stella, etichette), {spariti} spariti, {testi} testi");
                }
                ultima_ok = Some(adesso_ms());
                manda_rete(&app, rete("ok", json!({}), ultima_ok));
                if nuovi + aggiornati + spariti + testi > 0 {
                    ultima_ricarica = Instant::now();
                    let _ = app.emit("aggiornato", json!({}));
                }
                let _ = app.emit("primo-avvio", json!({ "tappa": "posta" }));
            }
            // il server ha risposto (giro, IDLE, controllo periodico): l'ora di «aggiornata alle»
            Notizia::Contatto { alle } => {
                ultima_ok = Some((alle.max(0) as u64) * 1000);
                manda_rete(&app, rete("ok", json!({}), ultima_ok));
            }
            Notizia::InLinea => {
                manda_rete(&app, rete("ok", json!({}), ultima_ok.or(Some(adesso_ms()))));
            }
            // giro 2: una caduta breve (il motore si ricollega da solo) non arriva all'interfaccia
            Notizia::Offline { motivo, riprovo_tra } => {
                let tra = riprovo_tra.map(|d| format!(", riprovo tra {}", motore::tempo::durata(d))).unwrap_or_default();
                diario!("[motore] caduta: {motivo}{tra}");
                if caduta.is_none() {
                    caduta = Some(Instant::now());
                }
            }
            Notizia::AccessoRifiutato { .. } => {
                diario!("[motore] accesso rifiutato");
                caduta = None;
                mostrata = false;
                manda_rete(&app, rete("auth", json!({}), ultima_ok));
            }
            Notizia::Errore { motivo } => {
                diario!("[motore] errore: {motivo}");
            }
        }
    }
}

/// Le opzioni della vista per l'interfaccia: l'HTML solo dove serve, il testo degli allegati a parte.
fn opzioni_vista(db: &Db, c: &Casella, dal: Option<vista::Dal>) -> Result<OpzioniVista, String> {
    let cfg = c.config();
    Ok(OpzioniVista {
        account: Account::da_config(&cfg, &c.nome, &[]),
        caselle: vec![vista::Casella::da_config(&cfg, db).map_err(|e| e.to_string())?],
        prefisso_allegati: PREFISSO_ALLEGATI.into(),
        limite: None,
        solo: None,
        dal,
        html_solo_senza_testo: true,
        testi_allegati: false,
        finestra: Some(FINESTRA),
    })
}

/// I dati per l'interfaccia, in JSON, nel formato `AMB_DATA` del prototipo. Con `dal`
/// solo quello che è cambiato dopo (giro 2).
pub fn dati(dati: &Path, c: &Casella, dal: Option<vista::Dal>) -> Result<String, String> {
    let db = Db::apri(&percorso_db(dati)).map_err(|e| format!("Non riesco ad aprire il database: {e}"))?;
    let differenza = dal.is_some();
    let opz = opzioni_vista(&db, c, dal)?;
    let t = Instant::now();
    let json = vista::json(&db, &opz).map_err(|e| format!("Non riesco a leggere il database: {e}"))?;
    let tipo = if !differenza {
        "tutti"
    } else if json.contains("\"delta\":true") {
        "differenza"
    } else {
        "tutti di nuovo"
    };
    diario!("[dati] {tipo}: {} KB in {} ms", json.len() / 1024, t.elapsed().as_millis());
    static PESI: AtomicBool = AtomicBool::new(false);
    if !PESI.swap(true, Ordering::SeqCst) {
        // a parte: con lo storico le somme scorrono tutto il database, e la pagina non le aspetta
        let percorso = percorso_db(dati);
        std::thread::spawn(move || {
            if let Ok(db) = Db::apri(&percorso) {
                pesi(&db);
            }
        });
    }
    Ok(json)
}

/// Solo misure, una volta per avvio: di cosa è fatto il peso (mai contenuti), e i non letti.
fn pesi(db: &Db) {
    {
        let mb = |sql: &str| -> f64 {
            db.conta(sql).map(|n| n as f64 / 1_048_576.0).unwrap_or(-1.0)
        };
        diario!(
            "[dati] nel database, in MB: testi {:.1}, HTML {:.1} (di cui nei messaggi senza testo {:.1}), testo degli allegati {:.1}",
            mb("SELECT coalesce(sum(length(testo)), 0) FROM corpi"),
            mb("SELECT coalesce(sum(length(html)), 0) FROM corpi"),
            mb("SELECT coalesce(sum(length(html)), 0) FROM corpi WHERE trim(coalesce(testo, '')) = ''"),
            mb("SELECT coalesce(sum(length(testo)), 0) FROM contenuti")
        );
        // per il confronto con Gmail, che conta solo la Posta in arrivo (giro 2)
        let n = |sql: &str| db.conta(sql).unwrap_or(-1);
        let non_letti = "FROM messaggi m WHERE m.eliminato IS NULL AND m.letto = 0
             AND NOT EXISTS (SELECT 1 FROM etichette e WHERE e.messaggio_id = m.id AND e.nome IN ('\\Sent', '\\Draft'))";
        diario!(
            "[dati] non letti: {} in tutta la casella, {} nella Posta in arrivo",
            n(&format!("SELECT count(*) {non_letti}")),
            n(&format!("SELECT count(*) {non_letti} AND EXISTS (SELECT 1 FROM etichette e WHERE e.messaggio_id = m.id AND e.nome = '\\Inbox')"))
        );
    }
}

/// Il testo dentro gli allegati, a parte: dopo il primo disegno, e poi solo i cambi.
pub fn testi_allegati(dati: &Path, c: &Casella, dal: Option<vista::Dal>) -> Result<String, String> {
    let db = Db::apri(&percorso_db(dati)).map_err(|e| format!("Non riesco ad aprire il database: {e}"))?;
    let differenza = dal.is_some();
    let opz = opzioni_vista(&db, c, dal)?;
    let t = Instant::now();
    let json = vista::testi_allegati(&db, &opz).map_err(|e| format!("Non riesco a leggere il database: {e}"))?;
    if !differenza || json.len() > 4096 {
        diario!("[dati] testo degli allegati: {} KB in {} ms", json.len() / 1024, t.elapsed().as_millis());
    }
    Ok(json)
}

/// Giro 2: i messaggi fuori dalla finestra che servono alla pagina (una persona aperta, una
/// ricerca), dal database. Il log dice solo quanti, mai chi o cosa.
pub fn storia(dati: &Path, c: &Casella, s: &vista::Storia) -> Result<String, String> {
    let db = Db::apri(&percorso_db(dati)).map_err(|e| format!("Non riesco ad aprire il database: {e}"))?;
    let opz = opzioni_vista(&db, c, None)?;
    let (tipo, quanti) = match s {
        vista::Storia::Persona(_) => ("persona", STORIA_PERSONA),
        vista::Storia::Cerca(_) => ("ricerca", STORIA_RICERCA),
    };
    let t = Instant::now();
    let json = vista::storia(&db, &opz, s, quanti).map_err(|e| format!("Non riesco a leggere il database: {e}"))?;
    let n = json.matches("\"localId\":").count();
    if n > 0 {
        diario!("[storia] {tipo}: {n} messaggi fuori dalla finestra, {} KB in {} ms", json.len() / 1024, t.elapsed().as_millis());
    }
    Ok(json)
}

/// Una connessione sola per gli allegati, riusata: solo il primo paga l'accesso.
static RICHIESTE: Mutex<Option<Richieste>> = Mutex::new(None);

/// Da chiamare quando la casella cambia (password nuova, casella tolta).
pub fn azzera_richieste() {
    if let Some(mut r) = RICHIESTE.lock().unwrap().take() {
        r.chiudi();
    }
    // quella del background può essere a metà di un file: la chiude lui al giro dopo
    AZZERA_SFONDO.store(true, Ordering::SeqCst);
    AZZERA_STORICO.store(true, Ordering::SeqCst);
}

pub struct FileAllegato {
    pub percorso: PathBuf,
    pub nome: String,
    pub tipo: String,
}

/// L'allegato `parte` del messaggio `id`: dall'archivio locale, altrimenti dal server.
fn allegato(dati: &Path, c: &Casella, id: i64, parte: &str) -> Result<FileAllegato, String> {
    allegato_con(&RICHIESTE, dati, c, id, parte)
}

/// Il background ha una connessione sua: un allegato grande non fa aspettare un clic.
static RICHIESTE_SFONDO: Mutex<Option<Richieste>> = Mutex::new(None);

fn allegato_con(conn: &Mutex<Option<Richieste>>, dati: &Path, c: &Casella, id: i64, parte: &str) -> Result<FileAllegato, String> {
    let mut db = Db::apri(&percorso_db(dati)).map_err(|e| format!("Non riesco ad aprire il database: {e}"))?;
    let a: Allegato = match su_richiesta::parte_locale(&db, id, parte).map_err(|e| e.to_string())? {
        Some(a) => a,
        None => {
            let mut guardia = conn.lock().unwrap();
            let r = guardia.get_or_insert_with(|| Richieste::new(c.config()));
            match r.parte(&mut db, id, parte) {
                Ok(a) => a,
                Err(e) => {
                    // il motivo del motore non contiene mai dati delle email
                    diario!("[motore] allegato {id}/{parte}: {e}");
                    // solo un problema della connessione la chiude: le richieste in fila
                    // dietro questa non devono ripartire tutte da capo (giro 1)
                    if connessione_rotta(&e)
                        && let Some(mut r) = guardia.take()
                    {
                        r.chiudi();
                    }
                    return Err(messaggio_errore(&e, c));
                }
            }
        }
    };
    Ok(FileAllegato {
        nome: a.nome_file.clone().unwrap_or_else(|| "allegato".into()),
        tipo: a.tipo.clone(),
        percorso: a.percorso,
    })
}

fn connessione_rotta(e: &Errore) -> bool {
    matches!(e, Errore::Rete(_) | Errore::Timeout | Errore::Chiusa | Errore::Tls(_) | Errore::Protocollo(_))
}

/// `/allegato/<id>/<parte>` → (id, parte).
fn leggi_percorso(percorso: &str) -> Option<(i64, String)> {
    leggi_percorso_con(percorso, "allegato/")
}

fn leggi_percorso_con(percorso: &str, prefisso: &str) -> Option<(i64, String)> {
    let resto = percorso.trim_start_matches('/').strip_prefix(prefisso)?;
    let (id, parte) = resto.split_once('/')?;
    let parte = parte.trim_end_matches('/');
    if parte.is_empty() || !parte.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some((id.parse().ok()?, parte.to_string()))
}

/// `/pagina/<id>/<parte>/<n>` → (id, parte, n).
fn leggi_pagina(percorso: &str) -> Option<(i64, String, u32)> {
    let resto = percorso.trim_start_matches('/').strip_prefix("pagina/")?;
    let (prima, n) = resto.trim_end_matches('/').rsplit_once('/')?;
    let (id, parte) = leggi_percorso_con(prima, "")?;
    Some((id, parte, n.parse().ok().filter(|&n| n > 0)?))
}

pub fn allegato_da_url(dati: &Path, c: &Casella, url: &str) -> Result<FileAllegato, String> {
    let percorso = url
        .strip_prefix("a10s://localhost")
        .or_else(|| url.strip_prefix("http://a10s.localhost"))
        .ok_or("Questo allegato non è dell'app.")?;
    let (id, parte) = leggi_percorso(percorso).ok_or("Indirizzo dell'allegato non valido.")?;
    allegato(dati, c, id, &parte)
}

/// Risponde alle richieste `a10s://localhost/allegato/<id>/<parte>` dell'interfaccia.
pub fn servi_allegato(app: &tauri::AppHandle, percorso: &str) -> tauri::http::Response<Vec<u8>> {
    let risposta = |codice: u16, tipo: &str, corpo: Vec<u8>| {
        tauri::http::Response::builder()
            .status(codice)
            .header("Content-Type", tipo)
            .header("Access-Control-Allow-Origin", "*")
            .header("Cache-Control", "private, max-age=31536000, immutable")
            .body(corpo)
            .unwrap()
    };
    let stato = app.state::<Stato>();
    // giro 2: una pagina del PDF per l'anteprima da sfogliare, `pagina/<id>/<parte>/<n>`
    if let Some((id, parte, n)) = leggi_pagina(percorso) {
        let Some(c) = stato.casella.lock().unwrap().clone() else {
            return risposta(404, "text/plain; charset=utf-8", b"nessuna casella".to_vec());
        };
        // il PDF prima dev'essere nell'archivio: se manca lo si scarica
        let trovata = allegato(&stato.dati, &c, id, &parte).and_then(|_| {
            let db = Db::apri(&percorso_db(&stato.dati)).map_err(|e| e.to_string())?;
            su_richiesta::pagina(&db, id, &parte, n).map_err(|e| e.to_string())
        });
        return match trovata.map(|p| p.and_then(|p| std::fs::read(p).ok())) {
            Ok(Some(corpo)) => risposta(200, "image/jpeg", corpo),
            Ok(None) => risposta(404, "text/plain; charset=utf-8", b"pagina assente".to_vec()),
            Err(e) => {
                diario!("[allegati] {id}/{parte} pagina {n}: {e}");
                let mut r = risposta(502, "text/plain; charset=utf-8", e.into_bytes());
                r.headers_mut().insert("Cache-Control", tauri::http::HeaderValue::from_static("no-store"));
                r
            }
        };
    }
    // la miniatura della prima pagina di un PDF: c'è solo se il motore l'ha già fatta
    if let Some((id, parte)) = leggi_percorso_con(percorso, "miniatura/") {
        let trovata = Db::apri(&percorso_db(&stato.dati))
            .ok()
            .and_then(|db| su_richiesta::miniatura(&db, id, &parte).ok().flatten())
            .and_then(|p| std::fs::read(p).ok());
        return match trovata {
            Some(corpo) => risposta(200, "image/jpeg", corpo),
            None => {
                let mut r = risposta(404, "text/plain; charset=utf-8", b"non ancora".to_vec());
                r.headers_mut().insert("Cache-Control", tauri::http::HeaderValue::from_static("no-store"));
                r
            }
        };
    }
    let Some((id, parte)) = leggi_percorso(percorso) else {
        return risposta(404, "text/plain; charset=utf-8", b"non trovato".to_vec());
    };
    let Some(c) = stato.casella.lock().unwrap().clone() else {
        return risposta(404, "text/plain; charset=utf-8", b"nessuna casella".to_vec());
    };
    match allegato(&stato.dati, &c, id, &parte).and_then(|f| {
        std::fs::read(&f.percorso).map(|b| (f, b)).map_err(|e| e.to_string())
    }) {
        Ok((f, corpo)) => {
            diario!("[allegati] {id}/{parte}: servito, {} ({} byte)", f.tipo, corpo.len());
            risposta(200, &tipo_servibile(&f.tipo), corpo)
        }
        Err(e) => {
            diario!("[allegati] {id}/{parte}: {e}");
            // un errore non si ricorda: la prossima richiesta riprova davvero
            let mut r = risposta(502, "text/plain; charset=utf-8", e.into_bytes());
            r.headers_mut().insert("Cache-Control", tauri::http::HeaderValue::from_static("no-store"));
            r
        }
    }
}

/// Il tipo da dichiarare: l'HTML di un allegato si serve come testo, così non diventa
/// una pagina con i permessi dell'app.
fn tipo_servibile(tipo: &str) -> String {
    let t = tipo.trim().to_ascii_lowercase();
    if t.is_empty() || t.contains("html") || t.contains("xml") || t.contains("javascript") || t.contains("svg") {
        if t.contains("svg") {
            // le immagini SVG si mostrano come immagini (in <img> gli script non girano)
            return "image/svg+xml".into();
        }
        return "text/plain; charset=utf-8".into();
    }
    if t.starts_with("text/") && !t.contains("charset") {
        return format!("{t}; charset=utf-8");
    }
    t
}

/// Cancella database e allegati della casella tolta. Solo file dentro la cartella dei dati.
pub fn cancella_dati_locali(dati: &Path) -> std::io::Result<()> {
    azzera_richieste();
    let db = percorso_db(dati);
    for est in ["", "-wal", "-shm", "-journal"] {
        let f = PathBuf::from(format!("{}{est}", db.display()));
        match std::fs::remove_file(&f) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    match std::fs::remove_dir_all(dati.join("allegati")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/* ---------- «Salva»: il messaggio intero, come .eml in Scaricati (giro 1) ---------- */

/// Chiede al server il messaggio intero (`BODY.PEEK[]`) e lo scrive in Scaricati come
/// `{id}-{oggetto}.eml`. Lo stesso messaggio salvato di nuovo sostituisce il file; un file
/// con lo stesso nome ma un altro messaggio resta, e il nuovo prende « (2)».
pub fn salva_messaggio(dati: &Path, c: &Casella, chiave: &str) -> Result<PathBuf, String> {
    let mut db = Db::apri(&percorso_db(dati)).map_err(|e| format!("Non riesco ad aprire il database: {e}"))?;
    let id = db
        .messaggio_per_chiave(chiave)
        .map_err(|e| e.to_string())?
        .ok_or("Questo messaggio non c'è più nel database.")?;
    let nome = su_richiesta::nome_eml_messaggio(&db, id).map_err(|e| e.to_string())?;
    let corpo = {
        let mut guardia = RICHIESTE.lock().unwrap();
        let r = guardia.get_or_insert_with(|| Richieste::new(c.config()));
        match r.messaggio_intero(&mut db, id) {
            Ok(b) => b,
            Err(e) => {
                diario!("[motore] messaggio intero {id}: {e}");
                if connessione_rotta(&e)
                    && let Some(mut r) = guardia.take()
                {
                    r.chiudi();
                }
                return Err(messaggio_errore(&e, c));
            }
        }
    };
    let cartella = crate::sistema::cartella_scaricati();
    std::fs::create_dir_all(&cartella).map_err(|e| format!("Non riesco a salvare il file: {e}"))?;
    let mut dest = cartella.join(crate::sistema::nome_sicuro(&nome));
    if dest.exists() && !stesso_messaggio(&dest, &corpo) {
        dest = crate::sistema::libero(&cartella, &crate::sistema::nome_sicuro(&nome));
    }
    std::fs::write(&dest, &corpo).map_err(|e| format!("Non riesco a salvare il file: {e}"))?;
    diario!("[messaggi] {id}: salvato ({} byte)", corpo.len());
    Ok(dest)
}

/// Il file che c'è è lo stesso messaggio? Si confronta il Message-ID delle intestazioni.
fn stesso_messaggio(file: &Path, nuovo: &[u8]) -> bool {
    let vecchio = std::fs::read(file).unwrap_or_default();
    match (message_id(&vecchio), message_id(nuovo)) {
        (Some(a), Some(b)) => a == b,
        _ => vecchio == nuovo,
    }
}

fn message_id(eml: &[u8]) -> Option<String> {
    let testa = String::from_utf8_lossy(&eml[..eml.len().min(64 * 1024)]);
    let testa = testa.split("\r\n\r\n").next().unwrap_or("").split("\n\n").next().unwrap_or("").to_string();
    let righe: Vec<&str> = testa.lines().collect();
    for (i, r) in righe.iter().enumerate() {
        if r.len() >= 11 && r[..11].eq_ignore_ascii_case("message-id:") {
            let mut v = r[11..].trim().to_string();
            // il valore può continuare sulla riga dopo
            if v.is_empty() {
                v = righe.get(i + 1).map(|x| x.trim().to_string()).unwrap_or_default();
            }
            return Some(v.trim_matches(['<', '>']).to_string()).filter(|v| !v.is_empty());
        }
    }
    None
}

/* ---------- gli allegati in background (giro 1, proposta 6) ---------- */

static FERMA_SCARICO: AtomicBool = AtomicBool::new(false);
static SCARICO_AVVIATO: AtomicBool = AtomicBool::new(false);
/// Quello che si vede in Allegati (più un paio di pagine): passa in testa alla coda.
static IN_TESTA: Mutex<Vec<(i64, String)>> = Mutex::new(Vec::new());

/// L'interfaccia dice quali allegati ha sullo schermo, come indirizzi `a10s://localhost/allegato/…`.
pub fn metti_in_testa(urls: &[String]) {
    let lista: Vec<(i64, String)> = urls
        .iter()
        .filter_map(|u| u.strip_prefix("a10s://localhost").or_else(|| u.strip_prefix("http://a10s.localhost")))
        .filter_map(leggi_percorso)
        .collect();
    *IN_TESTA.lock().unwrap() = lista;
}

/// Un thread che scarica tutti gli allegati, su una connessione sua: un clic su un allegato
/// non aspetta il background. Si ferma con la quota del giorno, sulla connessione a consumo
/// e col disco quasi pieno.
pub fn avvia_scaricamento(app: tauri::AppHandle) {
    if SCARICO_AVVIATO.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("a10s-scarico".into())
        .spawn(move || ciclo_scaricamento(app))
        .expect("thread degli allegati in background");
}

pub fn ferma_scaricamento() {
    FERMA_SCARICO.store(true, Ordering::SeqCst);
}

static AZZERA_SFONDO: AtomicBool = AtomicBool::new(false);

fn dormi(secondi: u64) {
    for _ in 0..secondi * 10 {
        if FERMA_SCARICO.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn ciclo_scaricamento(app: tauri::AppHandle) {
    bassa_priorita();
    // la prima sincronizzazione ha la precedenza
    dormi(20);
    let mut completato = false;
    let mut ultimo_avviso = Instant::now();
    let mut fatti_dopo_avviso = 0usize;
    let mut contenuti_letti = 0usize;
    while !FERMA_SCARICO.load(Ordering::SeqCst) {
        if AZZERA_SFONDO.swap(false, Ordering::SeqCst)
            && let Some(mut r) = RICHIESTE_SFONDO.lock().unwrap().take()
        {
            r.chiudi();
        }
        let stato = app.state::<Stato>();
        let Some(c) = stato.casella.lock().unwrap().clone() else {
            dormi(10);
            continue;
        };
        let Ok(db) = Db::apri(&percorso_db(&stato.dati)) else {
            dormi(30);
            continue;
        };
        if !completato {
            // misure e miniature per quello che era già nell'archivio
            match su_richiesta::completa_archivio(&db) {
                Ok(n) if n > 0 => diario!("[scarico] completati {n} allegati già scaricati"),
                Ok(_) => {}
                Err(e) => diario!("[scarico] completa archivio: {e}"),
            }
            completato = true;
        }
        // giro 2: testo e codici di quello che è già nell'archivio, pochi alla volta (niente rete)
        match su_richiesta::completa_contenuti(&db, 8) {
            Ok(n) if n > 0 => contenuti_letti += n,
            Ok(_) => {}
            Err(e) => diario!("[scarico] testo degli allegati: {e}"),
        }
        // giro 2: e le miniature delle immagini grandi, così la pagina non apre le foto intere
        contenuti_letti += riduci_immagini(&db, 8);
        if let Ok(Some(p)) = scaricamento::pausa(&db, QUOTA_CONSIGLIATA) {
            diario!("[scarico] in pausa: {}", match p { Pausa::Quota => "quota del giorno", Pausa::AConsumo => "connessione a consumo", Pausa::Disco => "disco quasi pieno" });
            chiudi_sfondo();
            dormi(120);
            continue;
        }
        let in_testa = IN_TESTA.lock().unwrap().clone();
        let lista = scaricamento::coda(&db, 4, &in_testa).unwrap_or_default();
        drop(db);
        if lista.is_empty() {
            // giro 2: niente da scaricare, la connessione si chiude (non resta ferma in CLOSE-WAIT)
            chiudi_sfondo();
            // e si legge quello che resta nell'archivio, a blocchi, senza far ricaricare
            // l'interfaccia a ogni blocco (al primo avvio sono migliaia di allegati)
            let letti = leggi_contenuti(&stato.dati) + contenuti_letti;
            contenuti_letti = 0;
            if letti > 0 {
                let _ = app.emit("aggiornato", json!({}));
            }
            dormi(60);
            continue;
        }
        let mut visti = false;
        for d in &lista {
            if FERMA_SCARICO.load(Ordering::SeqCst) {
                return;
            }
            if allegato_con(&RICHIESTE_SFONDO, &stato.dati, &c, d.messaggio, &d.parte).is_ok() {
                fatti_dopo_avviso += 1;
                visti |= in_testa.iter().any(|(m, p)| *m == d.messaggio && *p == d.parte);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // l'interfaccia ricarica i dati per le miniature nuove: presto per quelle sullo schermo, di rado per le altre
        let attesa = if visti { Duration::from_secs(3) } else { Duration::from_secs(120) };
        if (fatti_dopo_avviso > 0 || contenuti_letti > 0) && ultimo_avviso.elapsed() >= attesa {
            ultimo_avviso = Instant::now();
            fatti_dopo_avviso = 0;
            contenuti_letti = 0;
            let _ = app.emit("aggiornato", json!({}));
        }
    }
}

/// Le miniature di al massimo `quante` immagini grandi dell'archivio. Un JPEG del telefono aperto
/// intero occupa decine di MB nella pagina, la sua miniatura poche centinaia di KB. Restituisce
/// quante ne ha guardate.
fn riduci_immagini(db: &Db, quante: usize) -> usize {
    let lista = match su_richiesta::immagini_da_ridurre(db, quante) {
        Ok(l) => l,
        Err(e) => {
            diario!("[scarico] miniature: {e}");
            return 0;
        }
    };
    let mut n = 0;
    for i in lista {
        // lo stesso contenuto in più messaggi: la miniatura si fa una volta
        let fatta = i.miniatura.is_file() || crate::sistema::miniatura_immagine(&i.file, &i.miniatura);
        if su_richiesta::miniatura_fatta(db, i.messaggio, &i.parte, fatta).is_ok() {
            n += 1;
        }
    }
    n
}

/// Ogni quanto, durante una lettura lunga, il log dice a che punto è.
const CONTENUTI_AVANZAMENTO: Duration = Duration::from_secs(60);

/// Testo e codici di tutto quello che è nell'archivio e non è ancora stato guardato, e le
/// miniature delle immagini grandi, a blocchi di 25 con una pausa fra l'uno e l'altro.
/// L'interfaccia non si avvisa a metà: ricaricare i dati costa, e al primo avvio sono migliaia
/// di allegati. Restituisce quanti ne ha letti.
fn leggi_contenuti(dati: &Path) -> usize {
    let mut tutti = 0usize;
    let inizio = Instant::now();
    let mut avanzamento = Instant::now();
    while !FERMA_SCARICO.load(Ordering::SeqCst) {
        let Ok(db) = Db::apri(&percorso_db(dati)) else { break };
        let n = match su_richiesta::completa_contenuti(&db, 25) {
            Ok(n) => n,
            Err(e) => {
                diario!("[scarico] testo degli allegati: {e}");
                break;
            }
        } + riduci_immagini(&db, 10);
        drop(db);
        if n == 0 {
            break;
        }
        tutti += n;
        if avanzamento.elapsed() >= CONTENUTI_AVANZAMENTO {
            diario!("[scarico] testo e codici: {tutti} allegati letti finora");
            avanzamento = Instant::now();
        }
        // a bassa priorità: i programmi esterni (pdftotext, zbarimg) non devono occupare la CPU
        std::thread::sleep(Duration::from_millis(250));
    }
    if tutti > 0 {
        diario!("[scarico] testo e codici: {tutti} allegati letti in {}", motore::tempo::durata(inizio.elapsed()));
    }
    tutti
}

fn chiudi_sfondo() {
    if let Some(mut r) = RICHIESTE_SFONDO.lock().unwrap().take() {
        r.chiudi();
    }
}

/* ---------- le connessioni ferme (giro 2) ---------- */

/// Ogni quanto si guardano le connessioni degli allegati.
const GUARDA_OGNI: u64 = 15;
/// Dopo quanto senza richieste la connessione dei clic riceve un NOOP, che la tiene viva:
/// meno dei due minuti dopo cui il motore la richiuderebbe al clic successivo.
const NOOP_DOPO: Duration = Duration::from_secs(100);
/// Un salto dell'orologio più lungo di così fra due giri del guardiano è una sospensione.
const SOSPENSIONE: Duration = Duration::from_secs(30);

static GUARDIANO_AVVIATO: AtomicBool = AtomicBool::new(false);

/// Un thread che tiene d'occhio le connessioni degli allegati: se il server ne chiude una
/// ferma (Gmail lo fa dopo un po' di inattività) la chiude anche qui e riapre quella dei
/// clic subito, invece di lasciarla in CLOSE-WAIT e far pagare l'attesa al primo clic.
/// Dopo una sospensione del computer le riapre, e chiede un giro alla sincronizzazione.
pub fn avvia_guardiano(app: tauri::AppHandle) {
    if GUARDIANO_AVVIATO.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("a10s-connessioni".into())
        .spawn(move || guarda_connessioni(app))
        .expect("thread delle connessioni");
}

fn guarda_connessioni(app: tauri::AppHandle) {
    let mut riapri = false;
    let mut ultimo_errore = String::new();
    let mut prima = (Instant::now(), SystemTime::now());
    while !FERMA_SCARICO.load(Ordering::SeqCst) {
        dormi(GUARDA_OGNI);
        if FERMA_SCARICO.load(Ordering::SeqCst) {
            return;
        }
        // l'orologio del sistema va avanti anche in sospensione, quello monotono no
        let ora = (Instant::now(), SystemTime::now());
        let parete = ora.1.duration_since(prima.1).unwrap_or_default();
        let sospeso = parete.saturating_sub(ora.0 - prima.0);
        prima = ora;
        if sospeso > SOSPENSIONE {
            diario!("[connessioni] il computer era sospeso da {}: riapro le connessioni", motore::tempo::durata(sospeso));
            let aperta = RICHIESTE.lock().unwrap().as_mut().map(|r| {
                r.chiudi();
                true
            });
            riapri = aperta.unwrap_or(false);
            chiudi_sfondo();
            if let Some(s) = app.state::<Stato>().sincronia.lock().unwrap().as_ref() {
                s.aggiorna();
            }
        }
        if let Ok(mut g) = RICHIESTE.try_lock() {
            match g.as_mut() {
                None => riapri = false,
                Some(r) => {
                    match r.mantieni(NOOP_DOPO) {
                        Mantenuta::ChiusaDalServer => {
                            diario!("[connessioni] il server ha chiuso la connessione degli allegati, ferma: la riapro");
                            riapri = true;
                        }
                        Mantenuta::Caduta(m) => {
                            diario!("[connessioni] la connessione degli allegati non risponde ({m}): la riapro");
                            riapri = true;
                        }
                        Mantenuta::Viva | Mantenuta::Nessuna => {}
                    }
                    if riapri && !r.collegata() {
                        match r.collega() {
                            Ok(()) => {
                                diario!("[connessioni] connessione degli allegati riaperta");
                                riapri = false;
                                ultimo_errore.clear();
                            }
                            // la password non va più: lo dice la sincronizzazione, qui non si insiste
                            Err(Errore::Accesso(_)) => {
                                riapri = false;
                                *g = None;
                            }
                            Err(e) => {
                                let e = e.to_string();
                                if e != ultimo_errore {
                                    diario!("[connessioni] non riesco a riaprire la connessione degli allegati: {e}");
                                    ultimo_errore = e;
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Ok(mut g) = RICHIESTE_SFONDO.try_lock()
            && let Some(r) = g.as_mut()
            && matches!(r.mantieni(NOOP_DOPO), Mantenuta::ChiusaDalServer | Mantenuta::Caduta(_))
        {
            diario!("[connessioni] la connessione del background era chiusa: la riapre lui quando serve");
        }
    }
}

/// Per il pannello delle caselle: quanto è scaricato, quanto manca, e se è in pausa.
pub fn stato_allegati(dati: &Path) -> Result<serde_json::Value, String> {
    let db = Db::apri(&percorso_db(dati)).map_err(|e| format!("Non riesco ad aprire il database: {e}"))?;
    let t = scaricamento::totali(&db).map_err(|e| e.to_string())?;
    let pausa = scaricamento::pausa(&db, QUOTA_CONSIGLIATA).ok().flatten().map(|p| match p {
        Pausa::Quota => "quota",
        Pausa::AConsumo => "consumo",
        Pausa::Disco => "disco",
    });
    let st = storico::stato(&db).map_err(|e| e.to_string())?;
    Ok(json!({
        "scaricati": t.tutti.scaricati,
        "byteScaricati": t.tutti.byte_scaricati,
        "daScaricare": t.tutti.da_scaricare,
        "byteDaScaricare": t.tutti.byte_da_scaricare,
        "pausa": pausa,
        // giro 2: lo storico, solo finché non è finito
        "storico": st.filter(|_| !STORICO_FINITO.load(Ordering::SeqCst)).map(|s| json!({
            "sulServer": s.sul_server,
            "inLocale": s.in_locale,
            "senzaTesto": s.senza_testo,
        })),
    }))
}

/* ---------- lo storico in background (giro 2) ---------- */

static STORICO_AVVIATO: AtomicBool = AtomicBool::new(false);
static STORICO_FINITO: AtomicBool = AtomicBool::new(false);
/// Quanti messaggi per passo: un lotto di intestazioni o di testi.
const LOTTO_STORICO: usize = 200;
/// Ogni quanto, al massimo, la pagina sente dello storico: i thread della finestra possono
/// ricevere un messaggio vecchio.
const AVVISO_STORICO: Duration = Duration::from_secs(60);

/// Un thread che scarica lo storico su una connessione sua, a bassa priorità: prima le
/// intestazioni di tutta la casella, poi i testi, dal più recente indietro. Si ferma con la
/// quota del giorno, la connessione a consumo e il disco quasi pieno, come gli allegati.
pub fn avvia_storico(app: tauri::AppHandle) {
    if STORICO_AVVIATO.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("a10s-storico".into())
        .spawn(move || ciclo_storico(app))
        .expect("thread dello storico");
}

/// Abbassa la priorità del thread che chiama: la CPU va prima alla finestra e alla posta nuova.
fn bassa_priorita() {
    // su Linux la priorità di un thread si cambia col suo id
    #[cfg(target_os = "linux")]
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 10);
    }
}

fn ciclo_storico(app: tauri::AppHandle) {
    bassa_priorita();
    // la prima sincronizzazione ha la precedenza
    dormi(30);
    let mut st: Option<(Casella, Storico)> = None;
    let mut attese = motore::sincronizza::Attese::new(Duration::from_secs(30), Duration::from_secs(600));
    let mut ultimo_avviso = Instant::now();
    let (mut intestazioni, mut testi, mut byte) = (0usize, 0usize, 0u64);
    let inizio = Instant::now();
    let mut avanzamento = Instant::now();
    while !FERMA_SCARICO.load(Ordering::SeqCst) {
        let stato = app.state::<Stato>();
        let Some(c) = stato.casella.lock().unwrap().clone() else {
            st = None;
            dormi(10);
            continue;
        };
        // casella cambiata (password nuova, casella tolta e rimessa): connessione nuova
        if st.as_ref().is_some_and(|(vecchia, _)| vecchia.indirizzo != c.indirizzo || vecchia.imap != c.imap || vecchia.porta != c.porta)
            || AZZERA_STORICO.swap(false, Ordering::SeqCst)
        {
            if let Some((_, mut s)) = st.take() {
                s.chiudi();
            }
        }
        let Ok(mut db) = Db::apri(&percorso_db(&stato.dati)) else {
            dormi(30);
            continue;
        };
        if let Ok(Some(p)) = scaricamento::pausa(&db, QUOTA_CONSIGLIATA) {
            diario!("[storico] in pausa: {}", match p { Pausa::Quota => "quota del giorno", Pausa::AConsumo => "connessione a consumo", Pausa::Disco => "disco quasi pieno" });
            if let Some((_, s)) = st.as_mut() {
                s.chiudi();
            }
            drop(db);
            dormi(300);
            continue;
        }
        let (_, s) = st.get_or_insert_with(|| (c.clone(), Storico::new(c.config(), FINESTRA)));
        let esito = s.passo(&mut db, LOTTO_STORICO);
        drop(db);
        match esito {
            Ok(p) if p.aspetta => dormi(30),
            Ok(p) if p.finito => {
                if intestazioni + testi > 0 {
                    diario!(
                        "[storico] finito: {intestazioni} messaggi e {testi} testi in {}, {} MB",
                        motore::tempo::durata(inizio.elapsed()),
                        byte / 1_048_576
                    );
                    let _ = app.emit("aggiornato", json!({}));
                }
                (intestazioni, testi, byte) = (0, 0, 0);
                STORICO_FINITO.store(true, Ordering::SeqCst);
                s.chiudi();
                // la finestra scorre in avanti: lo storico non cresce più, ma si ricontrolla
                dormi(3600);
                STORICO_FINITO.store(false, Ordering::SeqCst);
            }
            Ok(p) => {
                attese.azzera();
                intestazioni += p.intestazioni;
                testi += p.testi;
                byte += p.byte;
                if avanzamento.elapsed() >= Duration::from_secs(300) {
                    avanzamento = Instant::now();
                    diario!("[storico] finora {intestazioni} messaggi e {testi} testi, {} MB; restano {} intestazioni", byte / 1_048_576, p.restano);
                }
                if ultimo_avviso.elapsed() >= AVVISO_STORICO {
                    ultimo_avviso = Instant::now();
                    let _ = app.emit("aggiornato", json!({}));
                }
                // a bassa priorità anche in rete: un passo ogni secondo lascia spazio ai clic
                std::thread::sleep(Duration::from_millis(if p.testi > 0 { 1500 } else { 800 }));
            }
            Err(e) => {
                let attesa = attese.prossima();
                // il motivo del motore non contiene mai dati delle email
                diario!("[storico] {e}: riprovo tra {}", motore::tempo::durata(attesa));
                if !e.di_rete() {
                    s.chiudi();
                }
                dormi(attesa.as_secs());
            }
        }
    }
    if let Some((_, mut s)) = st {
        s.chiudi();
    }
}

static AZZERA_STORICO: AtomicBool = AtomicBool::new(false);
