//! `motore`: riga di comando del banco di prova. Stampa solo conteggi, tempi,
//! estensioni del server ed errori: mai oggetti, indirizzi, nomi o testi.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use motore::ascolta::{self, Evento, OpzioniAscolto};
use motore::credenziali;
use motore::db::{self, Db};
use motore::errore::Errore;
use motore::rete::{Contatori, Indirizzo, Sicurezza};
use motore::scaricamento;
use motore::sincronizza::{self, Collegamento, Config, Opzioni, Rapporto};
use motore::tempo::{byte, data_utc, durata, numero, ora_utc};

const AIUTO: &str = "\
motore: il banco di prova del motore di Ambassadress (sola lettura IMAP)

Uso:
  motore sincronizza [opzioni]   scarica e aggiorna la casella nel database locale
  motore stato [--db FILE]       conteggi del database locale
  motore cerca PAROLE [--db FILE] quanti messaggi trova la ricerca (solo il numero)
  motore allegati [--db FILE]    totali degli allegati (numero e peso, per tipo e per anno)
  motore ascolta [opzioni]       resta in ascolto (IDLE) e sincronizza quando arriva posta
  motore aiuto

Opzioni di sincronizza e ascolta:
  --limite N           tiene sincronizzati gli N messaggi più recenti (predefinito 1000)
  --tutti              tutti i messaggi della cartella
  --corpi N            scarica il testo degli N messaggi più recenti (predefinito 100, 0 per nessuno)
  --cartella NOME      cartella IMAP (predefinita: quella con \\All, cioè «Tutti i messaggi»)
  --lotto N            messaggi per ogni FETCH di intestazioni (predefinito 200)
  --server HOST[:PORTA] predefinito imap.gmail.com:993
  --credenziali FILE   file di due righe, utente e password (predefinito ~/.secrets/gmail-*.txt)
  --timeout SECONDI    tempo massimo per ogni lettura dal server (predefinito 60)
  --db FILE            database (predefinito motore/private/ambassadress.sqlite)
  --in-chiaro          niente TLS: solo verso 127.0.0.1, per le prove

Solo per ascolta:
  --controllo SECONDI  ogni quanto verificare che la connessione IDLE sia viva (predefinito 240)
  --durata SECONDI     si ferma da solo dopo questo tempo (predefinito: mai, Ctrl+C per uscire)
";

struct Argomenti {
    comando: String,
    parole: Vec<String>,
    limite: Option<usize>,
    corpi: usize,
    cartella: Option<String>,
    lotto: usize,
    server: Indirizzo,
    credenziali: Option<PathBuf>,
    timeout: Duration,
    db: PathBuf,
    controllo: Duration,
    durata: Option<Duration>,
}

fn numero_da(v: Option<String>, nome: &str) -> Result<u64, Errore> {
    v.ok_or_else(|| Errore::Uso(format!("{nome} vuole un valore")))?
        .parse()
        .map_err(|_| Errore::Uso(format!("{nome} vuole un numero")))
}

