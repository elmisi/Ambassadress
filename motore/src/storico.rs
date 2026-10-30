//! Lo storico (giro 2): i messaggi più vecchi della finestra che la sincronizzazione tiene,
//! scaricati a poco a poco su una connessione a parte. Prima le intestazioni di tutta la
//! casella, dal più recente indietro, poi i testi, sempre dal più recente; gli allegati li
//! scarica già il background degli allegati (`scaricamento`).
//!
//! - Ogni `passo` fa un lotto e torna: chi lo chiama decide le pause (quota del giorno,
//!   connessione a consumo, disco quasi pieno) e lascia passare davanti la posta nuova e i clic.
//! - Lo stato sta tutto nel database: dopo una chiusura si riparte da quello che manca.
//! - La finestra resta della sincronizzazione: lo storico prende solo gli UID più vecchi dei
//!   suoi `finestra` più recenti, così le due connessioni non scaricano mai lo stesso messaggio.
//! - Se UIDVALIDITY è cambiata si ferma (`Errore::Uso`) finché la sincronizzazione non ha
//!   ricollegato i messaggi.
//! - I byte scaricati contano nella quota del giorno, come gli allegati.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::db::Db;
use crate::errore::{Errore, Risultato};
use crate::imap::{ModoExamine, Sessione};
use crate::rete::Contatori;
use crate::sincronizza::{self, Config, Opzioni, Rapporto, TENTATIVI_MASSIMI};
use crate::su_richiesta::Interruttore;

/// Quello che ha fatto un passo.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Passo {
    /// Messaggi nuovi salvati (intestazioni e struttura).
    pub intestazioni: usize,
    /// Testi salvati.
    pub testi: usize,
    /// Intestazioni ancora da scaricare dopo questo passo.
    pub restano: usize,
    /// Byte letti dal server.
    pub byte: u64,
    /// Vero se non resta niente da fare: intestazioni e testi ci sono tutti.
    pub finito: bool,
    /// Vero se la sincronizzazione non ha ancora fatto il primo giro: si riprova dopo.
    pub aspetta: bool,
}

/// A che punto è lo storico, per il pannello delle caselle. Solo numeri.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StatoStorico {
    /// Messaggi nella cartella sul server (all'ultimo giro).
    pub sul_server: u64,
    /// Messaggi della cartella nel database.
    pub in_locale: u64,
    /// Di questi, quanti aspettano ancora il testo.
    pub senza_testo: u64,
}

/// La cartella aperta e la lista degli UID che mancano.
struct Coda {
    cartella_id: i64,
    uidvalidity: u32,
    gmail: bool,
    modseq: bool,
    /// UID da scaricare, in ordine crescente: si prendono dalla fine (dal più recente).
    uid: Vec<u32>,
}

/// Lo storico su una connessione sua. Uno solo, nel thread che lo fa andare.
pub struct Storico {
    cfg: Config,
    contatori: Arc<Contatori>,
    sessione: Option<Sessione>,
    /// Cartella aperta con EXAMINE sulla connessione.
    aperta: Option<String>,
    finestra: usize,
    coda: Option<Coda>,
    /// Testi chiesti in questa sessione e non arrivati: non si richiedono a ogni passo.
    saltati: HashSet<i64>,
}

impl Storico {
    /// `finestra`: quanti messaggi recenti tiene la sincronizzazione (`Opzioni::limite`).
    pub fn new(cfg: Config, finestra: usize) -> Self {
        Storico {
            cfg,
            contatori: Arc::new(Contatori::default()),
            sessione: None,
            aperta: None,
            finestra,
            coda: None,
            saltati: HashSet::new(),
        }
    }

    /// L'interruttore per chiudere la connessione da un altro thread (chiusura dell'app).
    pub fn interruttore(&self) -> Interruttore {
        Interruttore(self.contatori.clone())
    }

    /// Chiude la connessione con LOGOUT. La lista degli UID resta: al passo dopo si riapre
    /// la cartella e si continua.
    pub fn chiudi(&mut self) {
        if let Some(s) = self.sessione.take() {
            s.logout();
        }
        self.aperta = None;
    }

    fn butta(&mut self) {
        if let Some(s) = self.sessione.take() {
            s.abbandona();
        }
        self.aperta = None;
    }

    /// Un lotto: `lotto` intestazioni se ne mancano, altrimenti `lotto` testi.
    pub fn passo(&mut self, db: &mut Db, lotto: usize) -> Risultato<Passo> {
        let Some((cartella_id, nome, uidvalidity, _)) = db.cartella_principale()? else {
            return Ok(Passo { aspetta: true, ..Passo::default() });
        };
        let letti = self.contatori.letti();
        let esito = self.passo_aperto(db, cartella_id, &nome, uidvalidity, lotto.max(1));
        let byte = self.contatori.letti() - letti;
        if byte > 0 {
            // senza pretese: la quota è una stima, il lotto è già salvato
            let _ = db.aggiungi_scaricati(byte);
        }
        match esito {
            Ok(mut p) => {
                p.byte = byte;
                Ok(p)
            }
            Err(e) => {
                db.annulla();
                if e.di_rete() {
                    self.butta();
                }
                Err(e)
            }
        }
    }

