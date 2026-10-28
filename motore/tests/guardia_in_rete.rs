//! La guardia della sola lettura sulla connessione vera: i comandi vietati non
//! arrivano mai al server, e la connessione resta usabile.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{FintoServer, PASSWORD, Stato, UTENTE};
use motore::errore::Errore;
use motore::imap::Sessione;
use motore::rete::Contatori;

#[test]
fn i_comandi_vietati_non_escono() {
    let server = FintoServer::avvia(Stato::gmail(3));
    let cfg = server.config();
    let mut s = Sessione::connetti(
        &cfg.indirizzo,
        Duration::from_secs(2),
        Arc::new(Contatori::default()),
    )
    .unwrap();
    s.login(UTENTE, PASSWORD).unwrap();
    server.svuota_comandi();

    let vietati = [
        "SELECT INBOX",
        "STORE 1 +FLAGS (\\Seen)",
        "UID STORE 1 +FLAGS (\\Deleted)",
        "EXPUNGE",
        "UID EXPUNGE 1:*",
        "COPY 1 Cestino",
        "UID MOVE 1 Cestino",
        "APPEND INBOX (\\Seen) {3}",
        "DELETE INBOX",
        "CREATE Nuova",
        "RENAME INBOX Altro",
        "SUBSCRIBE INBOX",
        "UID FETCH 1 (BODY[])",
        "FETCH 1 (BODY[TEXT])",
        "FETCH 1 RFC822",
        "FETCH 1 RFC822.TEXT",
        "NOOP\r\na9 EXPUNGE",
    ];
    for c in vietati {
        match s.esegui(c, |_| Ok(())) {
            Err(Errore::Rifiutato(_)) => {}
            altro => panic!("{c}: atteso un rifiuto della guardia, arrivato {altro:?}"),
        }
    }
    assert!(
        server.comandi().is_empty(),
        "nessun comando vietato è arrivato al server"
    );

    // La connessione è ancora in ordine.
    s.noop().unwrap();
    s.esegui(
        "UID FETCH 1 (UID FLAGS BODY.PEEK[HEADER.FIELDS (SUBJECT)])",
        |_| Ok(()),
    )
    .unwrap();
    let comandi = server.comandi();
    assert_eq!(comandi.len(), 2);
    assert!(comandi[0].ends_with("NOOP"));
    s.logout();
}

#[test]
fn done_solo_durante_idle() {
    let server = FintoServer::avvia(Stato::gmail(3));
    let cfg = server.config();
    let mut s = Sessione::connetti(
        &cfg.indirizzo,
        Duration::from_secs(2),
        Arc::new(Contatori::default()),
    )
    .unwrap();
    s.login(UTENTE, PASSWORD).unwrap();
    s.examine(
        "[Gmail]/Tutti i messaggi",
        motore::imap::ModoExamine::Semplice,
    )
    .unwrap();
    // DONE fuori da IDLE viene fermato.
    assert!(matches!(
        s.idle_termina("a99", Duration::from_millis(100)),
        Err(Errore::Rifiutato(_))
    ));
    let (etichetta, _) = s.idle_inizia().unwrap();
    s.idle_termina(&etichetta, Duration::from_secs(2)).unwrap();
    s.noop().unwrap();
    s.logout();
}
