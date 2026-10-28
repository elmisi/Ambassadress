//! L'eseguibile vero contro il finto server: funziona, e quello che stampa non
//! contiene mai oggetti, indirizzi, nomi o testi dei messaggi.

mod common;

use std::process::Command;

use common::{FintoServer, PASSWORD, Stato, UTENTE};

fn lancia(argomenti: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_motore"))
        .args(argomenti)
        .output()
        .unwrap();
    let mut testo = String::from_utf8_lossy(&out.stdout).into_owned();
    testo.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), testo)
}

fn niente_contenuti(testo: &str) {
    for vietato in [
        "esempio.invalid",
        "Persona",
        "Argomento di prova",
        "Preventivo",
        "parolarara",
        "Testo inventato",
        "documento-",
        "Prove",
        UTENTE,
        PASSWORD,
    ] {
        assert!(
            !testo.contains(vietato),
            "l'output contiene «{vietato}»:\n{testo}"
        );
    }
}

#[test]
fn sincronizza_stato_cerca() {
    let server = FintoServer::avvia(Stato::gmail(30));
    let dir = std::env::temp_dir().join(format!("motore-prova-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let credenziali = dir.join("credenziali.txt");
    std::fs::write(&credenziali, format!("{UTENTE}\n{PASSWORD}\n")).unwrap();
    let db = dir.join("prova.sqlite");
    let porta = format!("127.0.0.1:{}", server.porta);
    let comuni = [
        "--server",
        porta.as_str(),
        "--in-chiaro",
        "--credenziali",
        credenziali.to_str().unwrap(),
        "--db",
        db.to_str().unwrap(),
    ];

    let mut a = vec!["sincronizza", "--limite", "25", "--corpi", "10"];
    a.extend(comuni);
    let (ok, testo) = lancia(&a);
    assert!(ok, "{testo}");
    assert!(testo.contains("nuovi 25"), "{testo}");
    assert!(testo.contains("CONDSTORE sì"), "{testo}");
    assert!(testo.contains("X-GM-EXT-1 sì"), "{testo}");
    assert!(testo.contains("[Gmail]/Tutti i messaggi"), "{testo}");
    niente_contenuti(&testo);

    let (ok, testo) = lancia(&a);
    assert!(ok, "{testo}");
    assert!(testo.contains("Differenze    nessuna"), "{testo}");
    niente_contenuti(&testo);

    let (ok, testo) = lancia(&["stato", "--db", db.to_str().unwrap()]);
    assert!(ok, "{testo}");
    assert!(testo.contains("Messaggi      25"), "{testo}");
    assert!(testo.contains("scaricati 10"), "{testo}");
    niente_contenuti(&testo);

    let (ok, testo) = lancia(&["cerca", "preventivo", "--db", db.to_str().unwrap()]);
    assert!(ok, "{testo}");
    assert!(testo.starts_with("6 messaggi trovati"), "{testo}");

    // --in-chiaro verso un server che non è locale viene rifiutato.
    let (ok, testo) = lancia(&[
        "sincronizza",
        "--server",
        "imap.example.invalid:143",
        "--in-chiaro",
    ]);
    assert!(!ok);
    assert!(testo.contains("solo verso 127.0.0.1"), "{testo}");

    // Credenziali sbagliate: errore chiaro, senza ripetere e senza stampare il segreto.
    std::fs::write(&credenziali, format!("{UTENTE}\nsbagliata\n")).unwrap();
    let (ok, testo) = lancia(&a);
    assert!(!ok);
    assert!(testo.contains("accesso IMAP rifiutato"), "{testo}");
    assert!(!testo.contains("sbagliata"), "{testo}");

    let _ = std::fs::remove_dir_all(&dir);
}
