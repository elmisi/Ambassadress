//! La casella configurata al primo avvio: indirizzo, server e password.
//!
//! Sta in `<dati>/casella.json`, cartella 0700 e file 0600. La password è in chiaro
//! nel file: per la prova personale basta; il portachiavi del sistema arriva dopo.
//! Non si stampa mai, neanche negli errori.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use motore::credenziali::{Credenziali, Segreto};
use motore::rete::{Indirizzo, Sicurezza};
use motore::sincronizza::Config;

#[derive(Clone, Serialize, Deserialize)]
pub struct Casella {
    pub indirizzo: String,
    #[serde(default)]
    pub nome: String,
    /// «gmail» o «imap».
    pub tipo: String,
    pub imap: String,
    #[serde(default = "porta_imap")]
    pub porta: u16,
    #[serde(default)]
    pub smtp: String,
    /// Solo per le prove col finto server su localhost.
    #[serde(default)]
    pub in_chiaro: bool,
    pub password: String,
}

fn porta_imap() -> u16 {
    993
}

impl std::fmt::Debug for Casella {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Casella")
            .field("tipo", &self.tipo)
            .field("imap", &self.imap)
            .field("porta", &self.porta)
            .finish_non_exhaustive()
    }
}

impl Casella {
    pub fn config(&self) -> Config {
        Config {
            indirizzo: Indirizzo {
                host: self.imap.clone(),
                porta: self.porta,
                sicurezza: if self.in_chiaro { Sicurezza::InChiaro } else { Sicurezza::Tls },
            },
            credenziali: Credenziali {
                utente: Segreto::new(self.indirizzo.clone()),
                password: Segreto::new(self.password.clone()),
            },
            timeout: Duration::from_secs(60),
        }
    }

    /// Quello che l'interfaccia può sapere: niente password.
    pub fn pubblica(&self) -> serde_json::Value {
        serde_json::json!({
            "address": self.indirizzo,
            "name": self.nome,
            "kind": self.tipo,
            "server": self.imap,
        })
    }
}

fn file(dati: &Path) -> PathBuf {
    dati.join("casella.json")
}

pub fn leggi(dati: &Path) -> Option<Casella> {
    let testo = fs::read_to_string(file(dati)).ok()?;
    serde_json::from_str(&testo).ok()
}

pub fn salva(dati: &Path, c: &Casella) -> std::io::Result<()> {
    prepara_cartella(dati)?;
    let provvisorio = dati.join("casella.json.nuovo");
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&provvisorio)?;
    f.write_all(serde_json::to_string_pretty(c)?.as_bytes())?;
    f.sync_all()?;
    fs::rename(&provvisorio, file(dati))
}

pub fn togli(dati: &Path) -> std::io::Result<()> {
    match fs::remove_file(file(dati)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

pub fn prepara_cartella(dati: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dati)?;
    fs::set_permissions(dati, fs::Permissions::from_mode(0o700))
}
