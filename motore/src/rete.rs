//! La connessione al server: TCP, TLS, tempi massimi, conteggio dei byte e
//! suddivisione del flusso in risposte IMAP complete.
//!
//! `Connessione::invia` è l'unica funzione del programma che scrive sul socket,
//! e prima di scrivere passa il comando alla guardia della sola lettura.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::errore::{Errore, Risultato};
use crate::guardia::Guardia;

/// Oltre questa misura una risposta è considerata un errore del server.
const RISPOSTA_MASSIMA: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sicurezza {
    Tls,
    /// Solo per le prove in locale, contro il finto server.
    InChiaro,
}

#[derive(Clone, Debug)]
pub struct Indirizzo {
    pub host: String,
    pub porta: u16,
    pub sicurezza: Sicurezza,
}

impl Indirizzo {
    pub fn gmail() -> Self {
        Indirizzo {
            host: "imap.gmail.com".into(),
            porta: 993,
            sicurezza: Sicurezza::Tls,
        }
    }
}

/// Byte letti e scritti, comuni a tutte le connessioni che li condividono, e i loro
/// socket: `interrompi` li chiude tutti insieme da un altro thread (per la chiusura
/// veloce dell'app), anche mentre una lettura aspetta il server.
#[derive(Debug, Default)]
pub struct Contatori {
    pub letti: AtomicU64,
    pub scritti: AtomicU64,
    /// Dopo `interrompi` nessuna connessione nuova parte più.
    interrotto: AtomicBool,
    aperte: Mutex<Vec<(u64, TcpStream)>>,
}

/// Numeri per riconoscere i socket nel registro di `Contatori`.
static PROSSIMO_SOCKET: AtomicU64 = AtomicU64::new(1);

impl Contatori {
    pub fn letti(&self) -> u64 {
        self.letti.load(Ordering::Relaxed)
    }
    pub fn scritti(&self) -> u64 {
        self.scritti.load(Ordering::Relaxed)
    }

    /// Chiude la lettura di tutte le connessioni che condividono questi contatori:
    /// chi aspetta una risposta si sveglia subito con «connessione chiusa». La
    /// scrittura resta aperta, così il LOGOUT di cortesia può ancora uscire. Le
    /// connessioni aperte dopo vengono rifiutate subito.
    pub fn interrompi(&self) {
        self.interrotto.store(true, Ordering::SeqCst);
        if let Ok(aperte) = self.aperte.lock() {
            for (_, s) in aperte.iter() {
                let _ = s.shutdown(Shutdown::Read);
            }
        }
    }

    /// Vero dopo `interrompi`.
    pub fn interrotto(&self) -> bool {
        self.interrotto.load(Ordering::SeqCst)
    }

    fn registra(&self, s: &TcpStream) -> Option<u64> {
        let copia = s.try_clone().ok()?;
        let n = PROSSIMO_SOCKET.fetch_add(1, Ordering::Relaxed);
        let mut aperte = self.aperte.lock().ok()?;
        aperte.push((n, copia));
        // Controllato dopo l'inserimento: se `interrompi` è arrivato nel frattempo, si chiude qui.
        if self.interrotto() {
            let _ = s.shutdown(Shutdown::Read);
        }
        Some(n)
    }

    fn togli(&self, n: u64) {
        if let Ok(mut aperte) = self.aperte.lock() {
            aperte.retain(|(x, _)| *x != n);
        }
    }

    /// Quante connessioni sono aperte adesso con questi contatori.
    pub fn connessioni_aperte(&self) -> usize {
        self.aperte.lock().map(|a| a.len()).unwrap_or(0)
    }
}

trait Flusso: Read + Write + Send {}
impl<T: Read + Write + Send> Flusso for T {}

pub struct Connessione {
    flusso: Box<dyn Flusso>,
    /// Lo stesso socket, per regolare i tempi massimi e chiuderlo.
    tcp: TcpStream,
    buf: Vec<u8>,
    /// Dove riprendere la ricerca della fine della risposta: inizio della riga
    /// corrente e punto fin dove si è già cercato il CRLF.
    ripresa: (usize, usize),
    guardia: Guardia,
    contatori: Arc<Contatori>,
    /// Il posto nel registro dei socket di `contatori` (tenuto per il suo Drop).
    _registro: Option<Registro>,
    /// Versione TLS negoziata, se la connessione è cifrata.
    pub tls: Option<String>,
}