fn leggi_argomenti() -> Result<Argomenti, Errore> {
    let mut it = std::env::args().skip(1);
    let comando = it.next().unwrap_or_else(|| "aiuto".into());
    let mut a = Argomenti {
        comando,
        parole: Vec::new(),
        limite: Some(1000),
        corpi: 100,
        cartella: None,
        lotto: 200,
        server: Indirizzo::gmail(),
        credenziali: None,
        timeout: Duration::from_secs(60),
        db: db::percorso_predefinito(),
        controllo: Duration::from_secs(240),
        durata: None,
    };
    let mut in_chiaro = false;
    while let Some(x) = it.next() {
        match x.as_str() {
            "--limite" => a.limite = Some(numero_da(it.next(), "--limite")? as usize),
            "--tutti" => a.limite = None,
            "--corpi" => a.corpi = numero_da(it.next(), "--corpi")? as usize,
            "--cartella" => a.cartella = it.next(),
            "--lotto" => a.lotto = numero_da(it.next(), "--lotto")?.max(1) as usize,
            "--server" => {
                let v = it
                    .next()
                    .ok_or_else(|| Errore::Uso("--server vuole un valore".into()))?;
                match v.rsplit_once(':') {
                    Some((h, p)) => {
                        a.server.host = h.to_string();
                        a.server.porta = p
                            .parse()
                            .map_err(|_| Errore::Uso("porta non valida".into()))?;
                    }
                    None => a.server.host = v,
                }
            }
            "--credenziali" => a.credenziali = it.next().map(PathBuf::from),
            "--timeout" => {
                a.timeout = Duration::from_secs(numero_da(it.next(), "--timeout")?.max(1))
            }
            "--db" => {
                a.db = it
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| Errore::Uso("--db vuole un file".into()))?
            }
            "--controllo" => {
                a.controllo = Duration::from_secs(numero_da(it.next(), "--controllo")?.max(1))
            }
            "--durata" => a.durata = Some(Duration::from_secs(numero_da(it.next(), "--durata")?)),
            "--in-chiaro" => in_chiaro = true,
            altro if altro.starts_with("--") => {
                return Err(Errore::Uso(format!("opzione sconosciuta: {altro}")));
            }
            altro => a.parole.push(altro.to_string()),
        }
    }
    if in_chiaro {
        if a.server.host != "127.0.0.1" && a.server.host != "localhost" && a.server.host != "::1" {
            return Err(Errore::Uso(
                "--in-chiaro è ammesso solo verso 127.0.0.1".into(),
            ));
        }
        a.server.sicurezza = Sicurezza::InChiaro;
    }
    Ok(a)
}

fn main() -> ExitCode {
    let a = match leggi_argomenti() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{AIUTO}");
            return ExitCode::from(2);
        }
    };
    let esito = match a.comando.as_str() {
        "sincronizza" | "sync" => comando_sincronizza(&a),
        "stato" => comando_stato(&a),
        "cerca" => comando_cerca(&a),
        "allegati" => comando_allegati(&a),
        "ascolta" => comando_ascolta(&a),
        "aiuto" | "--help" | "-h" | "help" => {
            print!("{AIUTO}");
            Ok(())
        }
        altro => Err(Errore::Uso(format!(
            "comando sconosciuto: {altro}\n\n{AIUTO}"
        ))),
    };
    match esito {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Errore: {e}");
            ExitCode::FAILURE
        }
    }
}

fn config(a: &Argomenti) -> Result<Config, Errore> {
    Ok(Config {
        indirizzo: a.server.clone(),
        credenziali: credenziali::leggi(a.credenziali.as_deref())?,
        timeout: a.timeout,
    })
}

fn opzioni(a: &Argomenti, avanzamento: bool) -> Opzioni {
    Opzioni {
        limite: a.limite,
        corpi: a.corpi,
        cartella: a.cartella.clone(),
        lotto: a.lotto,
        mostra_avanzamento: avanzamento,
        ..Opzioni::default()
    }
}

/// Dimensione del database, compreso il file WAL.
fn dimensione_db(db: &Db) -> u64 {
    let base = std::fs::metadata(&db.percorso)
        .map(|m| m.len())
        .unwrap_or(0);
    let mut wal = db.percorso.clone().into_os_string();
    wal.push("-wal");
    base + std::fs::metadata(PathBuf::from(wal))
        .map(|m| m.len())
        .unwrap_or(0)
}

fn si_no(v: bool) -> &'static str {
    if v { "sì" } else { "no" }
}

