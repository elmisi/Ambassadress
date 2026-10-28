//! Ambassadress, l'app desktop: una finestra Tauri con l'interfaccia della v13
//! (in `ui/`) sopra il motore. Questa versione legge soltanto: niente cambia sul server.
//!
//! - Comandi per l'interfaccia (`ponte.js`): avvio, dati, primo avvio, password,
//!   togli casella, allegati, link, notifiche.
//! - Eventi verso l'interfaccia: `rete` (stato della rete), `aggiornato` (dati nuovi
//!   nel database), `primo-avvio` (le tappe del collegamento).
//! - Gli allegati si servono dal protocollo `a10s://localhost/allegato/<messaggio>/<parte>`:
//!   il motore li scarica la prima volta e li tiene nell'archivio per contenuto.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod casella;
mod motore_app;
mod sistema;

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};

use casella::Casella;
use motore_app::Sincronia;

pub struct Stato {
    pub dati: PathBuf,
    pub casella: Mutex<Option<Casella>>,
    /// L'ultimo stato della rete mandato all'interfaccia, per chi apre la finestra dopo.
    pub rete: Mutex<Value>,
    pub sincronia: Mutex<Option<Sincronia>>,
}

impl Stato {
    fn casella(&self) -> Result<Casella, String> {
        self.casella.lock().unwrap().clone().ok_or_else(|| "Nessuna casella configurata.".to_string())
    }
}

/// Manda lo stato della rete all'interfaccia e lo ricorda.
pub fn manda_rete(app: &tauri::AppHandle, rete: Value) {
    if let Some(s) = app.try_state::<Stato>() {
        *s.rete.lock().unwrap() = rete.clone();
    }
    let _ = app.emit("rete", rete);
}

fn avvia_sincronia(app: &tauri::AppHandle, stato: &Stato, c: Casella) {
    let mut s = stato.sincronia.lock().unwrap();
    if let Some(vecchia) = s.take() {
        vecchia.ferma();
    }
    *s = Some(Sincronia::avvia(app.clone(), stato.dati.clone(), c));
}

fn ferma_sincronia(stato: &Stato) {
    if let Some(vecchia) = stato.sincronia.lock().unwrap().take() {
        vecchia.ferma();
    }
}

#[tauri::command]
fn avvio(stato: State<'_, Stato>) -> Value {
    let c = stato.casella.lock().unwrap().clone();
    json!({
        "configurata": c.is_some(),
        "casella": c.map(|c| c.pubblica()),
        "rete": stato.rete.lock().unwrap().clone(),
    })
}

/// Tutti i dati per l'interfaccia, nel formato `AMB_DATA` del prototipo.
#[tauri::command]
async fn dati(stato: State<'_, Stato>) -> Result<tauri::ipc::Response, String> {
    let c = stato.casella()?;
    let dati = stato.dati.clone();
    let json = tauri::async_runtime::spawn_blocking(move || motore_app::dati(&dati, &c))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(json))
}

