//! La sincronizzazione di una cartella, in sola lettura.
//!
//! Ogni passata:
//! 1. si collega (se serve), legge le capacità, abilita CONDSTORE/QRESYNC;
//! 2. trova la cartella con tutti i messaggi (attributo `\All`), altrimenti INBOX;
//! 3. la apre con EXAMINE (con QRESYNC se c'è e si ha già uno stato);
//! 4. se UIDVALIDITY è cambiata, ricollega i messaggi già scaricati ai nuovi UID
//!    tramite l'identità stabile (X-GM-MSGID su Gmail), senza duplicati;
//! 5. confronta gli UID del server con quelli locali: spariti e da scaricare;
//! 6. aggiorna i flag cambiati (CHANGEDSINCE con CONDSTORE, altrimenti tutti);
//! 7. scarica intestazioni e struttura dei messaggi della finestra che mancano,
//!    dal più recente indietro, a lotti, ognuno in una transazione;
//! 8. scarica il testo dei messaggi più recenti con BODY.PEEK;
//! 9. salva lo stato della cartella.
//!
//! Lo stato sta tutto nel database, così una passata interrotta si riprende
//! dall'inizio senza perdere né duplicare niente.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::credenziali::Credenziali;
use crate::db::{CorpoDaScaricare, Db};
use crate::errore::{Errore, Risultato};
use crate::imap::{DatiFetch, ModoExamine, Sessione, insieme_uid};
use crate::mime::{self, CAMPI_INTESTAZIONE};
use crate::rete::{Contatori, Indirizzo};
use crate::tempo;

/// Dopo questi tentativi un UID che non arriva si lascia stare.
pub const TENTATIVI_MASSIMI: u32 = 3;

#[derive(Clone, Debug)]
pub struct Config {
    pub indirizzo: Indirizzo,
    pub credenziali: Credenziali,
    /// Tempo massimo per ogni lettura e per la connessione.
    pub timeout: Duration,
}

/// La fase di un giro, per «Scarico la posta, X di N».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fase {
    /// Intestazioni e struttura dei messaggi nuovi.
    Intestazioni,
    /// Testi dei messaggi più recenti.
    Testi,
}

/// Quello che succede durante la sincronizzazione, per l'interfaccia. Arriva sul
/// canale `Opzioni::notizie`; `servizio` aggiunge `InLinea` e le cadute dell'IDLE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notizia {
    /// Comincia un giro.
    Inizio,
    /// `fatti` di `totale` nella fase indicata.
    Passo {
        fase: Fase,
        fatti: usize,
        totale: usize,
    },
    /// Giro finito bene.
    Fine {
        nuovi: usize,
        aggiornati: usize,
        spariti: usize,
        testi: usize,
    },
    /// Rete assente o server irraggiungibile. Con `riprovo_tra` si riprova da soli dopo
    /// quell'attesa; con `None` il giro si è arreso.
    Offline {
        motivo: String,
        riprovo_tra: Option<Duration>,
    },
    /// Il server rifiuta utente o password: non si riprova finché non cambiano.
    AccessoRifiutato { motivo: String },
    /// Un errore che una nuova connessione non risolve (database, guardia, uso).
    Errore { motivo: String },
    /// La connessione IDLE è attiva: la posta nuova arriva da sola. Solo da `servizio`.
    InLinea,
    /// Il server ha risposto (secondi Unix): dopo un giro riuscito, quando l'IDLE si
    /// collega e a ogni controllo periodico dell'ascolto. Al massimo una al minuto,
    /// quindi per l'ora esatta dopo un aggiornamento a mano conta anche `Fine`.
    /// Solo da `servizio`.
    Contatto { alle: i64 },
}

