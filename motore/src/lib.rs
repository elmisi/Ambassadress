//! Il motore di Ambassadress, primo passo: sincronizzazione IMAP in sola lettura
//! verso un database SQLite locale, con misure. Vedi `README.md`.

pub mod ascolta;
pub mod credenziali;
pub mod db;
pub mod errore;
pub mod guardia;
pub mod imap;
pub mod mime;
pub mod rete;
pub mod sincronizza;
pub mod tempo;
pub mod utf7;
