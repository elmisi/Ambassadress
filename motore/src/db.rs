//! Il database locale (SQLite). Lo schema è pensato per l'app vera: messaggi,
//! thread, persone e indirizzi, parti e allegati dalla struttura, testo per la
//! ricerca (FTS5), stato della sincronizzazione per cartella.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::errore::Risultato;
use crate::imap::DatiFetch;
use crate::mime::{self, Parte, Persona, Ruolo};
use crate::tempo;

const VERSIONE_SCHEMA: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    chiave TEXT PRIMARY KEY,
    valore TEXT
);

-- Stato della sincronizzazione per cartella.
CREATE TABLE IF NOT EXISTS cartelle (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL UNIQUE,
    ruolo TEXT,
    uidvalidity INTEGER,
    uidnext INTEGER,
    highestmodseq INTEGER,
    esistenti INTEGER,
    ultimo_sync INTEGER
);

CREATE TABLE IF NOT EXISTS thread (
    id INTEGER PRIMARY KEY,
    chiave TEXT NOT NULL UNIQUE,      -- gm:<X-GM-THRID> oppure ref:<radice dei References>
    gm_thrid INTEGER
);

CREATE TABLE IF NOT EXISTS messaggi (
    id INTEGER PRIMARY KEY,
    chiave TEXT NOT NULL UNIQUE,      -- identità stabile: gm:<X-GM-MSGID>, mid:<Message-ID>|<byte>, h:<impronta>|<byte>
    gm_msgid INTEGER,
    thread_id INTEGER REFERENCES thread(id),
    message_id TEXT,
    in_reply_to TEXT,
    riferimenti TEXT,                 -- References, separati da spazio
    oggetto TEXT,
    data INTEGER,                     -- intestazione Date, secondi Unix
    data_interna INTEGER,             -- INTERNALDATE, secondi Unix
    dimensione INTEGER,               -- RFC822.SIZE
    letto INTEGER NOT NULL DEFAULT 0,
    importante INTEGER NOT NULL DEFAULT 0,
    risposto INTEGER NOT NULL DEFAULT 0,
    bozza INTEGER NOT NULL DEFAULT 0,
    altre_flag TEXT,                  -- le altre parole chiave IMAP, separate da spazio
    modseq INTEGER,
    lista_id TEXT,
    automatico INTEGER NOT NULL DEFAULT 0,
    struttura INTEGER NOT NULL DEFAULT 1,  -- 0: il server non ha dato una struttura leggibile
    corpo INTEGER NOT NULL DEFAULT 0,      -- 0 da scaricare, 1 scaricato, 2 niente da scaricare
    eliminato INTEGER,                     -- quando è sparito dal server
    aggiunto INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS messaggi_thread ON messaggi(thread_id);
CREATE INDEX IF NOT EXISTS messaggi_message_id ON messaggi(message_id);
CREATE INDEX IF NOT EXISTS messaggi_data ON messaggi(data_interna);

-- Dove sta un messaggio sul server: (cartella, UID). Cambia con UIDVALIDITY, il messaggio no.
CREATE TABLE IF NOT EXISTS posizioni (
    cartella_id INTEGER NOT NULL REFERENCES cartelle(id),
    uid INTEGER NOT NULL,
    messaggio_id INTEGER NOT NULL REFERENCES messaggi(id),
    PRIMARY KEY (cartella_id, uid)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS posizioni_messaggio ON posizioni(messaggio_id);

CREATE TABLE IF NOT EXISTS indirizzi (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    nome TEXT
);

CREATE TABLE IF NOT EXISTS partecipanti (
    messaggio_id INTEGER NOT NULL REFERENCES messaggi(id),
    ruolo TEXT NOT NULL,              -- da, a, cc, ccn, rispondi_a
    posizione INTEGER NOT NULL,
    indirizzo_id INTEGER NOT NULL REFERENCES indirizzi(id),
    nome TEXT,                        -- il nome come compare in questo messaggio
    PRIMARY KEY (messaggio_id, ruolo, posizione)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS partecipanti_indirizzo ON partecipanti(indirizzo_id);

CREATE TABLE IF NOT EXISTS etichette (
    messaggio_id INTEGER NOT NULL REFERENCES messaggi(id),
    nome TEXT NOT NULL,
    PRIMARY KEY (messaggio_id, nome)
) WITHOUT ROWID;

-- Parti foglia dalla BODYSTRUCTURE: testi, allegati, immagini incorporate.
CREATE TABLE IF NOT EXISTS parti (
    messaggio_id INTEGER NOT NULL REFERENCES messaggi(id),
    parte TEXT NOT NULL,
    tipo TEXT NOT NULL,
    charset TEXT,
    codifica TEXT,
    byte INTEGER,                     -- dimensione codificata sul server
    nome_file TEXT,
    disposizione TEXT,
    content_id TEXT,
    ruolo TEXT NOT NULL,              -- testo, html, allegato, incorporato, altro
    PRIMARY KEY (messaggio_id, parte)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS corpi (
    messaggio_id INTEGER PRIMARY KEY REFERENCES messaggi(id),
    testo TEXT,
    html TEXT,
    troncato INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE IF NOT EXISTS ricerca USING fts5(
    oggetto, persone, testo,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- UID chiesti e non arrivati (o arrivati malformati): si riprovano qualche volta.
CREATE TABLE IF NOT EXISTS mancanti (
    cartella_id INTEGER NOT NULL,
    uid INTEGER NOT NULL,
    tentativi INTEGER NOT NULL,
    PRIMARY KEY (cartella_id, uid)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS sincronizzazioni (
    id INTEGER PRIMARY KEY,
    inizio INTEGER NOT NULL,
    durata_ms INTEGER,
    esito TEXT,
    nuovi INTEGER, aggiornati INTEGER, spariti INTEGER, rimappati INTEGER,
    malformati INTEGER, corpi INTEGER, byte_letti INTEGER, byte_scritti INTEGER,
    riconnessioni INTEGER
);
"#;

pub struct Db {
    conn: Connection,
    pub percorso: PathBuf,
    in_transazione: bool,
}

#[derive(Debug, Clone)]
pub struct CartellaLocale {
    pub id: i64,
    pub uidvalidity: Option<u32>,
    pub uidnext: Option<u32>,
    pub highestmodseq: Option<u64>,
    pub esistenti: Option<u32>,
}

/// Una riga di `Db::cartelle`: nome, ruolo, messaggi qui, sul server, HIGHESTMODSEQ, ultimo giro.
pub type RigaCartella = (
    String,
    Option<String>,
    i64,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);
/// Una riga di `Db::ultime_sincronizzazioni`: inizio, durata in ms, esito, nuovi, aggiornati, spariti, byte letti.
pub type RigaGiro = (i64, i64, String, i64, i64, i64, i64);
/// Un messaggio senza testo: id, UID, parti di testo da chiedere.
pub type CorpoDaScaricare = (i64, u32, Vec<Parte>);

/// Il percorso predefinito: `motore/private/ambassadress.sqlite`.
pub fn percorso_predefinito() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("private")
        .join("ambassadress.sqlite")
}

fn flag_note(flags: &[String]) -> (bool, bool, bool, bool, String) {
    let mut altre = Vec::new();
    let (mut letto, mut importante, mut risposto, mut bozza) = (false, false, false, false);
    for f in flags {
        match f.to_ascii_lowercase().as_str() {
            "\\seen" => letto = true,
            "\\flagged" => importante = true,
            "\\answered" => risposto = true,
            "\\draft" => bozza = true,
            "\\recent" => {}
            _ => altre.push(f.as_str()),
        }
    }
    altre.sort_unstable();
    (letto, importante, risposto, bozza, altre.join(" "))
}

impl Db {
    pub fn apri(percorso: &Path) -> Risultato<Db> {
        if let Some(dir) = percorso.parent()
            && !dir.as_os_str().is_empty()
            && !dir.exists()
        {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let conn = Connection::open(percorso)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(percorso, std::fs::Permissions::from_mode(0o600));
        }
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(chiave, valore) VALUES ('schema', ?1)",
            params![VERSIONE_SCHEMA.to_string()],
        )?;
        Ok(Db {
            conn,
            percorso: percorso.to_path_buf(),
            in_transazione: false,
        })
    }

    pub fn inizia(&mut self) -> Risultato<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        self.in_transazione = true;
        Ok(())
    }

    pub fn conferma(&mut self) -> Risultato<()> {
        self.conn.execute_batch("COMMIT")?;
        self.in_transazione = false;
        Ok(())
    }

    /// Annulla la transazione aperta, se c'è (dopo un errore a metà).
    pub fn annulla(&mut self) {
        if self.in_transazione {
            let _ = self.conn.execute_batch("ROLLBACK");
            self.in_transazione = false;
        }
    }

    pub fn salva_meta(&self, chiave: &str, valore: &str) -> Risultato<()> {
        self.conn.execute(
            "INSERT INTO meta(chiave, valore) VALUES (?1, ?2) ON CONFLICT(chiave) DO UPDATE SET valore = excluded.valore",
            params![chiave, valore],
        )?;
        Ok(())
    }

    pub fn meta(&self, chiave: &str) -> Risultato<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT valore FROM meta WHERE chiave = ?1",
                params![chiave],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn cartella(&self, nome: &str, ruolo: &str) -> Risultato<CartellaLocale> {
        self.conn.execute(
            "INSERT INTO cartelle(nome, ruolo) VALUES (?1, ?2) ON CONFLICT(nome) DO UPDATE SET ruolo = excluded.ruolo",
            params![nome, ruolo],
        )?;
        Ok(self.conn.query_row(
            "SELECT id, uidvalidity, uidnext, highestmodseq, esistenti FROM cartelle WHERE nome = ?1",
            params![nome],
            |r| {
                Ok(CartellaLocale {
                    id: r.get(0)?,
                    uidvalidity: r.get::<_, Option<i64>>(1)?.map(|v| v as u32),
                    uidnext: r.get::<_, Option<i64>>(2)?.map(|v| v as u32),
                    highestmodseq: r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
                    esistenti: r.get::<_, Option<i64>>(4)?.map(|v| v as u32),
                })
            },
        )?)
    }

    pub fn aggiorna_cartella(
        &self,
        id: i64,
        uidvalidity: u32,
        uidnext: Option<u32>,
        highestmodseq: Option<u64>,
        esistenti: u32,
    ) -> Risultato<()> {
        self.conn.execute(
            "UPDATE cartelle SET uidvalidity = ?2, uidnext = ?3, highestmodseq = ?4, esistenti = ?5, ultimo_sync = ?6 WHERE id = ?1",
            params![
                id,
                uidvalidity as i64,
                uidnext.map(|v| v as i64),
                highestmodseq.map(|v| v as i64),
                esistenti as i64,
                tempo::adesso()
            ],
        )?;
        Ok(())
    }

    /// Nuova UIDVALIDITY: si dimenticano UID e stato, i messaggi restano.
    pub fn azzera_cartella(&self, id: i64, uidvalidity: u32) -> Risultato<Vec<i64>> {
        let mut st = self
            .conn
            .prepare("SELECT messaggio_id FROM posizioni WHERE cartella_id = ?1")?;
        let vecchi: Vec<i64> = st
            .query_map(params![id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        self.conn
            .execute("DELETE FROM posizioni WHERE cartella_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM mancanti WHERE cartella_id = ?1", params![id])?;
        self.conn.execute(
            "UPDATE cartelle SET uidvalidity = ?2, uidnext = NULL, highestmodseq = NULL, esistenti = NULL WHERE id = ?1",
            params![id, uidvalidity as i64],
        )?;
        Ok(vecchi)
    }

    /// Segna come eliminati i messaggi che non stanno più in nessuna cartella.
    pub fn segna_eliminati_se_orfani(&self, messaggi: &[i64]) -> Risultato<usize> {
        let mut n = 0;
        let ora = tempo::adesso();
        for id in messaggi {
            n += self.conn.execute(
                "UPDATE messaggi SET eliminato = ?2 WHERE id = ?1 AND eliminato IS NULL
                 AND NOT EXISTS (SELECT 1 FROM posizioni WHERE messaggio_id = ?1)",
                params![id, ora],
            )?;
        }
        Ok(n)
    }

    pub fn uid_locali(&self, cartella_id: i64) -> Risultato<Vec<u32>> {
        let mut st = self
            .conn
            .prepare_cached("SELECT uid FROM posizioni WHERE cartella_id = ?1 ORDER BY uid")?;
        let v = st
            .query_map(params![cartella_id], |r| r.get::<_, i64>(0))?
            .map(|r| r.map(|u| u as u32))
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn messaggio_per_chiave(&self, chiave: &str) -> Risultato<Option<i64>> {
        Ok(self
            .conn
            .prepare_cached("SELECT id FROM messaggi WHERE chiave = ?1")?
            .query_row(params![chiave], |r| r.get(0))
            .optional()?)
    }

    pub fn metti_posizione(&self, cartella_id: i64, uid: u32, messaggio_id: i64) -> Risultato<()> {
        self.conn
            .prepare_cached("INSERT OR REPLACE INTO posizioni(cartella_id, uid, messaggio_id) VALUES (?1, ?2, ?3)")?
            .execute(params![cartella_id, uid as i64, messaggio_id])?;
        self.conn
            .prepare_cached(
                "UPDATE messaggi SET eliminato = NULL WHERE id = ?1 AND eliminato IS NOT NULL",
            )?
            .execute(params![messaggio_id])?;
        Ok(())
    }

    /// Toglie le posizioni dei messaggi spariti dal server. Restituisce quanti.
    pub fn togli_spariti(&self, cartella_id: i64, uid: &[u32]) -> Risultato<usize> {
        let mut messaggi = Vec::new();
        for u in uid {
            let m: Option<i64> = self
                .conn
                .prepare_cached(
                    "SELECT messaggio_id FROM posizioni WHERE cartella_id = ?1 AND uid = ?2",
                )?
                .query_row(params![cartella_id, *u as i64], |r| r.get(0))
                .optional()?;
            if let Some(m) = m {
                self.conn
                    .prepare_cached("DELETE FROM posizioni WHERE cartella_id = ?1 AND uid = ?2")?
                    .execute(params![cartella_id, *u as i64])?;
                messaggi.push(m);
            }
        }
        self.segna_eliminati_se_orfani(&messaggi)?;
        Ok(messaggi.len())
    }

    /// Aggiorna flag, etichette e MODSEQ di un messaggio già presente. Vero se qualcosa è cambiato.
    pub fn aggiorna_flag(&self, cartella_id: i64, d: &DatiFetch) -> Risultato<bool> {
        let Some(uid) = d.uid else { return Ok(false) };
        let Some(id) = self
            .conn
            .prepare_cached(
                "SELECT messaggio_id FROM posizioni WHERE cartella_id = ?1 AND uid = ?2",
            )?
            .query_row(params![cartella_id, uid as i64], |r| r.get::<_, i64>(0))
            .optional()?
        else {
            return Ok(false);
        };
        self.aggiorna_flag_messaggio(id, d)
    }

    fn aggiorna_flag_messaggio(&self, id: i64, d: &DatiFetch) -> Risultato<bool> {
        let mut cambiato = false;
        if let Some(flags) = &d.flags {
            let (letto, importante, risposto, bozza, altre) = flag_note(flags);
            cambiato |= self
                .conn
                .prepare_cached(
                    "UPDATE messaggi SET letto = ?2, importante = ?3, risposto = ?4, bozza = ?5, altre_flag = ?6
                     WHERE id = ?1 AND (letto != ?2 OR importante != ?3 OR risposto != ?4 OR bozza != ?5
                                        OR IFNULL(altre_flag, '') != ?6)",
                )?
                .execute(params![id, letto, importante, risposto, bozza, altre])?
                > 0;
        }
        if let Some(etichette) = &d.etichette {
            let attuali: HashSet<String> = self
                .conn
                .prepare_cached("SELECT nome FROM etichette WHERE messaggio_id = ?1")?
                .query_map(params![id], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let nuove: HashSet<String> = etichette.iter().cloned().collect();
            if attuali != nuove {
                self.conn
                    .prepare_cached("DELETE FROM etichette WHERE messaggio_id = ?1")?
                    .execute(params![id])?;
                for e in &nuove {
                    self.conn
                        .prepare_cached(
                            "INSERT OR IGNORE INTO etichette(messaggio_id, nome) VALUES (?1, ?2)",
                        )?
                        .execute(params![id, e])?;
                }
                cambiato = true;
            }
        }
        if let Some(m) = d.modseq {
            self.conn
                .prepare_cached("UPDATE messaggi SET modseq = ?2 WHERE id = ?1")?
                .execute(params![id, m as i64])?;
        }
        Ok(cambiato)
    }

    fn indirizzo(&self, p: &Persona) -> Risultato<i64> {
        self.conn
            .prepare_cached(
                "INSERT INTO indirizzi(email, nome) VALUES (?1, ?2)
                 ON CONFLICT(email) DO UPDATE SET nome = COALESCE(excluded.nome, indirizzi.nome)",
            )?
            .execute(params![p.email, p.nome])?;
        Ok(self
            .conn
            .prepare_cached("SELECT id FROM indirizzi WHERE email = ?1")?
            .query_row(params![p.email], |r| r.get(0))?)
    }

    fn thread(&self, chiave: &str, gm_thrid: Option<u64>) -> Risultato<i64> {
        self.conn
            .prepare_cached("INSERT OR IGNORE INTO thread(chiave, gm_thrid) VALUES (?1, ?2)")?
            .execute(params![chiave, gm_thrid.map(|t| t as i64)])?;
        Ok(self
            .conn
            .prepare_cached("SELECT id FROM thread WHERE chiave = ?1")?
            .query_row(params![chiave], |r| r.get(0))?)
    }

    /// Salva un messaggio scaricato. Se esiste già (stessa identità) aggiorna solo
    /// flag e posizione: niente duplicati, anche dopo un'interruzione.
    /// Restituisce vero se il messaggio è nuovo.
    pub fn salva_messaggio(&self, cartella_id: i64, d: &DatiFetch) -> Risultato<bool> {
        let Some(uid) = d.uid else { return Ok(false) };
        let grezze = d.intestazioni.as_deref().unwrap_or_default();
        let h = mime::leggi_intestazioni(grezze);
        let chiave = match d.gm_msgid {
            Some(g) => format!("gm:{g}"),
            None => mime::chiave_generica(
                h.message_id.as_deref(),
                h.data,
                h.da.first().map(|p| p.email.as_str()),
                h.oggetto.as_deref(),
                d.dimensione,
            ),
        };
        if let Some(id) = self.messaggio_per_chiave(&chiave)? {
            self.aggiorna_flag_messaggio(id, d)?;
            self.metti_posizione(cartella_id, uid, id)?;
            return Ok(false);
        }

        let chiave_thread = match d.gm_thrid {
            Some(t) => format!("gm:{t}"),
            None => mime::chiave_thread(&h, &chiave),
        };
        let thread_id = self.thread(&chiave_thread, d.gm_thrid)?;
        let (letto, importante, risposto, bozza, altre) =
            flag_note(d.flags.as_deref().unwrap_or_default());
        let parti = d.struttura.as_deref().unwrap_or_default();
        let ha_testo = parti
            .iter()
            .any(|p| matches!(p.ruolo, Ruolo::Testo | Ruolo::Html));
        let corpo = if ha_testo { 0 } else { 2 };
        self.conn
            .prepare_cached(
                "INSERT INTO messaggi(chiave, gm_msgid, thread_id, message_id, in_reply_to, riferimenti, oggetto,
                    data, data_interna, dimensione, letto, importante, risposto, bozza, altre_flag, modseq,
                    lista_id, automatico, struttura, corpo, aggiunto)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            )?
            .execute(params![
                chiave,
                d.gm_msgid.map(|g| g as i64),
                thread_id,
                h.message_id,
                h.in_reply_to,
                if h.riferimenti.is_empty() { None } else { Some(h.riferimenti.join(" ")) },
                h.oggetto,
                h.data,
                d.data_interna,
                d.dimensione.map(|v| v as i64),
                letto,
                importante,
                risposto,
                bozza,
                altre,
                d.modseq.map(|m| m as i64),
                h.lista_id,
                h.automatico,
                d.struttura.is_some(),
                corpo,
                tempo::adesso(),
            ])?;
        let id = self.conn.last_insert_rowid();

        let mut persone_testo = Vec::new();
        for (ruolo, lista) in [
            ("da", &h.da),
            ("a", &h.a),
            ("cc", &h.cc),
            ("ccn", &h.ccn),
            ("rispondi_a", &h.rispondi_a),
        ] {
            for (i, p) in lista.iter().enumerate() {
                let ind = self.indirizzo(p)?;
                self.conn
                    .prepare_cached(
                        "INSERT OR REPLACE INTO partecipanti(messaggio_id, ruolo, posizione, indirizzo_id, nome)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                    )?
                    .execute(params![id, ruolo, i as i64, ind, p.nome])?;
                if ruolo != "ccn" && ruolo != "rispondi_a" {
                    if let Some(n) = &p.nome {
                        persone_testo.push(n.clone());
                    }
                    persone_testo.push(p.email.clone());
                }
            }
        }
        for e in d.etichette.as_deref().unwrap_or_default() {
            self.conn
                .prepare_cached(
                    "INSERT OR IGNORE INTO etichette(messaggio_id, nome) VALUES (?1, ?2)",
                )?
                .execute(params![id, e])?;
        }
        for p in parti {
            self.conn
                .prepare_cached(
                    "INSERT OR REPLACE INTO parti(messaggio_id, parte, tipo, charset, codifica, byte, nome_file,
                        disposizione, content_id, ruolo)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?
                .execute(params![
                    id,
                    p.parte,
                    p.tipo,
                    p.charset,
                    p.codifica,
                    p.byte as i64,
                    p.nome_file,
                    p.disposizione,
                    p.content_id,
                    p.ruolo.come_testo()
                ])?;
        }
        self.conn
            .prepare_cached(
                "INSERT INTO ricerca(rowid, oggetto, persone, testo) VALUES (?1, ?2, ?3, '')",
            )?
            .execute(params![
                id,
                h.oggetto.unwrap_or_default(),
                persone_testo.join(" ")
            ])?;
        self.metti_posizione(cartella_id, uid, id)?;
        Ok(true)
    }

    pub fn mancanti(&self, cartella_id: i64) -> Risultato<HashMap<u32, u32>> {
        let mut st = self
            .conn
            .prepare_cached("SELECT uid, tentativi FROM mancanti WHERE cartella_id = ?1")?;
        let v = st
            .query_map(params![cartella_id], |r| {
                Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32))
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn segna_mancante(&self, cartella_id: i64, uid: u32) -> Risultato<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO mancanti(cartella_id, uid, tentativi) VALUES (?1, ?2, 1)
                 ON CONFLICT(cartella_id, uid) DO UPDATE SET tentativi = tentativi + 1",
            )?
            .execute(params![cartella_id, uid as i64])?;
        Ok(())
    }

    pub fn togli_mancante(&self, cartella_id: i64, uid: u32) -> Risultato<()> {
        self.conn
            .prepare_cached("DELETE FROM mancanti WHERE cartella_id = ?1 AND uid = ?2")?
            .execute(params![cartella_id, uid as i64])?;
        Ok(())
    }

    /// Dimentica i mancanti che non sono più sul server.
    pub fn pulisci_mancanti(&self, cartella_id: i64, sul_server: &HashSet<u32>) -> Risultato<()> {
        for uid in self.mancanti(cartella_id)?.keys() {
            if !sul_server.contains(uid) {
                self.togli_mancante(cartella_id, *uid)?;
            }
        }
        Ok(())
    }

    /// I messaggi più recenti della cartella ancora senza corpo, con le parti di testo da chiedere.
    pub fn corpi_da_scaricare(
        &self,
        cartella_id: i64,
        quanti: usize,
    ) -> Risultato<Vec<CorpoDaScaricare>> {
        let mut st = self.conn.prepare(
            "SELECT m.id, p.uid, m.corpo FROM posizioni p JOIN messaggi m ON m.id = p.messaggio_id
             WHERE p.cartella_id = ?1 ORDER BY p.uid DESC LIMIT ?2",
        )?;
        let righe: Vec<(i64, u32, i64)> = st
            .query_map(params![cartella_id, quanti as i64], |r| {
                Ok((r.get(0)?, r.get::<_, i64>(1)? as u32, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::new();
        let mut st = self.conn.prepare_cached(
            "SELECT parte, tipo, charset, codifica, byte, nome_file, disposizione, content_id, ruolo
             FROM parti WHERE messaggio_id = ?1 AND ruolo IN ('testo', 'html')",
        )?;
        for (id, uid, corpo) in righe {
            if corpo != 0 {
                continue;
            }
            let tutte: Vec<Parte> = st
                .query_map(params![id], |r| {
                    Ok(Parte {
                        parte: r.get(0)?,
                        tipo: r.get(1)?,
                        charset: r.get(2)?,
                        codifica: r.get(3)?,
                        byte: r.get::<_, i64>(4)? as u64,
                        nome_file: r.get(5)?,
                        disposizione: r.get(6)?,
                        content_id: r.get(7)?,
                        ruolo: Ruolo::da_testo(&r.get::<_, String>(8)?),
                    })
                })?
                .collect::<Result<_, _>>()?;
            // La prima parte di testo semplice e la prima HTML, in ordine di parte.
            let mut scelte = Vec::new();
            for ruolo in [Ruolo::Testo, Ruolo::Html] {
                if let Some(p) = tutte
                    .iter()
                    .filter(|p| p.ruolo == ruolo)
                    .min_by(|a, b| ordine_parti(&a.parte, &b.parte))
                {
                    scelte.push(p.clone());
                }
            }
            if scelte.is_empty() {
                self.conn
                    .execute("UPDATE messaggi SET corpo = 2 WHERE id = ?1", params![id])?;
                continue;
            }
            scelte.sort_by(|a, b| ordine_parti(&a.parte, &b.parte));
            out.push((id, uid, scelte));
        }
        Ok(out)
    }

    pub fn salva_corpo(
        &self,
        id: i64,
        testo: Option<&str>,
        html: Option<&str>,
        troncato: bool,
    ) -> Risultato<()> {
        self.conn
            .prepare_cached("INSERT OR REPLACE INTO corpi(messaggio_id, testo, html, troncato) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![id, testo, html, troncato])?;
        self.conn
            .prepare_cached("UPDATE messaggi SET corpo = 1 WHERE id = ?1")?
            .execute(params![id])?;
        let per_ricerca = match (testo, html) {
            (Some(t), _) if !t.trim().is_empty() => t.to_string(),
            (_, Some(h)) => mime::senza_marcatori(h),
            _ => String::new(),
        };
        self.conn
            .prepare_cached("UPDATE ricerca SET testo = ?2 WHERE rowid = ?1")?
            .execute(params![id, per_ricerca])?;
        Ok(())
    }

    pub fn registra_sincronizzazione(&self, r: &crate::sincronizza::Rapporto) -> Risultato<()> {
        self.conn.execute(
            "INSERT INTO sincronizzazioni(inizio, durata_ms, esito, nuovi, aggiornati, spariti, rimappati,
                malformati, corpi, byte_letti, byte_scritti, riconnessioni)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                r.inizio,
                r.durata.as_millis() as i64,
                r.errore.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "ok".into()),
                r.nuovi as i64,
                r.aggiornati as i64,
                r.spariti as i64,
                r.rimappati as i64,
                r.malformati as i64,
                r.corpi as i64,
                r.byte_letti as i64,
                r.byte_scritti as i64,
                r.riconnessioni as i64,
            ],
        )?;
        Ok(())
    }

    /// Un numero da una query senza parametri.
    pub fn conta(&self, sql: &str) -> Risultato<i64> {
        Ok(self.conn.query_row(sql, [], |r| r.get(0))?)
    }

    /// Quanti messaggi trova la ricerca (solo il numero).
    pub fn cerca(&self, testo: &str) -> Risultato<i64> {
        // Ogni parola tra virgolette: niente operatori FTS5 dall'utente.
        let domanda: Vec<String> = testo
            .split_whitespace()
            .map(|p| format!("\"{}\"", p.replace('"', "\"\"")))
            .collect();
        if domanda.is_empty() {
            return Ok(0);
        }
        Ok(self.conn.query_row(
            "SELECT count(*) FROM ricerca WHERE ricerca MATCH ?1",
            params![domanda.join(" ")],
            |r| r.get(0),
        )?)
    }

    pub fn cartelle(&self) -> Risultato<Vec<RigaCartella>> {
        let mut st = self.conn.prepare(
            "SELECT c.nome, c.ruolo, (SELECT count(*) FROM posizioni p WHERE p.cartella_id = c.id),
                    c.esistenti, c.highestmodseq, c.ultimo_sync
             FROM cartelle c ORDER BY c.id",
        )?;
        let v = st
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn ultime_sincronizzazioni(&self, quante: usize) -> Risultato<Vec<RigaGiro>> {
        let mut st = self.conn.prepare(
            "SELECT inizio, durata_ms, esito, nuovi, aggiornati, spariti, byte_letti
             FROM sincronizzazioni ORDER BY id DESC LIMIT ?1",
        )?;
        let v = st
            .query_map(params![quante as i64], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }
}

/// Ordina "1.10" dopo "1.2".
fn ordine_parti(a: &str, b: &str) -> std::cmp::Ordering {
    let na: Vec<u32> = a.split('.').filter_map(|x| x.parse().ok()).collect();
    let nb: Vec<u32> = b.split('.').filter_map(|x| x.parse().ok()).collect();
    na.cmp(&nb)
}
