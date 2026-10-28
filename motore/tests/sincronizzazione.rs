//! Sincronizzazione contro il finto server: prima volta, differenze, QRESYNC,
//! UIDVALIDITY che cambia, connessione che cade a metà, risposte malformate.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{FintoServer, Stato, opzioni_prova};
use motore::db::Db;
use motore::rete::Contatori;
use motore::sincronizza::{Collegamento, Opzioni, Rapporto, sincronizza};

struct Banco {
    server: FintoServer,
    col: Collegamento,
    db: Db,
    _dir: CartellaTemporanea,
}

/// Una cartella temporanea per il database, cancellata alla fine della prova.
struct CartellaTemporanea(PathBuf);

impl CartellaTemporanea {
    fn nuova(nome: &str) -> Self {
        let p = std::env::temp_dir().join(format!("motore-prova-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        CartellaTemporanea(p)
    }
}

impl Drop for CartellaTemporanea {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn banco(nome: &str, stato: Stato) -> Banco {
    let server = FintoServer::avvia(stato);
    let dir = CartellaTemporanea::nuova(nome);
    let db = Db::apri(&dir.0.join("prova.sqlite")).unwrap();
    let col = Collegamento::new(server.config(), Arc::new(Contatori::default()));
    Banco {
        server,
        col,
        db,
        _dir: dir,
    }
}

impl Banco {
    fn giro(&mut self, opz: &Opzioni) -> Rapporto {
        let r = sincronizza(&mut self.col, &mut self.db, opz);
        if let Some(e) = &r.errore {
            panic!("sincronizzazione non riuscita: {e}");
        }
        r
    }

    fn conta(&self, sql: &str) -> i64 {
        self.db.conta(sql).unwrap()
    }
}

/// Nessun comando ricevuto dal server può cambiare qualcosa.
fn solo_lettura(comandi: &[String]) {
    let vietati = [
        "SELECT ",
        "STORE",
        "APPEND",
        "COPY",
        "MOVE",
        "EXPUNGE",
        "DELETE",
        "CREATE",
        "RENAME",
        "SUBSCRIBE",
        "CLOSE",
    ];
    for c in comandi {
        let su = c.to_ascii_uppercase();
        let dopo_etichetta = su.split_once(' ').map(|x| x.1).unwrap_or(&su);
        for v in vietati {
            assert!(
                !dopo_etichetta.starts_with(v) && !dopo_etichetta.starts_with(&format!("UID {v}")),
                "comando vietato: {c}"
            );
        }
        assert!(!su.contains("BODY["), "BODY senza PEEK: {c}");
        assert!(
            !su.contains("RFC822.TEXT") && !su.contains(" RFC822 ") && !su.contains("(RFC822)"),
            "RFC822: {c}"
        );
    }
}

#[test]
fn prima_sincronizzazione() {
    let mut b = banco("prima", Stato::gmail(30));
    let r = b.giro(&opzioni_prova());

    assert_eq!(r.sul_server, 30);
    assert_eq!(r.cartella, "[Gmail]/Tutti i messaggi");
    assert_eq!(r.nuovi, 20, "la finestra è di 20 messaggi");
    assert_eq!(r.malformati, 0);
    assert_eq!(r.corpi, 5);
    assert!(r.capacita.iter().any(|c| c == "X-GM-EXT-1"));
    assert!(r.abilitate.iter().any(|c| c == "CONDSTORE"));

    assert_eq!(b.conta("SELECT count(*) FROM messaggi"), 20);
    assert_eq!(b.conta("SELECT count(*) FROM posizioni"), 20);
    // I più recenti: UID 11..30.
    assert_eq!(b.conta("SELECT min(uid) FROM posizioni"), 11);
    assert_eq!(b.conta("SELECT count(*) FROM corpi"), 5);
    assert_eq!(
        b.conta(
            "SELECT min(p.uid) FROM corpi c JOIN posizioni p ON p.messaggio_id = c.messaggio_id"
        ),
        26
    );
    // Allegati dalla struttura: ogni terzo messaggio fra 11 e 30 (12, 15, …, 30).
    assert_eq!(
        b.conta("SELECT count(*) FROM parti WHERE ruolo = 'allegato'"),
        7
    );
    // Thread di Gmail: (n+1)/2 → 10 thread per i messaggi 11..30.
    assert_eq!(b.conta("SELECT count(*) FROM thread"), 10);
    assert_eq!(b.conta("SELECT count(*) FROM indirizzi"), 6);
    assert_eq!(b.conta("SELECT count(DISTINCT nome) FROM etichette"), 2);
    assert_eq!(
        b.conta("SELECT count(*) FROM etichette WHERE nome = 'Prove à'"),
        20
    );
    // Ricerca: nell'oggetto e nel testo scaricato (quoted-printable decodificato).
    assert_eq!(b.db.cerca("preventivo").unwrap(), 5);
    assert_eq!(b.db.cerca("parolarara30").unwrap(), 1);
    assert_eq!(b.db.cerca("perche").unwrap(), 5);
    assert_eq!(
        b.db.cerca("parolarara11").unwrap(),
        0,
        "il testo dei più vecchi non si scarica"
    );

    let comandi = b.server.comandi();
    solo_lettura(&comandi);
    assert!(
        comandi
            .iter()
            .any(|c| c.contains("EXAMINE \"[Gmail]/Tutti i messaggi\" (CONDSTORE)"))
    );
    assert!(
        comandi
            .iter()
            .any(|c| c.contains("BODY.PEEK[HEADER.FIELDS"))
    );
    assert!(comandi.iter().any(|c| c.contains("BODY.PEEK[1]<0.")));
}

#[test]
fn differenze_con_condstore() {
    let mut b = banco("differenze", Stato::gmail(30));
    b.giro(&opzioni_prova());
    b.server.svuota_comandi();

    // Sul server: 3 nuovi, 2 flag cambiati, 1 eliminato.
    b.server.con(|s| {
        s.casella.aggiungi();
        s.casella.aggiungi();
        s.casella.aggiungi();
        s.casella.cambia_flag(25, &["\\Seen", "\\Flagged"]);
        s.casella.cambia_flag(26, &[]);
        s.casella.rimuovi(20);
    });
    let r = b.giro(&opzioni_prova());
    assert_eq!(r.nuovi, 3);
    assert_eq!(r.aggiornati, 2);
    assert_eq!(r.spariti, 1);
    assert_eq!(r.modo_flag, "CONDSTORE");
    assert_eq!(
        b.conta("SELECT count(*) FROM messaggi WHERE eliminato IS NOT NULL"),
        1
    );
    assert_eq!(
        b.conta("SELECT importante FROM messaggi WHERE gm_msgid = 1000025"),
        1
    );
    assert_eq!(
        b.conta("SELECT letto FROM messaggi WHERE gm_msgid = 1000026"),
        0
    );
    let comandi = b.server.comandi();
    solo_lettura(&comandi);
    assert!(comandi.iter().any(|c| c.contains("(CHANGEDSINCE ")));
    // I messaggi già scaricati non si riscaricano.
    let intestazioni: Vec<_> = comandi
        .iter()
        .filter(|c| c.contains("BODYSTRUCTURE"))
        .collect();
    assert_eq!(intestazioni.len(), 1);
    assert!(
        intestazioni[0].contains("UID FETCH 31:33 "),
        "{}",
        intestazioni[0]
    );

    // Terzo giro senza novità: niente elenco degli UID, niente FETCH.
    b.server.svuota_comandi();
    let r = b.giro(&opzioni_prova());
    assert!(r.invariata);
    assert_eq!(r.nuovi + r.aggiornati + r.spariti, 0);
    let comandi = b.server.comandi();
    assert!(
        !comandi
            .iter()
            .any(|c| c.contains("SEARCH") || c.contains("FETCH")),
        "{comandi:?}"
    );
}

#[test]
fn differenze_con_qresync() {
    let mut stato = Stato::gmail(30);
    stato.qresync = true;
    let mut b = banco("qresync", stato);
    let r = b.giro(&opzioni_prova());
    assert!(r.abilitate.iter().any(|c| c == "QRESYNC"));
    b.server.svuota_comandi();
    b.server.con(|s| {
        s.casella.aggiungi();
        s.casella.cambia_flag(28, &["\\Answered"]);
        s.casella.rimuovi(15);
    });
    let r = b.giro(&opzioni_prova());
    assert_eq!(r.modo_flag, "QRESYNC");
    assert_eq!(r.nuovi, 1);
    assert_eq!(r.aggiornati, 1);
    assert_eq!(r.spariti, 1);
    assert_eq!(
        b.conta("SELECT risposto FROM messaggi WHERE gm_msgid = 1000028"),
        1
    );
    let comandi = b.server.comandi();
    solo_lettura(&comandi);
    assert!(comandi.iter().any(|c| c.contains("(QRESYNC (4242 ")));
    assert!(!comandi.iter().any(|c| c.contains("CHANGEDSINCE")));
}

#[test]
fn uidvalidity_cambiata_su_gmail() {
    let mut b = banco("uidvalidity", Stato::gmail(30));
    b.giro(&opzioni_prova());
    b.server.svuota_comandi();
    b.server.con(|s| s.casella.rinumera(9999));
    let r = b.giro(&opzioni_prova());
    assert!(r.uidvalidity_cambiata);
    // Il server ne ha 30, ma nel database ci sono solo i 20 della finestra.
    assert_eq!(r.rimappati, 20, "ricollegati per X-GM-MSGID");
    assert_eq!(r.nuovi, 0);
    assert_eq!(
        b.conta("SELECT count(*) FROM messaggi"),
        20,
        "niente duplicati"
    );
    assert_eq!(b.conta("SELECT count(*) FROM posizioni"), 20);
    assert_eq!(b.conta("SELECT min(uid) FROM posizioni"), 510);
    assert_eq!(
        b.conta("SELECT count(*) FROM messaggi WHERE eliminato IS NOT NULL"),
        0
    );
    let comandi = b.server.comandi();
    solo_lettura(&comandi);
    assert!(
        comandi
            .iter()
            .any(|c| c.contains("UID FETCH 1:* (UID X-GM-MSGID)"))
    );
    assert!(
        !comandi.iter().any(|c| c.contains("BODYSTRUCTURE")),
        "niente da riscaricare"
    );
}

#[test]
fn uidvalidity_cambiata_senza_gmail() {
    let mut stato = Stato::gmail(12);
    stato.gmail = false;
    let mut b = banco("uidvalidity-generico", stato);
    let opz = Opzioni {
        limite: None,
        ..opzioni_prova()
    };
    let r = b.giro(&opz);
    assert_eq!(r.nuovi, 12);
    b.server.con(|s| s.casella.rinumera(7));
    b.server.svuota_comandi();
    let r = b.giro(&opz);
    assert!(r.uidvalidity_cambiata);
    assert_eq!(r.rimappati, 12, "ricollegati per Message-ID e dimensione");
    assert_eq!(r.nuovi, 0);
    assert_eq!(b.conta("SELECT count(*) FROM messaggi"), 12);
    // Senza X-GM-THRID i thread vengono dai References: 1-2, 3-4, … → 6.
    assert_eq!(b.conta("SELECT count(*) FROM thread"), 6);
    solo_lettura(&b.server.comandi());
}

#[test]
fn connessione_che_cade_a_meta_fetch() {
    let mut stato = Stato::gmail(30);
    // Cade dopo 3 delle 7 risposte del primo lotto di intestazioni.
    stato.guasti.cadi_dopo = Some(3);
    let mut b = banco("cade", stato);
    let r = b.giro(&opzioni_prova());
    assert_eq!(r.riconnessioni, 1);
    assert_eq!(r.errori_rete.len(), 1);
    assert_eq!(r.nuovi, 20);
    assert_eq!(
        b.conta("SELECT count(*) FROM messaggi"),
        20,
        "niente duplicati dopo la ripresa"
    );
    assert_eq!(
        b.conta(
            "SELECT count(*) FROM (SELECT chiave FROM messaggi GROUP BY chiave HAVING count(*) > 1)"
        ),
        0
    );
    assert_eq!(b.server.con(|s| s.connessioni), 2);
}

#[test]
fn interruzione_fra_due_giri_riprende_senza_duplicati() {
    let mut stato = Stato::gmail(30);
    stato.guasti.cadi_dopo = Some(2);
    let mut b = banco("interrotto", stato);
    // Nessun tentativo di riconnessione: il giro si ferma con un errore…
    let opz = Opzioni {
        tentativi: 0,
        ..opzioni_prova()
    };
    let r = sincronizza(&mut b.col, &mut b.db, &opz);
    assert!(r.errore.as_ref().is_some_and(|e| e.di_rete()));
    let prima = b.conta("SELECT count(*) FROM messaggi");
    assert!(prima < 20);
    // …e il giro dopo riprende da dove serve.
    let r = b.giro(&opzioni_prova());
    assert_eq!(r.nuovi as i64, 20 - prima);
    assert_eq!(b.conta("SELECT count(*) FROM messaggi"), 20);
}

#[test]
fn risposte_malformate_non_fermano_niente() {
    let mut stato = Stato::gmail(30);
    stato.guasti.malformati = [22, 27].into_iter().collect();
    let mut b = banco("malformate", stato);
    let r = b.giro(&opzioni_prova());
    assert_eq!(r.malformati, 2);
    assert_eq!(r.nuovi, 18);
    assert_eq!(r.da_riprovare, 2);
    assert_eq!(b.conta("SELECT count(*) FROM mancanti"), 2);

    // Al giro dopo si richiedono senza BODYSTRUCTURE, e arrivano.
    b.server.svuota_comandi();
    let r = b.giro(&opzioni_prova());
    assert_eq!(r.nuovi, 2);
    assert_eq!(r.senza_struttura, 2);
    assert_eq!(b.conta("SELECT count(*) FROM mancanti"), 0);
    assert_eq!(
        b.conta("SELECT count(*) FROM messaggi WHERE struttura = 0"),
        2
    );
    let comandi = b.server.comandi();
    assert!(
        comandi
            .iter()
            .any(|c| c.contains("UID FETCH 22,27 ") && !c.contains("BODYSTRUCTURE")),
        "{comandi:?}"
    );
}

#[test]
fn cartella_aperta_in_scrittura_ferma_tutto() {
    let mut stato = Stato::gmail(5);
    stato.guasti.examine_in_scrittura = true;
    let mut b = banco("scrittura", stato);
    let opz = Opzioni {
        tentativi: 0,
        ..opzioni_prova()
    };
    let r = sincronizza(&mut b.col, &mut b.db, &opz);
    assert!(r.errore.is_some());
    assert_eq!(b.conta("SELECT count(*) FROM messaggi"), 0);
    assert!(!b.server.comandi().iter().any(|c| c.contains("FETCH")));
}

#[test]
fn accesso_rifiutato_senza_ripetere() {
    let server = FintoServer::avvia(Stato::gmail(5));
    let mut cfg = server.config();
    cfg.credenziali.password = motore::credenziali::Segreto::new("sbagliata".into());
    let dir = CartellaTemporanea::nuova("accesso");
    let mut db = Db::apri(&dir.0.join("prova.sqlite")).unwrap();
    let mut col = Collegamento::new(cfg, Arc::new(Contatori::default()));
    let r = sincronizza(&mut col, &mut db, &opzioni_prova());
    assert!(matches!(r.errore, Some(motore::errore::Errore::Accesso(_))));
    assert_eq!(r.riconnessioni, 0);
    assert_eq!(server.con(|s| s.connessioni), 1);
}

#[test]
fn server_muto_scade_il_tempo() {
    let mut stato = Stato::gmail(5);
    stato.guasti.saluto_muto = true;
    let server = FintoServer::avvia(stato);
    let mut cfg = server.config();
    cfg.timeout = std::time::Duration::from_millis(300);
    let dir = CartellaTemporanea::nuova("muto");
    let mut db = Db::apri(&dir.0.join("prova.sqlite")).unwrap();
    let mut col = Collegamento::new(cfg, Arc::new(Contatori::default()));
    let inizio = std::time::Instant::now();
    let opz = Opzioni {
        tentativi: 1,
        ..opzioni_prova()
    };
    let r = sincronizza(&mut col, &mut db, &opz);
    assert!(matches!(r.errore, Some(motore::errore::Errore::Timeout)));
    assert_eq!(r.riconnessioni, 1);
    assert!(inizio.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn limite_che_cresce_scarica_lo_storico() {
    let mut b = banco("storico", Stato::gmail(30));
    b.giro(&opzioni_prova());
    let r = b.giro(&Opzioni {
        limite: Some(25),
        ..opzioni_prova()
    });
    assert_eq!(r.nuovi, 5);
    assert_eq!(b.conta("SELECT min(uid) FROM posizioni"), 6);
    let r = b.giro(&Opzioni {
        limite: None,
        ..opzioni_prova()
    });
    assert_eq!(r.nuovi, 5);
    assert_eq!(b.conta("SELECT count(*) FROM messaggi"), 30);
}
