//! L'ascolto della posta nuova: una connessione dedicata in IDLE, separata da
//! quella di lavoro.
//!
//! - L'IDLE si rinnova prima dei 29 minuti (RFC 2177): ogni `controllo`, e
//!   comunque entro `rinnovo`.
//! - A ogni controllo si esce da IDLE con DONE e si manda un NOOP, ciascuno con
//!   un tempo massimo breve: se il server non risponde, la connessione è morta
//!   anche se il sistema operativo non se n'è accorto.
//! - Dopo un errore ci si ricollega con attese crescenti, fino a un massimo.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::errore::{Errore, Risultato};
use crate::imap::{ModoExamine, Novita, Sessione};
use crate::rete::Contatori;
use crate::sincronizza::{Attese, Config, trova_cartella};

#[derive(Clone, Debug)]
pub struct OpzioniAscolto {
    pub cartella: Option<String>,
    /// Ogni quanto uscire da IDLE per controllare che la connessione sia viva.
    pub controllo: Duration,
    /// Rinnovo dell'IDLE: deve restare sotto i 29 minuti.
    pub rinnovo: Duration,
    /// Tempo massimo per la risposta a DONE e a NOOP durante il controllo.
    pub timeout_controllo: Duration,
    pub attesa_minima: Duration,
    pub attesa_massima: Duration,
}

impl Default for OpzioniAscolto {
    fn default() -> Self {
        OpzioniAscolto {
            cartella: None,
            controllo: Duration::from_secs(4 * 60),
            rinnovo: Duration::from_secs(25 * 60),
            timeout_controllo: Duration::from_secs(20),
            attesa_minima: Duration::from_secs(1),
            attesa_massima: Duration::from_secs(5 * 60),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evento {
    Connesso {
        cartella: String,
        connessione: u32,
    },
    /// Il controllo periodico è andato bene: quanto ci hanno messo DONE e NOOP.
    Controllo {
        durata: Duration,
    },
    /// Il server ha segnalato qualcosa: messaggi nuovi, cancellati o cambiati.
    Novita {
        quante: usize,
    },
    /// La connessione è caduta (o è morta in silenzio): si riprova dopo `attesa`.
    Caduto {
        motivo: String,
        attesa: Duration,
    },
    /// L'ascolto si ferma (accesso rifiutato o richiesta di stop).
    Fermato {
        motivo: String,
    },
}

/// Il ciclo dell'ascolto. Gira finché `stop` non diventa vero.
pub fn ascolta(
    cfg: Config,
    contatori: Arc<Contatori>,
    opz: OpzioniAscolto,
    stop: Arc<AtomicBool>,
    tx: Sender<Evento>,
) {
    let mut attese = Attese::new(opz.attesa_minima, opz.attesa_massima);
    let mut connessioni = 0u32;
    while !stop.load(Ordering::Relaxed) {
        connessioni += 1;
        match sessione_idle(&cfg, &contatori, &opz, &stop, &tx, &mut attese, connessioni) {
            Ok(()) => break,
            Err(Errore::Accesso(t)) => {
                let _ = tx.send(Evento::Fermato {
                    motivo: format!("accesso rifiutato: {t}"),
                });
                return;
            }
            // Fermati da fuori (i socket chiusi da `Contatori::interrompi`): non è una caduta.
            Err(_) if stop.load(Ordering::Relaxed) => break,
            Err(e) => {
                let attesa = attese.prossima();
                if tx
                    .send(Evento::Caduto {
                        motivo: e.to_string(),
                        attesa,
                    })
                    .is_err()
                {
                    return;
                }
                dormi(attesa, &stop);
            }
        }
    }
    let _ = tx.send(Evento::Fermato {
        motivo: "richiesta di stop".into(),
    });
}

/// Dorme fino a `d`, svegliandosi subito se arriva lo stop.
fn dormi(d: Duration, stop: &AtomicBool) {
    let fine = Instant::now() + d;
    while Instant::now() < fine && !stop.load(Ordering::Relaxed) {
        std::thread::sleep((fine - Instant::now()).min(Duration::from_millis(200)));
    }
}

fn conta(novita: &[Novita]) -> usize {
    novita.len()
}

fn sessione_idle(
    cfg: &Config,
    contatori: &Arc<Contatori>,
    opz: &OpzioniAscolto,
    stop: &AtomicBool,
    tx: &Sender<Evento>,
    attese: &mut Attese,
    connessione: u32,
) -> Risultato<()> {
    let mut s = Sessione::connetti(&cfg.indirizzo, cfg.timeout, contatori.clone())?;
    let esito = (|| -> Risultato<()> {
        s.login(
            cfg.credenziali.utente.esponi(),
            cfg.credenziali.password.esponi(),
        )?;
        if !s.ha("IDLE") {
            return Err(Errore::Uso("il server non offre IDLE".into()));
        }
        let cartella = match &opz.cartella {
            Some(c) => c.clone(),
            None => trova_cartella(&mut s)?.0,
        };
        s.examine(&cartella, ModoExamine::Semplice)?;
        let _ = tx.send(Evento::Connesso {
            cartella,
            connessione,
        });
        let ogni = opz.controllo.min(opz.rinnovo);
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let (etichetta, prime) = s.idle_inizia()?;
            attese.azzera();
            if !prime.is_empty() {
                let _ = tx.send(Evento::Novita {
                    quante: conta(&prime),
                });
            }
            let controllo = Instant::now() + ogni;
            while Instant::now() < controllo && !stop.load(Ordering::Relaxed) {
                // Passi di un secondo al massimo, per accorgersi dello stop.
                let passo = controllo.min(Instant::now() + Duration::from_secs(1));
                let novita = s.idle_attendi(passo)?;
                if !novita.is_empty() {
                    let _ = tx.send(Evento::Novita {
                        quante: conta(&novita),
                    });
                }
            }
            let t = Instant::now();
            let mut novita = s.idle_termina(&etichetta, opz.timeout_controllo)?;
            let normale = s.timeout;
            s.timeout = opz.timeout_controllo;
            let noop = s.noop();
            s.timeout = normale;
            novita.extend(noop?);
            let _ = tx.send(Evento::Controllo {
                durata: t.elapsed(),
            });
            if !novita.is_empty() {
                let _ = tx.send(Evento::Novita {
                    quante: conta(&novita),
                });
            }
        }
    })();
    match esito {
        Ok(()) => {
            s.logout();
            Ok(())
        }
        // Con lo stop la lettura è stata interrotta apposta: il LOGOUT (dopo il DONE) si
        // manda lo stesso, senza aspettarne la risposta oltre `imap::ATTESA_LOGOUT`.
        Err(e) if stop.load(Ordering::Relaxed) => {
            s.logout();
            Err(e)
        }
        Err(e) => {
            s.abbandona();
            Err(e)
        }
    }
}
