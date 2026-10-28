//! L'ascolto in IDLE: posta nuova, controllo periodico, IDLE che muore in silenzio.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::{FintoServer, Stato};
use motore::ascolta::{Evento, OpzioniAscolto, ascolta};
use motore::rete::Contatori;

fn opzioni() -> OpzioniAscolto {
    OpzioniAscolto {
        cartella: None,
        controllo: Duration::from_millis(600),
        rinnovo: Duration::from_secs(25 * 60),
        timeout_controllo: Duration::from_millis(400),
        attesa_minima: Duration::from_millis(50),
        attesa_massima: Duration::from_millis(400),
    }
}

/// Raccoglie eventi finché `basta` non dice di fermarsi, o scade il tempo.
fn raccogli(
    rx: &mpsc::Receiver<Evento>,
    entro: Duration,
    basta: impl Fn(&[Evento]) -> bool,
) -> Vec<Evento> {
    let fine = Instant::now() + entro;
    let mut eventi = Vec::new();
    while Instant::now() < fine {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(50)) {
            eventi.push(e);
            if basta(&eventi) {
                break;
            }
        }
    }
    eventi
}

#[test]
fn posta_nuova_e_controllo_periodico() {
    let server = FintoServer::avvia(Stato::gmail(5));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let filo = {
        let (cfg, stop) = (server.config(), stop.clone());
        std::thread::spawn(move || {
            ascolta(cfg, Arc::new(Contatori::default()), opzioni(), stop, tx)
        })
    };
    let eventi = raccogli(&rx, Duration::from_secs(5), |e| {
        e.iter().any(|x| matches!(x, Evento::Connesso { .. }))
    });
    assert!(
        matches!(eventi.last(), Some(Evento::Connesso { connessione: 1, .. })),
        "{eventi:?}"
    );

    server.con(|s| {
        s.casella.aggiungi();
    });
    let eventi = raccogli(&rx, Duration::from_secs(5), |e| {
        e.iter().any(|x| matches!(x, Evento::Novita { .. }))
            && e.iter().any(|x| matches!(x, Evento::Controllo { .. }))
    });
    assert!(
        eventi.iter().any(|x| matches!(x, Evento::Novita { .. })),
        "{eventi:?}"
    );
    assert!(
        eventi.iter().any(|x| matches!(x, Evento::Controllo { .. })),
        "{eventi:?}"
    );
    assert!(
        !eventi.iter().any(|x| matches!(x, Evento::Caduto { .. })),
        "{eventi:?}"
    );

    stop.store(true, Ordering::Relaxed);
    filo.join().unwrap();
    let comandi = server.comandi();
    assert!(comandi.iter().any(|c| c.ends_with(" IDLE")));
    assert!(comandi.iter().any(|c| c == "DONE"));
    assert!(comandi.iter().any(|c| c.ends_with(" NOOP")));
    assert!(comandi.iter().any(|c| c.contains("EXAMINE")));
    assert!(
        !comandi
            .iter()
            .any(|c| c.to_ascii_uppercase().contains("SELECT "))
    );
}

#[test]
fn idle_che_muore_in_silenzio() {
    let mut stato = Stato::gmail(5);
    stato.guasti.idle_muto = 1;
    let server = FintoServer::avvia(stato);
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let filo = {
        let (cfg, stop) = (server.config(), stop.clone());
        std::thread::spawn(move || {
            ascolta(cfg, Arc::new(Contatori::default()), opzioni(), stop, tx)
        })
    };
    // Prima connessione: IDLE muto → al controllo DONE non ha risposta → caduta → riconnessione.
    let eventi = raccogli(&rx, Duration::from_secs(8), |e| {
        e.iter()
            .any(|x| matches!(x, Evento::Connesso { connessione: 2, .. }))
    });
    let inizio = eventi
        .iter()
        .position(|x| matches!(x, Evento::Connesso { connessione: 1, .. }));
    let caduta = eventi
        .iter()
        .position(|x| matches!(x, Evento::Caduto { .. }));
    let di_nuovo = eventi
        .iter()
        .position(|x| matches!(x, Evento::Connesso { connessione: 2, .. }));
    assert!(
        inizio.is_some() && caduta.is_some() && di_nuovo.is_some(),
        "{eventi:?}"
    );
    assert!(inizio < caduta && caduta < di_nuovo, "{eventi:?}");
    if let Some(Evento::Caduto { motivo, .. }) = caduta.map(|i| &eventi[i]) {
        assert!(motivo.contains("non risponde"), "{motivo}");
    }

    // La seconda connessione funziona: arriva la posta nuova.
    server.con(|s| {
        s.casella.aggiungi();
    });
    let eventi = raccogli(&rx, Duration::from_secs(5), |e| {
        e.iter().any(|x| matches!(x, Evento::Novita { .. }))
    });
    assert!(
        eventi.iter().any(|x| matches!(x, Evento::Novita { .. })),
        "{eventi:?}"
    );

    stop.store(true, Ordering::Relaxed);
    filo.join().unwrap();
    assert_eq!(server.con(|s| s.connessioni), 2);
}

#[test]
fn attese_crescenti_con_un_massimo() {
    use motore::sincronizza::Attese;
    let mut a = Attese::new(Duration::from_millis(100), Duration::from_millis(800));
    let v: Vec<u128> = (0..6).map(|_| a.prossima().as_millis()).collect();
    // 100, 200, 400, 800, 800, 800 più una variazione di al massimo un quinto.
    for (d, atteso) in v.iter().zip([100u128, 200, 400, 800, 800, 800]) {
        assert!(*d >= atteso && *d <= atteso + atteso / 5 + 1, "{v:?}");
    }
    a.azzera();
    assert!(a.prossima().as_millis() < 130);
}
