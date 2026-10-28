//! Lettura delle credenziali IMAP. Il contenuto dei file non viene mai stampato:
//! gli errori nominano solo il percorso.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::errore::Errore;

/// Una stringa segreta: non si stampa e si azzera quando non serve più.
pub struct Segreto(String);

impl Segreto {
    pub fn new(s: String) -> Self {
        Segreto(s)
    }
    pub fn esponi(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Segreto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Segreto(***)")
    }
}

impl Clone for Segreto {
    fn clone(&self) -> Self {
        Segreto(self.0.clone())
    }
}

impl Drop for Segreto {
    fn drop(&mut self) {
        // Sovrascrive i byte prima di liberare la memoria.
        let v = unsafe { self.0.as_bytes_mut() };
        for b in v.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

#[derive(Clone, Debug)]
pub struct Credenziali {
    pub utente: Segreto,
    pub password: Segreto,
}

fn cartella_segreti() -> Result<PathBuf, Errore> {
    let casa = std::env::var_os("HOME")
        .ok_or_else(|| Errore::Credenziali("manca la variabile HOME".into()))?;
    Ok(PathBuf::from(casa).join(".secrets"))
}

fn leggi_file(percorso: &Path) -> Result<String, Errore> {
    std::fs::read_to_string(percorso).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Errore::Credenziali(format!("manca {}", percorso.display()))
        }
        _ => Errore::Credenziali(format!("non riesco a leggere {}", percorso.display())),
    })
}

/// Toglie tutti gli spazi, come `tools/gmail-export.py`: le password per le app
/// di Google si copiano spesso a gruppi di quattro lettere.
fn senza_spazi(s: &str) -> String {
    s.split_whitespace().collect()
}

/// Senza percorso: `~/.secrets/gmail-user.txt` e `~/.secrets/gmail-app-password.txt`.
/// Con un percorso: un file di due righe, utente e password (per le prove).
pub fn leggi(percorso: Option<&Path>) -> Result<Credenziali, Errore> {
    let (utente, password) = match percorso {
        None => {
            let dir = cartella_segreti()?;
            let u = leggi_file(&dir.join("gmail-user.txt"))?;
            let p = leggi_file(&dir.join("gmail-app-password.txt"))?;
            let (u, p) = (senza_spazi(&u), senza_spazi(&p));
            if u.is_empty() {
                return Err(Errore::Credenziali(format!(
                    "{} è vuoto",
                    dir.join("gmail-user.txt").display()
                )));
            }
            if p.is_empty() {
                return Err(Errore::Credenziali(format!(
                    "{} è vuoto",
                    dir.join("gmail-app-password.txt").display()
                )));
            }
            (u, p)
        }
        Some(file) => {
            let testo = leggi_file(file)?;
            let mut righe = testo.lines().map(senza_spazi).filter(|r| !r.is_empty());
            match (righe.next(), righe.next()) {
                (Some(u), Some(p)) => (u, p),
                _ => {
                    return Err(Errore::Credenziali(format!(
                        "{} deve avere due righe: utente e password",
                        file.display()
                    )));
                }
            }
        }
    };
    Ok(Credenziali {
        utente: Segreto::new(utente),
        password: Segreto::new(password),
    })
}