fn configurazione_tls() -> Risultato<Arc<rustls::ClientConfig>> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    if let Some(c) = CONFIG.get() {
        return Ok(c.clone());
    }
    let radici = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Errore::Tls(e.to_string()))?
    .with_root_certificates(radici)
    .with_no_client_auth();
    Ok(CONFIG.get_or_init(|| Arc::new(config)).clone())
}

fn errore_tls(e: std::io::Error) -> Errore {
    if e.kind() == std::io::ErrorKind::InvalidData {
        Errore::Tls(e.to_string())
    } else {
        Errore::from(e)
    }
}

impl Connessione {
    pub fn apri(
        indirizzo: &Indirizzo,
        timeout: Duration,
        contatori: Arc<Contatori>,
    ) -> Risultato<Self> {
        if contatori.interrotto() {
            return Err(Errore::Chiusa);
        }
        let indirizzi: Vec<_> = (indirizzo.host.as_str(), indirizzo.porta)
            .to_socket_addrs()?
            .collect();
        if indirizzi.is_empty() {
            return Err(Errore::Rete(std::io::Error::other(
                "nome del server non risolto",
            )));
        }
        let mut ultimo = None;
        let mut tcp = None;
        for a in &indirizzi {
            match TcpStream::connect_timeout(a, timeout) {
                Ok(s) => {
                    tcp = Some(s);
                    break;
                }
                Err(e) => ultimo = Some(e),
            }
        }
        let tcp = match tcp {
            Some(t) => t,
            None => return Err(ultimo.map(Errore::from).unwrap_or(Errore::Timeout)),
        };
        tcp.set_nodelay(true)?;
        tcp.set_read_timeout(Some(timeout))?;
        tcp.set_write_timeout(Some(timeout))?;
        let controllo = tcp.try_clone()?;
        // Registrata prima della stretta di mano TLS, così `interrompi` arriva anche lì.
        // Se qualcosa va storto più avanti, il Drop di `Registro` la toglie.
        let registrata = contatori.registra(&controllo).map(|n| Registro {
            contatori: contatori.clone(),
            n,
        });
        if contatori.interrotto() {
            let _ = controllo.shutdown(Shutdown::Both);
            return Err(Errore::Chiusa);
        }

        let (flusso, tls): (Box<dyn Flusso>, Option<String>) = match indirizzo.sicurezza {
            Sicurezza::InChiaro => (Box::new(tcp), None),
            Sicurezza::Tls => {
                let nome = rustls::pki_types::ServerName::try_from(indirizzo.host.clone())
                    .map_err(|_| Errore::Uso("nome del server non valido per TLS".into()))?;
                let mut conn = rustls::ClientConnection::new(configurazione_tls()?, nome)
                    .map_err(|e| Errore::Tls(e.to_string()))?;
                let mut tcp = tcp;
                while conn.is_handshaking() {
                    conn.complete_io(&mut tcp).map_err(errore_tls)?;
                }
                let versione = conn.protocol_version().map(|v| match v {
                    rustls::ProtocolVersion::TLSv1_3 => "TLS 1.3".to_string(),
                    rustls::ProtocolVersion::TLSv1_2 => "TLS 1.2".to_string(),
                    altra => format!("{altra:?}"),
                });
                (Box::new(rustls::StreamOwned::new(conn, tcp)), versione)
            }
        };
        Ok(Connessione {
            flusso,
            tcp: controllo,
            buf: Vec::with_capacity(64 * 1024),
            ripresa: (0, 0),
            guardia: Guardia::new(),
            contatori,
            _registro: registrata,
            tls,
        })
    }

