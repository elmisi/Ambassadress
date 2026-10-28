//! Gli errori del motore. I messaggi non contengono mai dati delle email:
//! al massimo il nome di un comando IMAP o il testo di stato del server.

use std::fmt;

#[derive(Debug)]
pub enum Errore {
    /// Errore della rete (connessione rifiutata, interrotta, nome non risolto…).
    Rete(std::io::Error),
    /// Il server non ha risposto entro il tempo previsto.
    Timeout,
    /// Il server ha chiuso la connessione (anche con un BYE).
    Chiusa,
    /// Errore nella connessione cifrata.
    Tls(String),
    /// Il server ha mandato qualcosa che non segue il protocollo.
    Protocollo(String),
    /// La guardia della sola lettura ha fermato un comando prima che uscisse.
    Rifiutato(String),
    /// Il server ha risposto NO o BAD a un comando.
    Server {
        comando: String,
        stato: String,
        testo: String,
    },
    /// Accesso rifiutato: indirizzo o password per le app sbagliati.
    Accesso(String),
    /// Errore del database locale.
    Db(rusqlite::Error),
    /// File delle credenziali mancante, vuoto o illeggibile.
    Credenziali(String),
    /// Parametri sbagliati sulla riga di comando.
    Uso(String),
}

impl Errore {
    /// Vero per gli errori che una nuova connessione può risolvere.
    pub fn di_rete(&self) -> bool {
        matches!(
            self,
            Errore::Rete(_)
                | Errore::Timeout
                | Errore::Chiusa
                | Errore::Tls(_)
                | Errore::Protocollo(_)
        )
    }
}

impl fmt::Display for Errore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Errore::Rete(e) => write!(f, "errore di rete: {e}"),
            Errore::Timeout => write!(f, "il server non risponde (tempo scaduto)"),
            Errore::Chiusa => write!(f, "il server ha chiuso la connessione"),
            Errore::Tls(t) => write!(f, "errore TLS: {t}"),
            Errore::Protocollo(t) => write!(f, "risposta non valida dal server: {t}"),
            Errore::Rifiutato(t) => write!(f, "comando fermato dalla guardia di sola lettura: {t}"),
            Errore::Server {
                comando,
                stato,
                testo,
            } => {
                write!(f, "il server ha risposto {stato} a {comando}")?;
                if !testo.is_empty() {
                    write!(f, ": {testo}")?;
                }
                Ok(())
            }
            Errore::Accesso(t) => write!(f, "accesso IMAP rifiutato: {t}"),
            Errore::Db(e) => write!(f, "errore del database: {e}"),
            Errore::Credenziali(t) => write!(f, "credenziali: {t}"),
            Errore::Uso(t) => write!(f, "{t}"),
        }
    }
}

impl std::error::Error for Errore {}

impl From<std::io::Error> for Errore {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => Errore::Timeout,
            std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe => Errore::Chiusa,
            _ => Errore::Rete(e),
        }
    }
}

impl From<rusqlite::Error> for Errore {
    fn from(e: rusqlite::Error) -> Self {
        Errore::Db(e)
    }
}

pub type Risultato<T> = Result<T, Errore>;