#[derive(Clone, Debug)]
pub struct Opzioni {
    /// Quanti messaggi recenti tenere sincronizzati; `None` per tutti.
    pub limite: Option<usize>,
    /// Per quanti dei messaggi più recenti scaricare il testo.
    pub corpi: usize,
    /// Cartella da sincronizzare; `None` per quella con `\All` (o INBOX).
    pub cartella: Option<String>,
    pub lotto: usize,
    pub lotto_corpi: usize,
    /// Byte massimi scaricati per ogni parte di testo.
    pub massimo_corpo: u32,
    /// Riconnessioni durante una sincronizzazione prima di arrendersi.
    pub tentativi: u32,
    pub attesa_iniziale: Duration,
    pub mostra_avanzamento: bool,
    /// Dove mandare le notizie del giro (inizio, avanzamento, fine, errori).
    pub notizie: Option<Sender<Notizia>>,
    /// Se diventa vero il giro non riprova più dopo un errore di rete (chiusura dell'app).
    pub stop: Option<Arc<AtomicBool>>,
}

impl Default for Opzioni {
    fn default() -> Self {
        Opzioni {
            limite: Some(1000),
            corpi: 100,
            cartella: None,
            lotto: 200,
            lotto_corpi: 25,
            massimo_corpo: 1024 * 1024,
            tentativi: 3,
            attesa_iniziale: Duration::from_secs(1),
            mostra_avanzamento: false,
            notizie: None,
            stop: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Tempi {
    pub connessione: Duration,
    pub accesso: Duration,
    pub cartelle: Duration,
    pub examine: Duration,
    pub rimappa: Duration,
    pub elenco_uid: Duration,
    pub flag: Duration,
    pub intestazioni: Duration,
    pub corpi: Duration,
}

#[derive(Debug, Default)]
pub struct Rapporto {
    pub inizio: i64,
    pub durata: Duration,
    pub server: String,
    pub tls: Option<String>,
    pub capacita: Vec<String>,
    pub abilitate: Vec<String>,
    pub cartella: String,
    pub ruolo_cartella: String,
    pub sul_server: u32,
    pub permanentflags: Option<Vec<String>>,
    pub finestra: usize,
    pub gia_presenti: usize,
    pub da_scaricare: usize,
    pub nuovi: usize,
    pub ricollegati: usize,
    pub aggiornati: usize,
    pub spariti: usize,
    pub rimappati: usize,
    pub uidvalidity_cambiata: bool,
    pub malformati: usize,
    pub da_riprovare: usize,
    pub abbandonati: usize,
    pub no_dal_server: usize,
    pub corpi: usize,
    pub corpi_troncati: usize,
    pub allegati: usize,
    pub senza_struttura: usize,
    pub invariata: bool,
    pub modo_flag: &'static str,
    pub byte_letti: u64,
    pub byte_scritti: u64,
    /// Byte letti durante il download di intestazioni e struttura.
    pub byte_intestazioni: u64,
    /// Byte letti durante il download dei testi.
    pub byte_testi: u64,
    pub riconnessioni: u32,
    pub errori_rete: Vec<String>,
    pub errore: Option<Errore>,
    pub tempi: Tempi,
}

/// La connessione di lavoro, aperta quando serve e riaperta dopo un errore.
pub struct Collegamento {
    cfg: Config,
    contatori: Arc<Contatori>,
    sessione: Option<Sessione>,
}

impl Collegamento {
    pub fn new(cfg: Config, contatori: Arc<Contatori>) -> Self {
        Collegamento {
            cfg,
            contatori,
            sessione: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn contatori(&self) -> &Arc<Contatori> {
        &self.contatori
    }

    fn sessione(&mut self, r: &mut Rapporto) -> Risultato<&mut Sessione> {
        if self.sessione.is_none() {
            let t = Instant::now();
            let mut s = Sessione::connetti(
                &self.cfg.indirizzo,
                self.cfg.timeout,
                self.contatori.clone(),
            )?;
            r.tempi.connessione += t.elapsed();
            let t = Instant::now();
            s.login(
                self.cfg.credenziali.utente.esponi(),
                self.cfg.credenziali.password.esponi(),
            )?;
            s.id()?;
            s.abilita_estensioni()?;
            r.tempi.accesso += t.elapsed();
            self.sessione = Some(s);
        }
        Ok(self.sessione.as_mut().expect("sessione appena aperta"))
    }

    /// Butta la connessione dopo un errore di rete.
    pub fn butta(&mut self) {
        if let Some(s) = self.sessione.take() {
            s.abbandona();
        }
    }

    /// Chiude con LOGOUT.
    pub fn chiudi(&mut self) {
        if let Some(s) = self.sessione.take() {
            s.logout();
        }
    }
}

/// Attese crescenti fra un tentativo e l'altro, con un massimo.
pub struct Attese {
    prossima: Duration,
    minima: Duration,
    massima: Duration,
}

impl Attese {
    pub fn new(minima: Duration, massima: Duration) -> Self {
        Attese {
            prossima: minima,
            minima,
            massima,
        }
    }

    pub fn prossima(&mut self) -> Duration {
        let d = self.prossima;
        self.prossima = (d * 2).min(self.massima);
        // Un po' di variazione, per non ripresentarsi tutti insieme dopo un guasto.
        let variazione = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|t| t.subsec_nanos() as u64)
            .unwrap_or(0)
            % (d.as_millis() as u64 / 5 + 1);
        d + Duration::from_millis(variazione)
    }

    pub fn azzera(&mut self) {
        self.prossima = self.minima;
    }
}

/// Apre una connessione nuova già pronta: LOGIN, ID, ENABLE. Per le richieste
/// dell'app (testi, allegati) che usano una connessione loro.
pub fn apri_sessione(cfg: &Config, contatori: Arc<Contatori>) -> Risultato<Sessione> {
    let mut s = Sessione::connetti(&cfg.indirizzo, cfg.timeout, contatori)?;
    let esito = (|| {
        s.login(
            cfg.credenziali.utente.esponi(),
            cfg.credenziali.password.esponi(),
        )?;
        s.id()?;
        s.abilita_estensioni()
    })();
    match esito {
        Ok(()) => Ok(s),
        Err(e) => {
            s.abbandona();
            Err(e)
        }
    }
}

fn notizia(opz: &Opzioni, n: Notizia) {
    if let Some(tx) = &opz.notizie {
        let _ = tx.send(n);
    }
}

/// Trova la cartella con tutti i messaggi (`\All`); se non c'è, INBOX.
pub fn trova_cartella(s: &mut Sessione) -> Risultato<(String, String)> {
    let cartelle = s.elenca_cartelle()?;
    if let Some(c) = cartelle.iter().find(|c| c.tutti && c.selezionabile) {
        return Ok((c.nome.clone(), "\\All".into()));
    }
    Ok(("INBOX".into(), "INBOX (nessuna cartella \\All)".into()))
}

/// Sincronizza una volta, riprovando dopo gli errori di rete. Il rapporto c'è sempre;
/// se qualcosa è andato storto `rapporto.errore` dice cosa.
pub fn sincronizza(col: &mut Collegamento, db: &mut Db, opz: &Opzioni) -> Rapporto {
    let inizio = Instant::now();
    let letti0 = col.contatori.letti();
    let scritti0 = col.contatori.scritti();
    let mut r = Rapporto {
        inizio: tempo::adesso(),
        server: format!("{}:{}", col.cfg.indirizzo.host, col.cfg.indirizzo.porta),
        ..Default::default()
    };
    let mut attese = Attese::new(opz.attesa_iniziale, Duration::from_secs(30));
    let mut tentativi = 0;
    notizia(opz, Notizia::Inizio);
    loop {
        match passata(col, db, opz, &mut r) {
            Ok(()) => break,
            Err(e) => {
                db.annulla();
                let fermato = opz.stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed));
                if e.di_rete() {
                    if fermato {
                        // Lettura interrotta apposta: il LOGOUT si manda, senza aspettare.
                        col.chiudi();
                    } else {
                        col.butta();
                    }
                    if tentativi < opz.tentativi && !fermato {
                        tentativi += 1;
                        r.riconnessioni += 1;
                        r.errori_rete.push(e.to_string());
                        if opz.mostra_avanzamento {
                            eprintln!(
                                "\n  {e}: riprovo (tentativo {tentativi} di {})",
                                opz.tentativi
                            );
                        }
                        let attesa = attese.prossima();
                        notizia(
                            opz,
                            Notizia::Offline {
                                motivo: e.to_string(),
                                riprovo_tra: Some(attesa),
                            },
                        );
                        std::thread::sleep(attesa);
                        continue;
                    }
                }
                r.errore = Some(e);
                break;
            }
        }
    }
    r.durata = inizio.elapsed();
    r.byte_letti = col.contatori.letti() - letti0;
    r.byte_scritti = col.contatori.scritti() - scritti0;
    if !r.capacita.is_empty() {
        let _ = db.salva_meta("capacita", &r.capacita.join(" "));
    }
    if let Err(e) = db.registra_sincronizzazione(&r)
        && r.errore.is_none()
    {
        r.errore = Some(e);
    }
    // Interrotto apposta (chiusura dell'app): l'errore che ne viene non è una notizia.
    let fermato = opz.stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed));
    if r.errore.is_some() && fermato {
        return r;
    }
    notizia(
        opz,
        match &r.errore {
            None => Notizia::Fine {
                nuovi: r.nuovi,
                aggiornati: r.aggiornati,
                spariti: r.spariti,
                testi: r.corpi,
            },
            Some(Errore::Accesso(t)) => Notizia::AccessoRifiutato { motivo: t.clone() },
            Some(e) if e.di_rete() => Notizia::Offline {
                motivo: e.to_string(),
                riprovo_tra: None,
            },
            Some(e) => Notizia::Errore {
                motivo: e.to_string(),
            },
        },
    );
    r
}