    /// Manda un comando completo (con CRLF). La guardia lo controlla prima che esca.
    pub fn invia(&mut self, comando: &[u8]) -> Risultato<()> {
        self.guardia.verifica(comando)?;
        self.flusso.write_all(comando).map_err(errore_tls)?;
        self.flusso.flush().map_err(errore_tls)?;
        self.contatori
            .scritti
            .fetch_add(comando.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    pub fn annulla_idle(&mut self) {
        self.guardia.annulla_idle();
    }

    /// Legge la prossima risposta completa (con gli eventuali letterali), entro la scadenza.
    /// Se la scadenza passa, i byte già arrivati restano per la chiamata successiva.
    pub fn leggi_risposta(&mut self, scadenza: Instant) -> Risultato<Vec<u8>> {
        loop {
            if let Some(fine) = self.risposta_completa()? {
                let risposta: Vec<u8> = self.buf.drain(..fine).collect();
                self.ripresa = (0, 0);
                return Ok(risposta);
            }
            let ora = Instant::now();
            if ora >= scadenza {
                return Err(Errore::Timeout);
            }
            let attesa = (scadenza - ora).max(Duration::from_millis(1));
            self.tcp.set_read_timeout(Some(attesa))?;
            let mut pezzo = [0u8; 64 * 1024];
            match self.flusso.read(&mut pezzo) {
                Ok(0) => return Err(Errore::Chiusa),
                Ok(n) => {
                    self.buf.extend_from_slice(&pezzo[..n]);
                    self.contatori.letti.fetch_add(n as u64, Ordering::Relaxed);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(errore_tls(e)),
            }
        }
    }

    /// Se nel buffer c'è una risposta intera, la sua lunghezza.
    fn risposta_completa(&mut self) -> Risultato<Option<usize>> {
        let (mut i, cercato) = self.ripresa;
        let mut da = cercato.max(i);
        loop {
            let Some(pos) = trova_crlf(&self.buf[da..]).map(|p| p + da - i) else {
                if self.buf.len() > RISPOSTA_MASSIMA {
                    return Err(Errore::Protocollo("risposta troppo grande".into()));
                }
                // Il CR finale potrebbe aspettare il suo LF: si riparte da lì.
                self.ripresa = (i, self.buf.len().saturating_sub(1).max(i));
                return Ok(None);
            };
            let riga = &self.buf[i..i + pos];
            let fine_riga = i + pos + 2;
            match letterale(riga) {
                Some(n) => {
                    if n > RISPOSTA_MASSIMA {
                        return Err(Errore::Protocollo("letterale troppo grande".into()));
                    }
                    let dopo = fine_riga + n;
                    if self.buf.len() < dopo {
                        self.ripresa = (i, i);
                        return Ok(None);
                    }
                    i = dopo;
                    da = dopo;
                }
                None => return Ok(Some(fine_riga)),
            }
        }
    }

    pub fn chiudi(&mut self) {
        let _ = self.tcp.shutdown(Shutdown::Both);
    }
}

/// Un socket nel registro di `Contatori`: esce dal registro quando la connessione finisce.
struct Registro {
    contatori: Arc<Contatori>,
    n: u64,
}

impl Drop for Registro {
    fn drop(&mut self) {
        self.contatori.togli(self.n);
    }
}

fn trova_crlf(b: &[u8]) -> Option<usize> {
    b.windows(2).position(|w| w == b"\r\n")
}

/// Se la riga finisce con `{n}` (o `{n+}`, `~{n}`), la lunghezza del letterale che segue.
fn letterale(riga: &[u8]) -> Option<usize> {
    let riga = riga.strip_suffix(b"}")?;
    let aperta = riga.iter().rposition(|&b| b == b'{')?;
    let mut numero = &riga[aperta + 1..];
    if let Some(n) = numero.strip_suffix(b"+") {
        numero = n;
    }
    if numero.is_empty() || numero.len() > 12 || !numero.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(numero).ok()?.parse().ok()
}

#[cfg(test)]
mod prove {
    use super::letterale;

    #[test]
    fn letterali() {
        assert_eq!(letterale(b"* 1 FETCH (BODY[] {12}"), Some(12));
        assert_eq!(letterale(b"* 1 FETCH (BODY[] {12+}"), Some(12));
        assert_eq!(letterale(b"* 1 FETCH (BODY[] ~{3}"), Some(3));
        assert_eq!(letterale(b"* OK pronto"), None);
        assert_eq!(letterale(b"* OK {abc}"), None);
        assert_eq!(letterale(b"* OK {}"), None);
    }
}
