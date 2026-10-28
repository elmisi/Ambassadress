//! Il servizio per l'app: un thread che sincronizza, resta in ascolto con IDLE e
//! risincronizza quando arriva qualcosa, si ricollega da solo dopo una caduta e
//! racconta tutto su un canale di `Notizia`. L'app lo avvia una volta e legge le
//! notizie per l'etichetta della rete («Scarico la posta, X di N», «Offline»,
//! «La password non funziona più»); i dati li legge dal database con `vista`.
//!
//! - Dopo un errore di rete riprova con attese crescenti (1 s, 2 s, 4 s… fino a 5 minuti).
//! - Se il server rifiuta utente o password si ferma e lo dice una volta: niente
//!   tentativi a ripetizione, che su Gmail possono bloccare l'account. Per riprovare
//!   l'app avvia un servizio nuovo con le credenziali nuove.
//! - Quando l'IDLE si ricollega dopo una caduta, sincronizza: nel frattempo può
//!   essere arrivato qualcosa.
//! - `Notizia::Contatto` dice quando il server ha risposto l'ultima volta (giro
//!   riuscito, IDLE collegato, controllo periodico), al massimo una al minuto.
//! - `ferma` torna in meno di mezzo secondo: chiude la lettura dei socket di ascolto
//!   e di lavoro, il LOGOUT parte ma la risposta si aspetta al massimo 300 ms.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::ascolta::{self, Evento, OpzioniAscolto};
use crate::db::Db;
use crate::errore::Errore;
use crate::rete::Contatori;
use crate::sincronizza::{self, Attese, Collegamento, Config, Notizia, Opzioni};

enum Comando {
    Sincronizza,
}

/// Il servizio in background. Si ferma con `ferma` (o quando viene lasciato andare).
pub struct Servizio {
    comandi: Sender<Comando>,
    stop: Arc<AtomicBool>,
    stop_ascolto: Arc<AtomicBool>,
    /// Comuni alle connessioni di lavoro e di ascolto: `interrompi` le chiude tutte.
    contatori: Arc<Contatori>,
    filo: Option<JoinHandle<()>>,
}

/// Al massimo ogni quanto si manda `Notizia::Contatto`.
pub const INTERVALLO_CONTATTO: Duration = Duration::from_secs(60);

/// Quanto `ferma` aspetta i thread del servizio prima di lasciarli finire da soli.
const ATTESA_CHIUSURA: Duration = Duration::from_millis(450);

/// Opzioni del servizio: quelle della sincronizzazione e quelle dell'ascolto.
#[derive(Clone, Debug, Default)]
pub struct OpzioniServizio {
    pub sincronizza: Opzioni,
    pub ascolto: OpzioniAscolto,
    /// Attese fra un tentativo e l'altro quando la rete non c'è.
    pub attesa_minima: Option<Duration>,
    pub attesa_massima: Option<Duration>,
}

impl Servizio {
    /// Avvia il servizio: apre il database in `percorso_db` (un `Db` suo), sincronizza
    /// subito e poi resta in ascolto. Le notizie arrivano su `notizie`.
    pub fn avvia(
        cfg: Config,
        percorso_db: PathBuf,
        opz: OpzioniServizio,
        notizie: Sender<Notizia>,
    ) -> Servizio {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_ascolto = Arc::new(AtomicBool::new(false));
        let contatori = Arc::new(Contatori::default());
        let filo = {
            let fili = Fili {
                stop: stop.clone(),
                stop_ascolto: stop_ascolto.clone(),
                contatori: contatori.clone(),
            };
            std::thread::Builder::new()
                .name("a10s-servizio".into())
                .spawn(move || ciclo(cfg, percorso_db, opz, notizie, rx, fili))
                .expect("thread del servizio")
        };
        Servizio {
            comandi: tx,
            stop,
            stop_ascolto,
            contatori,
            filo: Some(filo),
        }
    }

    /// Chiede una sincronizzazione adesso (per esempio quando l'app torna in primo
    /// piano, o un clic sull'etichetta della rete). Funziona anche mentre l'ascolto è
    /// in IDLE: il giro usa la connessione di lavoro, non quella dell'ascolto.
    pub fn sincronizza_ora(&self) {
        let _ = self.comandi.send(Comando::Sincronizza);
    }

    /// Ferma il servizio in meno di mezzo secondo: chiude subito la lettura delle
    /// connessioni di ascolto e di lavoro (chi aspetta il server si sveglia), manda i
    /// LOGOUT senza aspettarli oltre 300 ms e attende i thread al massimo 450 ms; se
    /// non hanno finito, finiscono da soli poco dopo (una transazione SQLite a metà
    /// si annulla da sola, quindi nel database non resta niente di rotto).
    pub fn ferma(mut self) {
        self.chiudi();
    }

    fn chiudi(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.stop_ascolto.store(true, Ordering::SeqCst);
        self.contatori.interrompi();
        if let Some(f) = self.filo.take() {
            let (tx, rx) = mpsc::channel();
            let attesa = std::thread::Builder::new()
                .name("a10s-chiusura".into())
                .spawn(move || {
                    let _ = f.join();
                    let _ = tx.send(());
                });
            if attesa.is_ok() {
                let _ = rx.recv_timeout(ATTESA_CHIUSURA);
            }
        }
    }
}

impl Drop for Servizio {
    fn drop(&mut self) {
        self.chiudi();
    }
}

fn manda(tx: &Sender<Notizia>, n: Notizia) {
    let _ = tx.send(n);
}