fn avanzamento(opz: &Opzioni, fase: Fase, fatti: usize, totale: usize) {
    notizia(
        opz,
        Notizia::Passo {
            fase,
            fatti,
            totale,
        },
    );
    if opz.mostra_avanzamento {
        let fase = match fase {
            Fase::Intestazioni => "intestazioni",
            Fase::Testi => "testi",
        };
        eprint!(
            "\r  {fase}: {}/{}   ",
            tempo::numero(fatti as u64),
            tempo::numero(totale as u64)
        );
        let _ = std::io::stderr().flush();
        if fatti >= totale {
            eprintln!();
        }
    }
}

fn passata(col: &mut Collegamento, db: &mut Db, opz: &Opzioni, r: &mut Rapporto) -> Risultato<()> {
    let contatori = col.contatori.clone();
    let s = col.sessione(r)?;
    r.tls = s.tls().map(str::to_string);
    r.capacita = s.capacita().to_vec();
    r.abilitate = ["CONDSTORE", "QRESYNC"]
        .iter()
        .filter(|c| s.abilitata(c))
        .map(|c| c.to_string())
        .collect();
    let gmail = s.ha("X-GM-EXT-1");

    // Cartella
    let t = Instant::now();
    let (nome, ruolo) = match &opz.cartella {
        Some(c) => (c.clone(), "scelta a mano".to_string()),
        None if !r.cartella.is_empty() => (r.cartella.clone(), r.ruolo_cartella.clone()),
        None => trova_cartella(s)?,
    };
    r.tempi.cartelle += t.elapsed();
    r.cartella = nome.clone();
    r.ruolo_cartella = ruolo.clone();
    let locale = db.cartella(&nome, &ruolo)?;

    // EXAMINE
    let qresync = s.abilitata("QRESYNC");
    let condstore = s.abilitata("CONDSTORE");
    let modo = match (qresync, locale.uidvalidity, locale.highestmodseq) {
        (true, Some(v), Some(m)) => ModoExamine::Qresync {
            uidvalidity: v,
            modseq: m,
        },
        _ if condstore => ModoExamine::Condstore,
        _ => ModoExamine::Semplice,
    };
    let t = Instant::now();
    let ex = s.examine(&nome, modo)?;
    r.tempi.examine += t.elapsed();
    r.sul_server = ex.esistenti;
    r.permanentflags = ex.permanentflags.clone();
    let uidvalidity = ex
        .uidvalidity
        .ok_or_else(|| Errore::Protocollo("EXAMINE senza UIDVALIDITY".into()))?;
    let modseq_attivo = condstore && ex.highestmodseq.is_some() && !ex.nomodseq;

    // UIDVALIDITY
    let mut modseq_locale = locale.highestmodseq;
    let mut riallineata = false;
    if let Some(v) = locale.uidvalidity
        && v != uidvalidity
    {
        let t = Instant::now();
        rimappa(s, db, locale.id, uidvalidity, gmail, r)?;
        r.tempi.rimappa += t.elapsed();
        modseq_locale = None;
        riallineata = true;
    }
    let qresync_usato = matches!(modo, ModoExamine::Qresync { .. }) && !riallineata;

    // Novità portate da QRESYNC dentro EXAMINE
    if qresync_usato {
        let locali = db.uid_locali(locale.id)?;
        let spariti: Vec<u32> = locali
            .iter()
            .copied()
            .filter(|u| ex.spariti.iter().any(|g| g.contains(u)))
            .collect();
        if !spariti.is_empty() {
            r.spariti += db.togli_spariti(locale.id, &spariti)?;
        }
        for d in &ex.aggiornamenti {
            if db.aggiorna_flag(locale.id, d)? {
                r.aggiornati += 1;
            }
        }
    }

    let uid_locali = db.uid_locali(locale.id)?;
    let mancanti = db.mancanti(locale.id)?;
    let voluti = opz
        .limite
        .map_or(ex.esistenti as usize, |l| l.min(ex.esistenti as usize));
    let invariata = !riallineata
        && locale.uidnext.is_some()
        && locale.uidnext == ex.uidnext
        && locale.esistenti == Some(ex.esistenti)
        && modseq_attivo
        && modseq_locale == ex.highestmodseq
        && mancanti.is_empty();
    r.invariata = invariata && uid_locali.len() >= voluti;

    // Confronto degli UID
    let mut da_scaricare: Vec<u32> = Vec::new();
    let mut tolti: HashSet<u32> = HashSet::new();
    if !r.invariata {
        let t = Instant::now();
        let sul_server = s.uid_search_tutti()?;
        r.tempi.elenco_uid += t.elapsed();
        let insieme: HashSet<u32> = sul_server.iter().copied().collect();
        let spariti: Vec<u32> = uid_locali
            .iter()
            .copied()
            .filter(|u| !insieme.contains(u))
            .collect();
        if !spariti.is_empty() {
            r.spariti += db.togli_spariti(locale.id, &spariti)?;
            tolti.extend(spariti);
        }
        db.pulisci_mancanti(locale.id, &insieme)?;
        let finestra = match opz.limite {
            Some(l) => &sul_server[sul_server.len().saturating_sub(l)..],
            None => &sul_server[..],
        };
        let locali: HashSet<u32> = uid_locali.iter().copied().collect();
        r.finestra = finestra.len();
        r.gia_presenti = finestra.iter().filter(|u| locali.contains(u)).count();
        r.abbandonati = 0;
        for &u in finestra.iter().rev() {
            if locali.contains(&u) {
                continue;
            }
            match mancanti.get(&u) {
                Some(&t) if t >= TENTATIVI_MASSIMI => r.abbandonati += 1,
                _ => da_scaricare.push(u),
            }
        }
        r.da_scaricare = da_scaricare.len();
    } else {
        r.finestra = voluti;
        r.gia_presenti = voluti;
    }

    // Flag cambiati
    let t = Instant::now();
    let attributi_flag = format!(
        "(UID FLAGS{}{})",
        if modseq_attivo { " MODSEQ" } else { "" },
        if gmail { " X-GM-LABELS" } else { "" }
    );
    if qresync_usato {
        r.modo_flag = "QRESYNC";
    } else if modseq_attivo && modseq_locale.is_some() {
        r.modo_flag = "CONDSTORE";
        if modseq_locale != ex.highestmodseq {
            let m = modseq_locale.unwrap_or(0);
            let mut cambi = Vec::new();
            s.uid_fetch(
                "1:*",
                &attributi_flag,
                Some(&format!("(CHANGEDSINCE {m})")),
                |d| {
                    if let Some(d) = d {
                        cambi.push(d);
                    }
                },
            )?;
            db.inizia()?;
            for d in &cambi {
                if d.uid.is_some_and(|u| !tolti.contains(&u)) && db.aggiorna_flag(locale.id, d)? {
                    r.aggiornati += 1;
                }
            }
            db.conferma()?;
        }
    } else {
        let locali: Vec<u32> = db.uid_locali(locale.id)?;
        if !locali.is_empty() {
            r.modo_flag = "tutti i flag";
            let mut cambi = Vec::new();
            for pezzo in locali.chunks(5000) {
                s.uid_fetch(&insieme_uid(pezzo), &attributi_flag, None, |d| {
                    if let Some(d) = d {
                        cambi.push(d);
                    }
                })?;
            }
            db.inizia()?;
            for d in &cambi {
                if db.aggiorna_flag(locale.id, d)? {
                    r.aggiornati += 1;
                }
            }
            db.conferma()?;
        } else {
            r.modo_flag = "nessuno (prima volta)";
        }
    }
    r.tempi.flag += t.elapsed();

    // Intestazioni e struttura
    let t = Instant::now();
    let letti = contatori.letti();
    scarica_intestazioni(
        s,
        db,
        locale.id,
        &da_scaricare,
        &mancanti,
        gmail,
        modseq_attivo,
        opz,
        r,
    )?;
    r.tempi.intestazioni += t.elapsed();
    r.byte_intestazioni += contatori.letti() - letti;

    // Testo dei più recenti
    let t = Instant::now();
    let letti = contatori.letti();
    scarica_corpi(s, db, locale.id, opz, r)?;
    r.tempi.corpi += t.elapsed();
    r.byte_testi += contatori.letti() - letti;

    db.aggiorna_cartella(
        locale.id,
        uidvalidity,
        ex.uidnext,
        if modseq_attivo {
            ex.highestmodseq
        } else {
            None
        },
        ex.esistenti,
    )?;
    Ok(())
}

