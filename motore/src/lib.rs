//! Il motore di Ambassadress: sincronizzazione IMAP in sola lettura verso un
//! database SQLite locale, con misure, più quello che serve all'app (servizio in
//! background con notizie, testi e allegati su richiesta, allegati e storico in
//! background, vista JSON). Vedi `README.md`.

pub mod ascolta;
pub mod credenziali;
pub mod db;
pub mod errore;
pub mod guardia;
pub mod imap;
pub mod impronta;
pub mod json;
pub mod mime;
pub mod misure;
pub mod rete;
pub mod scaricamento;
pub mod servizio;
pub mod sincronizza;
pub mod sistema;
pub mod storico;
pub mod su_richiesta;
pub mod tempo;
pub mod utf7;
pub mod vista;