fn stampa_rapporto(r: &Rapporto, db: &Db) {
    println!(
        "Ambassadress · motore {} · sincronizzazione in sola lettura",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!(
        "Server        {}{}",
        r.server,
        r.tls
            .as_deref()
            .map(|t| format!(", {t}"))
            .unwrap_or_default()
    );
    if !r.capacita.is_empty() {
        let ha = |c: &str| r.capacita.iter().any(|x| x.eq_ignore_ascii_case(c));
        let compress = r
            .capacita
            .iter()
            .any(|x| x.to_ascii_uppercase().starts_with("COMPRESS="));
        println!(
            "Estensioni    CONDSTORE {} · QRESYNC {} · IDLE {} · COMPRESS {} (non usata) · X-GM-EXT-1 {} · ENABLE {} · ID {}",
            si_no(ha("CONDSTORE")),
            si_no(ha("QRESYNC")),
            si_no(ha("IDLE")),
            si_no(compress),
            si_no(ha("X-GM-EXT-1")),
            si_no(ha("ENABLE")),
            si_no(ha("ID")),
        );
        println!(
            "Abilitate     {}",
            if r.abilitate.is_empty() {
                "nessuna".into()
            } else {
                r.abilitate.join(", ")
            }
        );
        println!("Tutte         {}", r.capacita.join(" "));
    }
    if !r.cartella.is_empty() {
        println!(
            "Cartella      {} ({}): {} messaggi sul server",
            r.cartella,
            r.ruolo_cartella,
            numero(r.sul_server as u64)
        );
        if let Some(f) = &r.permanentflags {
            println!("Flag permanenti dichiarati con EXAMINE: ({})", f.join(" "));
        }
        if r.invariata {
            println!(
                "Differenze    nessuna (UIDNEXT, numero di messaggi e HIGHESTMODSEQ uguali a prima)"
            );
        } else {
            println!(
                "Finestra      {} più recenti: {} già presenti, {} da scaricare{}",
                numero(r.finestra as u64),
                numero(r.gia_presenti as u64),
                numero(r.da_scaricare as u64),
                if r.abbandonati > 0 {
                    format!(", {} lasciati stare dopo troppi tentativi", r.abbandonati)
                } else {
                    String::new()
                }
            );
        }
    }
    println!(
        "Messaggi      nuovi {} · aggiornati {} · spariti {} · ricollegati {}",
        numero(r.nuovi as u64),
        numero(r.aggiornati as u64),
        numero(r.spariti as u64),
        numero(r.ricollegati as u64)
    );
    if r.uidvalidity_cambiata {
        println!(
            "UIDVALIDITY   cambiata: {} messaggi ricollegati ai nuovi UID senza riscaricarli",
            numero(r.rimappati as u64)
        );
    }
    println!(
        "Problemi      malformati {} · da riprovare {} · senza struttura {} · NO dal server {}",
        r.malformati, r.da_riprovare, r.senza_struttura, r.no_dal_server
    );
    println!(
        "Flag          controllati con: {}",
        if r.modo_flag.is_empty() {
            "-"
        } else {
            r.modo_flag
        }
    );
    println!(
        "Testi         {} scaricati (troncati {}) · allegati registrati dalla struttura {}",
        numero(r.corpi as u64),
        r.corpi_troncati,
        numero(r.allegati as u64)
    );
    let t = &r.tempi;
    println!(
        "Tempi         connessione {} · accesso {} · cartelle {} · examine {}{} · elenco UID {} · flag {} · intestazioni {} · testi {} · totale {}",
        durata(t.connessione),
        durata(t.accesso),
        durata(t.cartelle),
        durata(t.examine),
        if r.uidvalidity_cambiata {
            format!(" · riallineamento {}", durata(t.rimappa))
        } else {
            String::new()
        },
        durata(t.elenco_uid),
        durata(t.flag),
        durata(t.intestazioni),
        durata(t.corpi),
        durata(r.durata)
    );
    if r.nuovi > 0 && !t.intestazioni.is_zero() {
        println!(
            "Velocità      {:.0} messaggi/s per le intestazioni",
            r.nuovi as f64 / t.intestazioni.as_secs_f64()
        );
    }
    println!(
        "Rete          letti {} (intestazioni e struttura {}, testi {}) · scritti {} · riconnessioni {}",
        byte(r.byte_letti),
        byte(r.byte_intestazioni),
        byte(r.byte_testi),
        byte(r.byte_scritti),
        r.riconnessioni
    );
    if r.nuovi > 0 && r.byte_intestazioni > 0 {
        println!(
            "Per messaggio {} di intestazioni e struttura in media",
            byte(r.byte_intestazioni / r.nuovi as u64)
        );
    }
    for e in &r.errori_rete {
        println!("              ripreso dopo: {e}");
    }
    if let Ok(n) = db.conta("SELECT count(*) FROM messaggi") {
        println!(
            "Database      {} messaggi in tutto · {}",
            numero(n as u64),
            byte(dimensione_db(db))
        );
    }
}

fn comando_sincronizza(a: &Argomenti) -> Result<(), Errore> {
    let cfg = config(a)?;
    let mut db = Db::apri(&a.db)?;
    let contatori = Arc::new(Contatori::default());
    let mut col = Collegamento::new(cfg, contatori);
    eprintln!("Sincronizzo in sola lettura…");
    let r = sincronizza::sincronizza(&mut col, &mut db, &opzioni(a, true));
    col.chiudi();
    stampa_rapporto(&r, &db);
    match r.errore {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn comando_stato(a: &Argomenti) -> Result<(), Errore> {
    if !a.db.exists() {
        println!("Il database non c'è ancora: lancia prima `motore sincronizza`.");
        return Ok(());
    }
    let db = Db::apri(&a.db)?;
    let c = |sql: &str| db.conta(sql).map(|n| numero(n.max(0) as u64));
    println!(
        "Database      {} ({})",
        db.percorso.display(),
        byte(dimensione_db(&db))
    );
    println!(
        "Messaggi      {} · eliminati altrove {}",
        c("SELECT count(*) FROM messaggi")?,
        c("SELECT count(*) FROM messaggi WHERE eliminato IS NOT NULL")?
    );
    println!(
        "Stato         da leggere {} · importanti {} · risposti {} · bozze {} · automatici (newsletter, notifiche) {}",
        c("SELECT count(*) FROM messaggi WHERE letto = 0 AND eliminato IS NULL")?,
        c("SELECT count(*) FROM messaggi WHERE importante = 1")?,
        c("SELECT count(*) FROM messaggi WHERE risposto = 1")?,
        c("SELECT count(*) FROM messaggi WHERE bozza = 1")?,
        c("SELECT count(*) FROM messaggi WHERE automatico = 1")?
    );
    println!(
        "Thread        {} · con più di un messaggio {}",
        c("SELECT count(*) FROM thread")?,
        c(
            "SELECT count(*) FROM (SELECT thread_id FROM messaggi GROUP BY thread_id HAVING count(*) > 1)"
        )?
    );
    println!(
        "Persone       {} indirizzi · {} mittenti diversi",
        c("SELECT count(*) FROM indirizzi")?,
        c("SELECT count(DISTINCT indirizzo_id) FROM partecipanti WHERE ruolo = 'da'")?
    );
    println!(
        "Etichette     {} diverse · {} assegnazioni",
        c("SELECT count(DISTINCT nome) FROM etichette")?,
        c("SELECT count(*) FROM etichette")?
    );
    println!(
        "Allegati      {} (in {} messaggi) · {} immagini incorporate · {} di struttura",
        c("SELECT count(*) FROM parti WHERE ruolo = 'allegato'")?,
        c("SELECT count(DISTINCT messaggio_id) FROM parti WHERE ruolo = 'allegato'")?,
        c("SELECT count(*) FROM parti WHERE ruolo = 'incorporato'")?,
        byte(
            db.conta("SELECT IFNULL(sum(byte), 0) FROM parti WHERE ruolo = 'allegato'")?
                .max(0) as u64
        )
    );
    println!(
        "Testi         scaricati {} · troncati {} · da scaricare {} · senza parti di testo {} · senza struttura {}",
        c("SELECT count(*) FROM messaggi WHERE corpo = 1")?,
        c("SELECT count(*) FROM corpi WHERE troncato = 1")?,
        c("SELECT count(*) FROM messaggi WHERE corpo = 0")?,
        c("SELECT count(*) FROM messaggi WHERE corpo = 2")?,
        c("SELECT count(*) FROM messaggi WHERE struttura = 0")?
    );
    println!(
        "Ricerca       {} righe nell'indice FTS5 · {} con testo",
        c("SELECT count(*) FROM ricerca")?,
        c("SELECT count(*) FROM ricerca WHERE length(testo) > 0")?
    );
    println!("Da riprovare  {} UID", c("SELECT count(*) FROM mancanti")?);
    for (nome, ruolo, locali, esistenti, modseq, ultimo) in db.cartelle()? {
        println!(
            "Cartella      {} ({}): {} qui, {} sul server all'ultimo giro · HIGHESTMODSEQ {} · ultimo giro {}",
            nome,
            ruolo.unwrap_or_default(),
            numero(locali as u64),
            esistenti.map(|e| numero(e as u64)).unwrap_or("-".into()),
            if modseq.is_some() { "salvato" } else { "-" },
            ultimo.map(data_utc).unwrap_or("-".into())
        );
    }
    if let Some(cap) = db.meta("capacita")? {
        println!("Estensioni    {cap}");
    }
    let ultime = db.ultime_sincronizzazioni(5)?;
    if !ultime.is_empty() {
        println!("Ultimi giri");
        for (inizio, ms, esito, nuovi, aggiornati, spariti, letti) in ultime {
            println!(
                "  {} · {} · nuovi {} · aggiornati {} · spariti {} · letti {} · {}",
                data_utc(inizio),
                durata(Duration::from_millis(ms.max(0) as u64)),
                nuovi,
                aggiornati,
                spariti,
                byte(letti.max(0) as u64),
                esito
            );
        }
    }
    Ok(())
}

fn comando_cerca(a: &Argomenti) -> Result<(), Errore> {
    if a.parole.is_empty() {
        return Err(Errore::Uso("cerca vuole almeno una parola".into()));
    }
    let db = Db::apri(&a.db)?;
    let t = Instant::now();
    let n = db.cerca(&a.parole.join(" "))?;
    println!(
        "{} messaggi trovati in {}",
        numero(n.max(0) as u64),
        durata(t.elapsed())
    );
    Ok(())
}

/// `61 · 4.2 MB · scaricati 10 (1.1 MB) · da scaricare 51 (3.1 MB)`
fn riga_conto(c: &scaricamento::Conto) -> String {
    format!(
        "{} · {} · scaricati {} ({}) · da scaricare {} ({})",
        numero(c.numero()),
        byte(c.byte()),
        numero(c.scaricati),
        byte(c.byte_scaricati),
        numero(c.da_scaricare),
        byte(c.byte_da_scaricare)
    )
}

/// Solo i totali degli allegati: numeri, tipi e anni, mai nomi, oggetti o indirizzi.
fn comando_allegati(a: &Argomenti) -> Result<(), Errore> {
    if !a.db.exists() {
        println!("Il database non c'è ancora: lancia prima `motore sincronizza`.");
        return Ok(());
    }
    let db = Db::apri(&a.db)?;
    let t = scaricamento::totali(&db)?;
    println!("Allegati      {}", riga_conto(&t.tutti));
    println!("Per tipo");
    for (c, conto) in &t.per_tipo {
        println!("  {:<11} {}", c.nome(), riga_conto(conto));
    }
    println!("Per anno");
    for (anno, conto) in &t.per_anno {
        let anno = if *anno == 0 {
            "senza data".to_string()
        } else {
            anno.to_string()
        };
        println!("  {anno:<11} {}", riga_conto(conto));
    }
    let q = scaricamento::quota(&db, scaricamento::QUOTA_CONSIGLIATA)?;
    println!(
        "Archivio      {} su disco (lo stesso contenuto una volta sola) · in attesa di riprovare {}",
        byte(t.archivio),
        numero(t.in_attesa)
    );
    println!(
        "Oggi          {} scaricati dal server, quota {} (giorno UTC {})",
        byte(q.scaricati),
        byte(q.limite),
        q.giorno
    );
    Ok(())
}

fn riga_breve(r: &Rapporto) -> String {
    format!(
        "nuovi {} · aggiornati {} · spariti {} · testi {} · {} · letti {}",
        r.nuovi,
        r.aggiornati,
        r.spariti,
        r.corpi,
        durata(r.durata),
        byte(r.byte_letti)
    )
}

fn comando_ascolta(a: &Argomenti) -> Result<(), Errore> {
    let cfg = config(a)?;
    let mut db = Db::apri(&a.db)?;
    let contatori = Arc::new(Contatori::default());
    let opz = opzioni(a, false);
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let opz_ascolto = OpzioniAscolto {
        cartella: a.cartella.clone(),
        controllo: a.controllo,
        ..Default::default()
    };
    let filo = {
        let (cfg, contatori, stop) = (cfg.clone(), contatori.clone(), stop.clone());
        std::thread::spawn(move || ascolta::ascolta(cfg, contatori, opz_ascolto, stop, tx))
    };
    let mut col = Collegamento::new(cfg, contatori);
    let inizio = Instant::now();
    println!(
        "[{}] ascolto in sola lettura · controllo ogni {} · {}",
        ora_utc(),
        durata(a.controllo),
        a.durata
            .map(|d| format!("mi fermo dopo {}", durata(d)))
            .unwrap_or_else(|| "Ctrl+C per uscire".into())
    );
    let r = sincronizza::sincronizza(&mut col, &mut db, &opz);
    match &r.errore {
        None => println!(
            "[{}] sincronizzazione iniziale: {}",
            ora_utc(),
            riga_breve(&r)
        ),
        Some(e) => println!(
            "[{}] sincronizzazione iniziale non riuscita: {e}",
            ora_utc()
        ),
    }
    loop {
        if a.durata.is_some_and(|d| inizio.elapsed() >= d) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Evento::Novita { quante }) => {
                // Raccoglie le novità arrivate insieme e sincronizza una volta sola.
                let mut totale = quante;
                while let Ok(Evento::Novita { quante }) =
                    rx.recv_timeout(Duration::from_millis(300))
                {
                    totale += quante;
                }
                println!("[{}] il server segnala {} novità", ora_utc(), totale);
                let r = sincronizza::sincronizza(&mut col, &mut db, &opz);
                match &r.errore {
                    None => println!("[{}] sincronizzato: {}", ora_utc(), riga_breve(&r)),
                    Some(e) => println!("[{}] sincronizzazione non riuscita: {e}", ora_utc()),
                }
            }
            Ok(Evento::Connesso {
                cartella,
                connessione,
            }) => {
                let quale = if connessione == 1 {
                    "connesso".to_string()
                } else {
                    format!("riconnesso ({connessione}ª connessione)")
                };
                println!("[{}] IDLE {quale} su {cartella}", ora_utc());
            }
            Ok(Evento::Controllo { durata: d }) => {
                println!(
                    "[{}] controllo: connessione viva (DONE + NOOP in {})",
                    ora_utc(),
                    durata(d)
                );
            }
            Ok(Evento::Caduto { motivo, attesa }) => {
                println!(
                    "[{}] connessione IDLE persa: {motivo}; riprovo tra {}",
                    ora_utc(),
                    durata(attesa)
                );
            }
            Ok(Evento::Fermato { motivo }) => {
                println!("[{}] ascolto fermato: {motivo}", ora_utc());
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    stop.store(true, Ordering::Relaxed);
    let _ = filo.join();
    col.chiudi();
    println!(
        "[{}] fine dell'ascolto dopo {} · letti in tutto {} · scritti {}",
        ora_utc(),
        durata(inizio.elapsed()),
        byte(col.contatori().letti()),
        byte(col.contatori().scritti())
    );
    Ok(())
}