/// Il primo avvio: prova a collegarsi e ad accedere, poi salva la casella e sincronizza.
#[tauri::command]
async fn configura(
    app: tauri::AppHandle,
    stato: State<'_, Stato>,
    indirizzo: String,
    password: String,
    imap: String,
    smtp: String,
    tipo: String,
) -> Result<Value, String> {
    let indirizzo = indirizzo.trim().to_lowercase();
    let (host, porta, in_chiaro) = server_di_prova().unwrap_or((imap.trim().to_string(), 993, false));
    if host.is_empty() {
        return Err("Manca il server IMAP.".into());
    }
    let c = Casella {
        indirizzo,
        nome: String::new(),
        tipo: if tipo == "gmail" { "gmail".into() } else { "imap".into() },
        imap: host,
        porta,
        smtp: smtp.trim().to_string(),
        in_chiaro,
        password: password.split_whitespace().collect(),
    };
    let prova = c.clone();
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        motore_app::prova_accesso(&prova, |tappa| {
            let _ = a.emit("primo-avvio", json!({ "tappa": tappa }));
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    casella::salva(&stato.dati, &c).map_err(|e| format!("Non riesco a salvare la casella: {e}"))?;
    *stato.casella.lock().unwrap() = Some(c.clone());
    avvia_sincronia(&app, &stato, c.clone());
    Ok(json!({ "casella": c.pubblica() }))
}

/// Per le prove: `A10S_SERVER=127.0.0.1:1143` usa il finto server, in chiaro.
/// Solo su questo computer: una password non deve mai uscire in chiaro.
fn server_di_prova() -> Option<(String, u16, bool)> {
    let v = std::env::var("A10S_SERVER").ok()?;
    let (h, p) = v.rsplit_once(':')?;
    if !matches!(h, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        eprintln!("[prova] A10S_SERVER vale solo per 127.0.0.1 o localhost: lo ignoro");
        return None;
    }
    Some((h.trim_matches(['[', ']']).to_string(), p.parse().ok()?, true))
}

#[tauri::command]
async fn cambia_password(app: tauri::AppHandle, stato: State<'_, Stato>, password: String) -> Result<bool, String> {
    let mut c = stato.casella()?;
    c.password = password.split_whitespace().collect();
    let prova = c.clone();
    tauri::async_runtime::spawn_blocking(move || motore_app::prova_accesso(&prova, |_| {}))
        .await
        .map_err(|e| e.to_string())??;
    casella::salva(&stato.dati, &c).map_err(|e| format!("Non riesco a salvare la password: {e}"))?;
    motore_app::azzera_richieste();
    *stato.casella.lock().unwrap() = Some(c.clone());
    avvia_sincronia(&app, &stato, c);
    Ok(true)
}

/// Toglie la casella e i suoi dati da questo computer. Sul server non cambia niente.
#[tauri::command]
async fn togli_casella(app: tauri::AppHandle, stato: State<'_, Stato>) -> Result<bool, String> {
    ferma_sincronia(&stato);
    motore_app::azzera_richieste();
    casella::togli(&stato.dati).map_err(|e| e.to_string())?;
    *stato.casella.lock().unwrap() = None;
    motore_app::cancella_dati_locali(&stato.dati).map_err(|e| format!("Casella tolta, ma non riesco a cancellare i suoi dati: {e}"))?;
    manda_rete(&app, json!({ "stato": "ok" }));
    Ok(true)
}

#[tauri::command]
async fn apri_allegato(stato: State<'_, Stato>, url: String, nome: String) -> Result<bool, String> {
    let c = stato.casella()?;
    let dati = stato.dati.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let f = motore_app::allegato_da_url(&dati, &c, &url)?;
        let cartella = sistema::cartella_aperti();
        let copia = sistema::copia_con_nome(&f.percorso, &cartella, if nome.is_empty() { &f.nome } else { &nome })
            .map_err(|e| format!("Non riesco a preparare il file: {e}"))?;
        sistema::apri(&copia.to_string_lossy()).map_err(|e| format!("Non riesco ad aprire il file: {e}"))?;
        eprintln!("[allegati] passato a xdg-open: {url} ({})", f.tipo);
        Ok::<_, String>(true)
    })
    .await
    .map_err(|e| e.to_string())?
    .inspect_err(|e| eprintln!("[allegati] «Apri» non riesce: {e}"))
}

