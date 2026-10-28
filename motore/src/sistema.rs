//! I programmi del sistema che il motore chiama, senza dipendenze nuove:
//! `pdfinfo` e `pdftoppm` (poppler-utils) per le pagine e la miniatura dei PDF,
//! `busctl` per sapere da NetworkManager se la connessione è a consumo, `df` per lo
//! spazio libero sul disco.
//!
//! Ogni chiamata ha un tempo massimo e non passa mai per una shell. Se il programma
//! manca, fallisce o non risponde in tempo, la funzione restituisce «niente» senza
//! errore: è il chiamante a fare a meno di quello che non c'è.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Com'è andata la chiamata di un programma esterno.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Uscita {
    /// Uscito con 0: quello che ha scritto su stdout.
    Fatto(Vec<u8>),
    /// Partito ma fallito (codice diverso da 0, segnale o tempo scaduto).
    Fallito,
    /// Non c'è sul sistema (o non si può lanciare).
    Assente,
}

/// Lancia `programma` con `argomenti`, aspetta al massimo `entro` e restituisce stdout.
/// Stdout si legge alla fine: va bene per le uscite brevi di questi programmi.
pub(crate) fn esegui(programma: &str, argomenti: &[&std::ffi::OsStr], entro: Duration) -> Uscita {
    let figlio = Command::new(programma)
        .args(argomenti)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Messaggi in inglese e numeri senza separatori locali, qualunque sia la lingua del sistema.
        .env("LC_ALL", "C")
        .spawn();
    let mut figlio = match figlio {
        Ok(f) => f,
        Err(_) => return Uscita::Assente,
    };
    // Stdout in un thread a parte, così un'uscita lunga non blocca il programma.
    let mut stdout = figlio.stdout.take();
    let lettore = std::thread::spawn(move || {
        let mut v = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.take(1024 * 1024).read_to_end(&mut v);
        }
        v
    });
    let fine = Instant::now() + entro;
    let stato = loop {
        match figlio.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < fine => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = figlio.kill();
                let _ = figlio.wait();
                break None;
            }
        }
    };
    let uscita = lettore.join().unwrap_or_default();
    match stato {
        Some(s) if s.success() => Uscita::Fatto(uscita),
        _ => Uscita::Fallito,
    }
}

/// Il numero di pagine dall'uscita di `pdfinfo` (`Pages:          12`).
pub(crate) fn leggi_pagine(uscita: &str) -> Option<u32> {
    uscita
        .lines()
        .find_map(|r| r.strip_prefix("Pages:"))
        .and_then(|n| n.trim().parse().ok())
        .filter(|&n| n > 0)
}

/// Pagine di un PDF con `pdfinfo`. `Ok(None)` se il PDF è protetto o rotto,
/// `Err(())` se poppler-utils non c'è (così si può riprovare quando ci sarà).
pub(crate) fn pagine_pdf_esito(pdf: &Path) -> Result<Option<u32>, ()> {
    match esegui("pdfinfo", &[pdf.as_os_str()], Duration::from_secs(10)) {
        Uscita::Fatto(o) => Ok(leggi_pagine(&String::from_utf8_lossy(&o))),
        Uscita::Fallito => Ok(None),
        Uscita::Assente => Err(()),
    }
}

/// Pagine di un PDF con `pdfinfo`; `None` se il PDF è protetto, rotto o poppler manca.
pub fn pagine_pdf(pdf: &Path) -> Option<u32> {
    pagine_pdf_esito(pdf).ok().flatten()
}