/// Quello che il thread del servizio condivide con `Servizio`.
struct Fili {
    stop: Arc<AtomicBool>,
    stop_ascolto: Arc<AtomicBool>,
    contatori: Arc<Contatori>,
}

/// `Notizia::Contatto`, se dall'ultima è passato almeno `INTERVALLO_CONTATTO`.
fn contatto(ultimo: &mut Option<Instant>, tx: &Sender<Notizia>) {
    if ultimo.is_some_and(|t| t.elapsed() < INTERVALLO_CONTATTO) {
        return;
    }
    *ultimo = Some(Instant::now());
    manda(
        tx,
        Notizia::Contatto {
            alle: crate::tempo::adesso(),
        },
    );
}

fn ciclo(
    cfg: Config,
    percorso_db: PathBuf,
    opz: OpzioniServizio,
    notizie: Sender<Notizia>,
    comandi: Receiver<Comando>,
    fili: Fili,
) {
    let Fili {
        stop,
        stop_ascolto,
        contatori,
    } = fili;
    let mut db = match Db::apri(&percorso_db) {
        Ok(d) => d,
        Err(e) => {
            manda(
                &notizie,
                Notizia::Errore {
                    motivo: e.to_string(),
                },
            );
            return;
        }
    };
    let mut col = Collegamento::new(cfg.clone(), contatori.clone());
    let mut opz_sync = opz.sincronizza.clone();
    opz_sync.notizie = Some(notizie.clone());
    opz_sync.mostra_avanzamento = false;
    opz_sync.stop = Some(stop.clone());
    let mut ultimo_contatto: Option<Instant> = None;

    // L'ascolto parte dopo il primo giro riuscito, così non si fanno due accessi
    // insieme con una password sbagliata.
    let (tx_ascolto, rx_ascolto) = mpsc::channel::<Evento>();
    let mut ascolto: Option<JoinHandle<()>> = None;

    let mut attese = Attese::new(
        opz.attesa_minima.unwrap_or(Duration::from_secs(1)),
        opz.attesa_massima.unwrap_or(Duration::from_secs(5 * 60)),
    );
    let mut da_fare = true;
    let mut prossimo: Option<Instant> = None;
    let mut bloccato = false;

    while !stop.load(Ordering::Relaxed) {
        let scaduto = prossimo.is_some_and(|t| Instant::now() >= t);
        if (da_fare || scaduto) && !bloccato {
            da_fare = false;
            prossimo = None;
            let r = sincronizza::sincronizza(&mut col, &mut db, &opz_sync);
            if stop.load(Ordering::Relaxed) {
                break;
            }
            match &r.errore {
                None => {
                    attese.azzera();
                    contatto(&mut ultimo_contatto, &notizie);
                    if ascolto.is_none() {
                        let (cfg, contatori, stop, tx) = (
                            cfg.clone(),
                            contatori.clone(),
                            stop_ascolto.clone(),
                            tx_ascolto.clone(),
                        );
                        let o = opz.ascolto.clone();
                        ascolto = std::thread::Builder::new()
                            .name("a10s-idle".into())
                            .spawn(move || ascolta::ascolta(cfg, contatori, o, stop, tx))
                            .ok();
                    }
                }
                Some(Errore::Accesso(_)) => {
                    // La notizia l'ha già mandata la sincronizzazione.
                    bloccato = true;
                    stop_ascolto.store(true, Ordering::Relaxed);
                }
                Some(e) if e.di_rete() => {
                    let attesa = attese.prossima();
                    manda(
                        &notizie,
                        Notizia::Offline {
                            motivo: e.to_string(),
                            riprovo_tra: Some(attesa),
                        },
                    );
                    prossimo = Some(Instant::now() + attesa);
                }
                Some(_) => {
                    // Errore del database o simili: si riprova più tardi, senza insistere.
                    prossimo = Some(Instant::now() + attese.prossima());
                }
            }
        }

        match comandi.try_recv() {
            Ok(Comando::Sincronizza) => da_fare = true,
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }

        match rx_ascolto.recv_timeout(Duration::from_millis(100)) {
            Ok(Evento::Novita { .. }) => {
                // Le novità arrivate insieme diventano un giro solo.
                while let Ok(Evento::Novita { .. }) =
                    rx_ascolto.recv_timeout(Duration::from_millis(200))
                {}
                da_fare = true;
            }
            Ok(Evento::Connesso { connessione, .. }) => {
                manda(&notizie, Notizia::InLinea);
                contatto(&mut ultimo_contatto, &notizie);
                if connessione > 1 {
                    da_fare = true;
                }
            }
            Ok(Evento::Caduto { .. }) if stop.load(Ordering::Relaxed) => {}
            Ok(Evento::Caduto { motivo, attesa }) => {
                manda(
                    &notizie,
                    Notizia::Offline {
                        motivo,
                        riprovo_tra: Some(attesa),
                    },
                );
            }
            Ok(Evento::Fermato { motivo }) => {
                if motivo.starts_with("accesso rifiutato") && !bloccato {
                    bloccato = true;
                    manda(&notizie, Notizia::AccessoRifiutato { motivo });
                }
            }
            Ok(Evento::Controllo { .. }) => contatto(&mut ultimo_contatto, &notizie),
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
    }
    stop_ascolto.store(true, Ordering::Relaxed);
    if let Some(a) = ascolto {
        let _ = a.join();
    }
    col.chiudi();
}