#[tauri::command]
async fn salva_allegato(stato: State<'_, Stato>, url: String, nome: String) -> Result<String, String> {
    let c = stato.casella()?;
    let dati = stato.dati.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let f = motore_app::allegato_da_url(&dati, &c, &url)?;
        let cartella = sistema::cartella_scaricati();
        let dest = sistema::copia_con_nome(&f.percorso, &cartella, if nome.is_empty() { &f.nome } else { &nome })
            .map_err(|e| format!("Non riesco a salvare il file: {e}"))?;
        let casa = std::env::var("HOME").unwrap_or_default();
        let mostra = dest.to_string_lossy().to_string();
        Ok::<_, String>(match mostra.strip_prefix(&casa) {
            Some(resto) if !casa.is_empty() => format!("~{resto}"),
            _ => mostra,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// «Salva» nel menu «⋯»: il messaggio intero in Scaricati, come .eml.
#[tauri::command]
async fn salva_messaggio(stato: State<'_, Stato>, id: String) -> Result<String, String> {
    let c = stato.casella()?;
    let dati = stato.dati.clone();
    tauri::async_runtime::spawn_blocking(move || motore_app::salva_messaggio(&dati, &c, &id).map(|p| sistema::da_mostrare(&p)))
        .await
        .map_err(|e| e.to_string())?
}

/// Gli allegati che si vedono in Allegati passano in testa alla coda del background.
#[tauri::command]
fn priorita(urls: Vec<String>) {
    motore_app::metti_in_testa(&urls);
}

/// Per il pannello delle caselle: allegati scaricati, da scaricare, pausa.
#[tauri::command]
async fn stato_allegati(stato: State<'_, Stato>) -> Result<Value, String> {
    let dati = stato.dati.clone();
    tauri::async_runtime::spawn_blocking(move || motore_app::stato_allegati(&dati)).await.map_err(|e| e.to_string())?
}

/// «aggiornata alle …»: un clic chiede al motore di sincronizzare subito.
#[tauri::command]
fn aggiorna(stato: State<'_, Stato>) -> Result<bool, String> {
    match stato.sincronia.lock().unwrap().as_ref() {
        Some(s) => {
            s.aggiorna();
            Ok(true)
        }
        None => Err("Nessuna casella configurata.".into()),
    }
}

/// Il nome del programma che apre questo tipo di file («Papers»), per «Apri con …».
#[tauri::command]
async fn programma_per(tipo: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || sistema::programma_per(&tipo)).await.ok().flatten()
}

/// «Scarica .json» in «Segnala»: un testo in Scaricati, senza lasciare la pagina.
#[tauri::command]
async fn salva_testo(nome: String, testo: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sistema::salva_testo(&nome, &testo).map(|p| sistema::da_mostrare(&p)).map_err(|e| format!("Non riesco a salvare il file: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn apri_link(url: String) -> Result<bool, String> {
    let u = url.trim();
    let minuscolo = u.to_ascii_lowercase();
    if !(minuscolo.starts_with("https://") || minuscolo.starts_with("http://") || minuscolo.starts_with("mailto:")) {
        return Err("Questo link non si apre.".into());
    }
    sistema::apri(u).map_err(|e| format!("Non riesco ad aprire il link: {e}"))?;
    Ok(true)
}

/// Gli errori della pagina, sul terminale da cui si è lanciata l'app.
#[tauri::command]
fn diario(riga: String) {
    eprintln!("[interfaccia] {}", riga.chars().take(2000).collect::<String>());
}

#[tauri::command]
fn notifica(titolo: String, testo: String) {
    sistema::notifica(&titolo, &testo);
}

/// Solo per le prove: `A10S_PROVA_JS=file.js` esegue quello script nella pagina, dopo
/// tre secondi. Lo script scrive i risultati con `A10S.invoca('diario', {riga})`.
/// Solo per le prove: `A10S_PROVA_CHIUDI=secondi` chiude la finestra come la ×, per
/// misurare la chiusura (quanto ci mette, se WebKit cade).
fn prova_chiusura(app: &tauri::AppHandle) {
    let Some(secondi) = std::env::var("A10S_PROVA_CHIUDI").ok().and_then(|v| v.parse::<f64>().ok()) else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs_f64(secondi));
        eprintln!("[prova] chiudo la finestra");
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.close();
        }
    });
}

fn prova_scriptata(app: &tauri::AppHandle) {
    let Some(file) = std::env::var_os("A10S_PROVA_JS") else { return };
    let Ok(js) = std::fs::read_to_string(&file) else {
        eprintln!("[prova] non riesco a leggere lo script");
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.eval(&js);
        }
    });
}

/// La finestra mostra solo le pagine dell'app e gli allegati del protocollo `a10s`:
/// http, https e mailto vanno al programma del sistema, il resto si ferma.
fn navigazione_ammessa(url: &tauri::Url) -> bool {
    let host = url.host_str().unwrap_or("");
    let ok = match url.scheme() {
        "tauri" | "a10s" | "about" | "data" | "blob" => true,
        "http" | "https" => matches!(host, "tauri.localhost" | "a10s.localhost" | "ipc.localhost"),
        _ => false,
    };
    if !ok {
        fuori(url);
    }
    ok
}

fn fuori(url: &tauri::Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        eprintln!("[finestra] un indirizzo esterno va al programma del sistema");
        let _ = sistema::apri(url.as_str());
    } else {
        eprintln!("[finestra] navigazione fermata ({})", url.scheme());
    }
}

/// Ferma i processi `WebKitWebProcess` figli di questa app. Quello di rete
/// (`WebKitNetworkProcess`), che salva lo stato dell'interfaccia, chiude da solo.
fn ferma_processo_pagina() {
    let io = std::process::id();
    let Ok(voci) = std::fs::read_dir("/proc") else { return };
    for v in voci.flatten() {
        let Some(pid) = v.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        // «pid (nome) stato ppid …»: il nome può contenere spazi, si legge fino all'ultima «)»
        let (Some(a), Some(b)) = (stat.find('('), stat.rfind(')')) else { continue };
        let nome = &stat[a + 1..b];
        let ppid = stat[b + 1..].split_whitespace().nth(1).and_then(|p| p.parse::<u32>().ok());
        if ppid == Some(io) && nome.starts_with("WebKitWebProces") {
            let ok = std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).status().is_ok_and(|s| s.success());
            eprintln!("[finestra] processo della pagina fermato alla chiusura: {}", if ok { "sì" } else { "no" });
        }
    }
}