/// La prima pagina del PDF in JPEG, con il lato lungo di 360 pixel, scritta in
/// `destinazione` passando da un file temporaneo accanto (mai un JPEG a metà con
/// il nome giusto). `Ok(true)` se c'è, `Ok(false)` se il PDF non si disegna,
/// `Err(())` se `pdftoppm` non c'è.
pub(crate) fn miniatura_pdf_esito(pdf: &Path, destinazione: &Path) -> Result<bool, ()> {
    let cartella = destinazione.parent().unwrap_or_else(|| Path::new("."));
    let nome = destinazione
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // pdftoppm con -singlefile aggiunge da solo «.jpg» alla base.
    let base = cartella.join(format!(
        ".{nome}.{}.{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut scritto = base.clone().into_os_string();
    scritto.push(".jpg");
    let scritto = std::path::PathBuf::from(scritto);
    let argomenti: Vec<&std::ffi::OsStr> = [
        "-f",
        "1",
        "-l",
        "1",
        "-singlefile",
        "-scale-to",
        "360",
        "-jpeg",
        "-jpegopt",
        "quality=72",
    ]
    .iter()
    .map(std::ffi::OsStr::new)
    .chain([pdf.as_os_str(), base.as_os_str()])
    .collect();
    let esito = match esegui("pdftoppm", &argomenti, Duration::from_secs(20)) {
        Uscita::Fatto(_) => {
            let buono = std::fs::metadata(&scritto).is_ok_and(|m| m.len() > 0);
            Ok(buono && std::fs::rename(&scritto, destinazione).is_ok())
        }
        Uscita::Fallito => Ok(false),
        Uscita::Assente => Err(()),
    };
    let _ = std::fs::remove_file(&scritto);
    esito
}

/// La miniatura della prima pagina (vedi `miniatura_pdf_esito`); vero se è stata scritta.
pub fn miniatura_pdf(pdf: &Path, destinazione: &Path) -> bool {
    miniatura_pdf_esito(pdf, destinazione).unwrap_or(false)
}

/// La risposta di `busctl get-property … Metered`: `u 4`. NetworkManager usa
/// NMMetered: 1 sì, 2 no, 3 sì (indovinato), 4 no (indovinato), 0 non si sa.
pub(crate) fn leggi_a_consumo(uscita: &str) -> Option<bool> {
    let mut parole = uscita.split_whitespace();
    if parole.next()? != "u" {
        return None;
    }
    match parole.next()?.parse::<u32>().ok()? {
        1 | 3 => Some(true),
        2 | 4 => Some(false),
        _ => None,
    }
}

/// Vero se NetworkManager dice che la connessione è a consumo. Se non si sa
/// (valore 0, NetworkManager o busctl assenti) la connessione si tratta come normale.
pub fn connessione_a_consumo() -> bool {
    let argomenti: Vec<&std::ffi::OsStr> = [
        "get-property",
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
        "Metered",
    ]
    .iter()
    .map(std::ffi::OsStr::new)
    .collect();
    match esegui("busctl", &argomenti, Duration::from_secs(3)) {
        Uscita::Fatto(o) => leggi_a_consumo(&String::from_utf8_lossy(&o)).unwrap_or(false),
        _ => false,
    }
}

/// L'uscita di `df -B1 --output=avail`: un'intestazione, poi i byte.
pub(crate) fn leggi_spazio(uscita: &str) -> Option<u64> {
    uscita.lines().nth(1)?.trim().parse().ok()
}

/// Byte liberi sul disco che contiene `cartella` (`df -B1 --output=avail`).
/// `None` se df manca o fallisce.
pub fn spazio_libero(cartella: &Path) -> Option<u64> {
    let argomenti = [
        std::ffi::OsStr::new("-B1"),
        std::ffi::OsStr::new("--output=avail"),
        cartella.as_os_str(),
    ];
    match esegui("df", &argomenti, Duration::from_secs(5)) {
        Uscita::Fatto(o) => leggi_spazio(&String::from_utf8_lossy(&o)),
        _ => None,
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn uscite_dei_programmi() {
        assert_eq!(
            leggi_pagine("Producer:       x\nPages:          12\nEncrypted:      no\n"),
            Some(12)
        );
        assert_eq!(leggi_pagine("Pages: 0\n"), None);
        assert_eq!(leggi_pagine("Syntax Error\n"), None);
        assert_eq!(leggi_a_consumo("u 1\n"), Some(true));
        assert_eq!(leggi_a_consumo("u 3"), Some(true));
        assert_eq!(leggi_a_consumo("u 2\n"), Some(false));
        assert_eq!(leggi_a_consumo("u 4\n"), Some(false));
        assert_eq!(leggi_a_consumo("u 0\n"), None);
        assert_eq!(leggi_a_consumo("s \"x\"\n"), None);
        assert_eq!(leggi_spazio("       Avail\n123456789\n"), Some(123456789));
        assert_eq!(leggi_spazio("Avail\n"), None);
    }

    #[test]
    fn programmi_assenti_o_falliti() {
        assert_eq!(
            esegui("programma-che-non-esiste-a10s", &[], Duration::from_secs(1)),
            Uscita::Assente
        );
        assert_eq!(
            esegui("false", &[], Duration::from_secs(1)),
            Uscita::Fallito
        );
        let t = Instant::now();
        assert_eq!(
            esegui(
                "sleep",
                &[std::ffi::OsStr::new("5")],
                Duration::from_millis(200)
            ),
            Uscita::Fallito
        );
        assert!(
            t.elapsed() < Duration::from_secs(2),
            "tempo massimo rispettato"
        );
        // df c'è su ogni Linux: la cartella corrente ha dello spazio.
        assert!(spazio_libero(Path::new(".")).is_some_and(|b| b > 0));
        assert_eq!(spazio_libero(Path::new("/non/esiste/a10s")), None);
        // Senza NetworkManager (o con «non si sa») la connessione è normale: qui basta che non si blocchi.
        let _ = connessione_a_consumo();
    }

    #[test]
    fn pdf_rotto() {
        let dir = std::env::temp_dir().join(format!("motore-sistema-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("rotto.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\nniente di buono\n").unwrap();
        let dest = dir.join("rotto.jpg");
        assert_eq!(pagine_pdf(&pdf), None);
        assert!(!miniatura_pdf(&pdf, &dest));
        assert!(!dest.exists());
        // Nessun file temporaneo rimasto.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