/// UIDVALIDITY cambiata: si ricollegano i messaggi già scaricati ai nuovi UID.
fn rimappa(
    s: &mut Sessione,
    db: &mut Db,
    cartella_id: i64,
    uidvalidity: u32,
    gmail: bool,
    r: &mut Rapporto,
) -> Risultato<()> {
    r.uidvalidity_cambiata = true;
    let mut identita: Vec<(u32, String)> = Vec::new();
    if gmail {
        s.uid_fetch("1:*", "(UID X-GM-MSGID)", None, |d| {
            if let Some(DatiFetch {
                uid: Some(u),
                gm_msgid: Some(g),
                ..
            }) = d
            {
                identita.push((u, format!("gm:{g}")));
            }
        })?;
    } else {
        s.uid_fetch(
            "1:*",
            "(UID RFC822.SIZE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID DATE FROM SUBJECT)])",
            None,
            |d| {
                if let Some(d) = d
                    && let Some(u) = d.uid
                {
                    let h = mime::leggi_intestazioni(d.intestazioni.as_deref().unwrap_or_default());
                    let chiave = mime::chiave_generica(
                        h.message_id.as_deref(),
                        h.data,
                        h.da.first().map(|p| p.email.as_str()),
                        h.oggetto.as_deref(),
                        d.dimensione,
                    );
                    identita.push((u, chiave));
                }
            },
        )?;
    }
    db.inizia()?;
    let vecchi = db.azzera_cartella(cartella_id, uidvalidity)?;
    for (uid, chiave) in &identita {
        if let Some(id) = db.messaggio_per_chiave(chiave)? {
            db.metti_posizione(cartella_id, *uid, id)?;
            r.rimappati += 1;
        }
    }
    r.spariti += db.segna_eliminati_se_orfani(&vecchi)?;
    db.conferma()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scarica_intestazioni(
    s: &mut Sessione,
    db: &mut Db,
    cartella_id: i64,
    da_scaricare: &[u32],
    mancanti: &HashMap<u32, u32>,
    gmail: bool,
    modseq: bool,
    opz: &Opzioni,
    r: &mut Rapporto,
) -> Risultato<()> {
    if da_scaricare.is_empty() {
        return Ok(());
    }
    let extra = format!(
        "{}{}",
        if gmail {
            " X-GM-MSGID X-GM-THRID X-GM-LABELS"
        } else {
            ""
        },
        if modseq { " MODSEQ" } else { "" }
    );
    let pieni = format!(
        "(UID FLAGS INTERNALDATE RFC822.SIZE BODYSTRUCTURE{extra} BODY.PEEK[HEADER.FIELDS ({CAMPI_INTESTAZIONE})])"
    );
    // Chi è già arrivato malformato si richiede senza struttura, che è la parte più delicata.
    let ridotti = format!(
        "(UID FLAGS INTERNALDATE RFC822.SIZE{extra} BODY.PEEK[HEADER.FIELDS ({CAMPI_INTESTAZIONE})])"
    );
    let (ritentati, normali): (Vec<u32>, Vec<u32>) = da_scaricare
        .iter()
        .partition(|u| mancanti.get(u).copied().unwrap_or(0) > 0);

    let totale = da_scaricare.len();
    let mut fatti = 0;
    avanzamento(opz, Fase::Intestazioni, 0, totale);
    for (lista, attributi) in [(&normali, &pieni), (&ritentati, &ridotti)] {
        for lotto in lista.chunks(opz.lotto.max(1)) {
            let chiesti: HashSet<u32> = lotto.iter().copied().collect();
            let mut arrivati: Vec<DatiFetch> = Vec::new();
            let mut malformati = 0;
            match s.uid_fetch(&insieme_uid(lotto), attributi, None, |d| match d {
                Some(d) => arrivati.push(d),
                None => malformati += 1,
            }) {
                Ok(()) => {}
                // Gmail risponde NO se qualche messaggio non è leggibile, ma manda gli altri.
                Err(Errore::Server { .. }) => r.no_dal_server += 1,
                Err(e) => return Err(e),
            }
            r.malformati += malformati;
            db.inizia()?;
            let mut salvati = HashSet::new();
            for d in &arrivati {
                let Some(uid) = d.uid else { continue };
                // Risposte non chieste (flag cambiati nel frattempo): si ignorano qui.
                if !chiesti.contains(&uid) || d.intestazioni.is_none() || salvati.contains(&uid) {
                    continue;
                }
                if db.salva_messaggio(cartella_id, d)? {
                    r.nuovi += 1;
                    if let Some(p) = &d.struttura {
                        r.allegati += p
                            .iter()
                            .filter(|p| p.ruolo == mime::Ruolo::Allegato)
                            .count();
                    }
                    if d.struttura.is_none() {
                        r.senza_struttura += 1;
                    }
                } else {
                    r.ricollegati += 1;
                }
                db.togli_mancante(cartella_id, uid)?;
                salvati.insert(uid);
            }
            for &u in lotto {
                if !salvati.contains(&u) {
                    db.segna_mancante(cartella_id, u)?;
                }
            }
            db.conferma()?;
            r.da_riprovare += lotto.len() - salvati.len();
            fatti += lotto.len();
            avanzamento(opz, Fase::Intestazioni, fatti, totale);
        }
    }
    Ok(())
}

fn scarica_corpi(
    s: &mut Sessione,
    db: &mut Db,
    cartella_id: i64,
    opz: &Opzioni,
    r: &mut Rapporto,
) -> Risultato<()> {
    if opz.corpi == 0 {
        return Ok(());
    }
    let lista = db.corpi_da_scaricare(cartella_id, opz.corpi)?;
    scarica_lista_corpi(s, db, &lista, opz, r)
}

/// Scarica e salva il testo dei messaggi indicati, nella cartella già aperta.
pub(crate) fn scarica_lista_corpi(
    s: &mut Sessione,
    db: &mut Db,
    lista: &[CorpoDaScaricare],
    opz: &Opzioni,
    r: &mut Rapporto,
) -> Risultato<()> {
    if lista.is_empty() {
        return Ok(());
    }
    // Messaggi con le stesse parti si chiedono insieme.
    let mut gruppi: BTreeMap<String, Vec<&CorpoDaScaricare>> = BTreeMap::new();
    for m in lista {
        let firma =
            m.2.iter()
                .map(|p| p.parte.as_str())
                .collect::<Vec<_>>()
                .join("+");
        gruppi.entry(firma).or_default().push(m);
    }
    let totale = lista.len();
    let mut fatti = 0;
    let massimo = opz.massimo_corpo;
    avanzamento(opz, Fase::Testi, 0, totale);
    for gruppo in gruppi.values() {
        for lotto in gruppo.chunks(opz.lotto_corpi.max(1)) {
            let parti = &lotto[0].2;
            let attributi = format!(
                "(UID{})",
                parti
                    .iter()
                    .map(|p| format!(" BODY.PEEK[{}]<0.{massimo}>", p.parte))
                    .collect::<String>()
            );
            let uid: Vec<u32> = lotto.iter().map(|m| m.1).collect();
            let mut arrivati: HashMap<u32, DatiFetch> = HashMap::new();
            match s.uid_fetch(&insieme_uid(&uid), &attributi, None, |d| {
                if let Some(d) = d
                    && let Some(u) = d.uid
                    && !d.sezioni.is_empty()
                {
                    arrivati.insert(u, d);
                }
            }) {
                Ok(()) => {}
                Err(Errore::Server { .. }) => r.no_dal_server += 1,
                Err(e) => return Err(e),
            }
            db.inizia()?;
            for (id, u, parti) in lotto.iter().map(|m| (m.0, m.1, &m.2)) {
                let Some(d) = arrivati.get(&u) else { continue };
                let mut testo: Option<String> = None;
                let mut html: Option<String> = None;
                let mut troncato = false;
                for p in parti {
                    let Some((_, dati)) = d.sezioni.iter().find(|(n, _)| *n == p.parte) else {
                        continue;
                    };
                    troncato |= p.byte > massimo as u64;
                    let (t, h) = mime::decodifica_parte(p, dati);
                    match p.ruolo {
                        // Il testo semplice vero vince su quello ricavato dall'HTML.
                        mime::Ruolo::Testo => {
                            if t.is_some() {
                                testo = t;
                            }
                        }
                        mime::Ruolo::Html => {
                            html = html.or(h);
                            if testo.is_none() {
                                testo = t;
                            }
                        }
                        _ => {}
                    }
                }
                db.salva_corpo(id, testo.as_deref(), html.as_deref(), troncato)?;
                r.corpi += 1;
                if troncato {
                    r.corpi_troncati += 1;
                }
            }
            db.conferma()?;
            fatti += lotto.len();
            avanzamento(opz, Fase::Testi, fatti, totale);
        }
    }
    Ok(())
}