    fn passo_aperto(
        &mut self,
        db: &mut Db,
        cartella_id: i64,
        nome: &str,
        uidvalidity: u32,
        lotto: usize,
    ) -> Risultato<Passo> {
        if self.sessione.is_none() {
            self.sessione = Some(sincronizza::apri_sessione(&self.cfg, self.contatori.clone())?);
            self.aperta = None;
        }
        let s = self.sessione.as_mut().expect("sessione appena aperta");
        if self.aperta.as_deref() != Some(nome) {
            let ex = s.examine(nome, ModoExamine::Semplice)?;
            if ex.uidvalidity != Some(uidvalidity) {
                self.coda = None;
                return Err(Errore::Uso(
                    "UIDVALIDITY cambiata: lo storico aspetta la sincronizzazione".into(),
                ));
            }
            self.aperta = Some(nome.to_string());
            let gmail = s.ha("X-GM-EXT-1");
            let modseq = s.abilitata("CONDSTORE") && ex.highestmodseq.is_some() && !ex.nomodseq;
            let vecchia = self
                .coda
                .as_ref()
                .is_some_and(|c| c.cartella_id != cartella_id || c.uidvalidity != uidvalidity);
            if vecchia {
                self.coda = None;
            }
            if let Some(c) = self.coda.as_mut() {
                c.gmail = gmail;
                c.modseq = modseq;
            } else {
                // la lista si fa una volta per sessione: UID SEARCH ALL costa qualche centinaio
                // di KB su una casella grande
                let sul_server = s.uid_search_tutti()?;
                let vecchi = &sul_server[..sul_server.len().saturating_sub(self.finestra)];
                let locali: HashSet<u32> = db.uid_locali(cartella_id)?.into_iter().collect();
                let mancanti = db.mancanti(cartella_id)?;
                let uid = vecchi
                    .iter()
                    .copied()
                    .filter(|u| !locali.contains(u))
                    .filter(|u| mancanti.get(u).copied().unwrap_or(0) < TENTATIVI_MASSIMI)
                    .collect();
                self.coda = Some(Coda { cartella_id, uidvalidity, gmail, modseq, uid });
            }
        }
        let coda = self.coda.as_mut().expect("coda appena fatta");

        // Intestazioni e struttura, dal più recente indietro
        if !coda.uid.is_empty() {
            let da = coda.uid.len().saturating_sub(lotto);
            let lotto_uid: Vec<u32> = coda.uid.split_off(da).into_iter().rev().collect();
            let gia = db.uid_presenti(cartella_id, &lotto_uid)?;
            let chiesti: Vec<u32> = lotto_uid.into_iter().filter(|u| !gia.contains(u)).collect();
            let mut r = Rapporto::default();
            if !chiesti.is_empty() {
                let tutti = db.mancanti(cartella_id)?;
                let mancanti: HashMap<u32, u32> =
                    chiesti.iter().filter_map(|u| tutti.get(u).map(|t| (*u, *t))).collect();
                let opz = Opzioni { lotto: chiesti.len(), ..Opzioni::default() };
                sincronizza::scarica_intestazioni(
                    s,
                    db,
                    cartella_id,
                    &chiesti,
                    &mancanti,
                    coda.gmail,
                    coda.modseq,
                    &opz,
                    &mut r,
                )?;
            }
            return Ok(Passo {
                intestazioni: r.nuovi + r.ricollegati,
                restano: coda.uid.len(),
                ..Passo::default()
            });
        }

        // Poi i testi, dal più recente indietro
        let lista = db.corpi_storico(cartella_id, lotto, &self.saltati)?;
        if lista.is_empty() {
            return Ok(Passo { finito: true, ..Passo::default() });
        }
        let opz = Opzioni { lotto_corpi: lotto, ..Opzioni::default() };
        let mut r = Rapporto::default();
        sincronizza::scarica_lista_corpi(s, db, &lista, &opz, &mut r)?;
        let ids: Vec<i64> = lista.iter().map(|c| c.0).collect();
        self.saltati.extend(db.senza_corpo(&ids)?);
        Ok(Passo { testi: r.corpi, ..Passo::default() })
    }
}

/// A che punto è lo storico, o `None` se la sincronizzazione non ha ancora fatto un giro.
pub fn stato(db: &Db) -> Risultato<Option<StatoStorico>> {
    let Some((cartella_id, _, _, sul_server)) = db.cartella_principale()? else {
        return Ok(None);
    };
    let (in_locale, senza_testo) = db.conta_storico(cartella_id)?;
    Ok(Some(StatoStorico { sul_server: sul_server as u64, in_locale, senza_testo }))
}