/// `--installa` / `--disinstalla`: il file .desktop e le icone nella cartella dell'utente.
fn comandi_di_installazione() -> bool {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let esito = match arg.as_str() {
        "--installa" => std::env::current_exe().and_then(|e| sistema::installa(&e)).map(|f| ("Scritti", f)),
        "--disinstalla" => sistema::disinstalla().map(|f| ("Tolti", f)),
        _ => return false,
    };
    match esito {
        Ok((verbo, file)) => {
            println!("{verbo} {} file:", file.len());
            file.iter().for_each(|f| println!("  {}", f.display()));
        }
        Err(e) => eprintln!("Non riesco: {e}"),
    }
    true
}

type Lavoro = (String, tauri::UriSchemeResponder);
static CODA_ALLEGATI: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Lavoro>>> = std::sync::OnceLock::new();

fn coda_allegati(app: tauri::AppHandle) -> Mutex<std::sync::mpsc::Sender<Lavoro>> {
    let (tx, rx) = std::sync::mpsc::channel::<Lavoro>();
    let rx = std::sync::Arc::new(Mutex::new(rx));
    for n in 0..2 {
        let (app, rx) = (app.clone(), rx.clone());
        std::thread::Builder::new()
            .name(format!("a10s-allegati-{n}"))
            .spawn(move || loop {
                let lavoro = rx.lock().unwrap().recv();
                let Ok((percorso, risposta)) = lavoro else { break };
                risposta.respond(motore_app::servi_allegato(&app, &percorso));
            })
            .expect("thread degli allegati");
    }
    Mutex::new(tx)
}

fn main() {
    if comandi_di_installazione() {
        return;
    }
    let cartella = sistema::cartella_dati();
    let _ = casella::prepara_cartella(&cartella);
    let c = casella::leggi(&cartella);
    let stato = Stato {
        dati: cartella,
        casella: Mutex::new(c.clone()),
        rete: Mutex::new(json!({ "stato": "ok" })),
        sincronia: Mutex::new(None),
    };

    tauri::Builder::default()
        .manage(stato)
        .register_asynchronous_uri_scheme_protocol("a10s", move |ctx, richiesta, risposta| {
            // una coda sola, due lavoratori: niente decine di thread fermi sulla stessa connessione
            let coda = CODA_ALLEGATI.get_or_init(|| coda_allegati(ctx.app_handle().clone()));
            let _ = coda.lock().unwrap().send((richiesta.uri().path().to_string(), risposta));
        })
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title("Ambassadress")
                .inner_size(1400.0, 900.0)
                .min_inner_size(380.0, 500.0)
                .on_navigation(navigazione_ammessa)
                .on_new_window(|url, _| {
                    fuori(&url);
                    tauri::webview::NewWindowResponse::Deny
                })
                .build()?;
            if let Some(c) = c {
                let stato = app.state::<Stato>();
                avvia_sincronia(app.handle(), &stato, c);
            }
            motore_app::avvia_scaricamento(app.handle().clone());
            prova_scriptata(app.handle());
            prova_chiusura(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            avvio,
            dati,
            configura,
            cambia_password,
            togli_casella,
            apri_allegato,
            salva_allegato,
            aggiorna,
            salva_messaggio,
            priorita,
            stato_allegati,
            programma_per,
            salva_testo,
            apri_link,
            notifica,
            diario
        ])
        .build(tauri::generate_context!())
        .expect("avvio dell'app")
        .run(|app, evento| match evento {
            // la finestra sparisce subito; il motore si ferma dietro
            tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::CloseRequested { .. }, .. } => {
                if let Some(w) = app.get_webview_window(&label) {
                    let _ = w.hide();
                }
                // WebKitGTK 2.52 cade mentre chiude il processo che disegna la pagina
                // (free(): corrupted unsorted chunks) e Ubuntu ne scrive un rapporto di 200 MB:
                // la finestra è già nascosta, quel processo si ferma prima di arrivarci
                ferma_processo_pagina();
            }
            tauri::RunEvent::Exit => {
                motore_app::ferma_scaricamento();
                ferma_sincronia(&app.state::<Stato>());
            }
            _ => {}
        });
}
