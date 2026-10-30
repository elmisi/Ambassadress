//! Quello che l'app chiede quando serve, ognuno su una connessione sua (EXAMINE e
//! BODY.PEEK, quindi sempre in sola lettura): il testo dei messaggi che non lo hanno
//! ancora e il contenuto delle parti (allegati, immagini incorporate).
//!
//! Gli allegati si salvano per contenuto in `<cartella del database>/allegati/`, con
//! il nome uguale allo SHA-256 del contenuto decodificato: lo stesso file in più
//! email occupa spazio una volta sola. La tabella `allegati` ricorda messaggio e
//! parte → impronta, così la seconda volta non si va in rete.
//!
//! Appena una parte è nell'archivio se ne leggono le misure: larghezza e altezza
//! delle immagini (`misure`), pagine e miniatura della prima pagina dei PDF
//! (`sistema`, con poppler-utils). La miniatura sta accanto al file, con lo stesso
//! nome più `.miniatura.jpg`, e si fa una volta sola per contenuto.
//!
//! Più tardi, in background, `completa_contenuti` legge quello che c'è dentro: il testo
//! dei PDF, dei documenti e dei file di testo (per la ricerca) e il tipo dei QR code e
//! dei codici a barre nelle immagini (tabella `contenuti`, giro 2).
//!
//! Tutte le funzioni sono bloccanti, con il tempo massimo di `Config::timeout` su
//! ogni lettura, e si possono chiamare da un thread qualunque con il suo `Db`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::{OptionalExtension, params};

use crate::db::{CorpoDaScaricare, Db, MisureParte, Posizione};
use crate::errore::{Errore, Risultato};
use crate::imap::{ModoExamine, Sessione};
use crate::rete::Contatori;
use crate::sincronizza::{self, Config, Opzioni, Rapporto};
use crate::{impronta, misure, sistema, tempo};

/// Le parti più grandi si chiedono a pezzi di questa misura (`BODY.PEEK[n]<inizio.quanti>`),
/// così nessuna risposta sola deve arrivare tutta entro il tempo massimo.
const PEZZO: u64 = 4 * 1024 * 1024;

/// Una parte scaricata e decodificata, pronta da servire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allegato {
    /// Il file decodificato nell'archivio.
    pub percorso: PathBuf,
    /// Tipo MIME in minuscolo, come nella struttura del messaggio.
    pub tipo: String,
    pub nome_file: Option<String>,
    /// Byte del contenuto decodificato.
    pub byte: u64,
    /// SHA-256 del contenuto, in esadecimale (è anche il nome del file).
    pub impronta: String,
}

/// La cartella dell'archivio degli allegati: `allegati/` accanto al database.
pub fn cartella_allegati(db: &Db) -> PathBuf {
    db.percorso
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("allegati")
}

/// Il nome della miniatura di un PDF nell'archivio: accanto al file, stesso hash.
pub fn nome_miniatura(impronta: &str) -> String {
    format!("{impronta}.miniatura.jpg")
}

/// Vero per i nomi dei file di contenuto dell'archivio (64 cifre esadecimali):
/// miniature e file temporanei restano fuori.
pub fn nome_di_contenuto(nome: &str) -> bool {
    nome.len() == 64 && nome.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parte_valida(parte: &str) -> bool {
    !parte.is_empty()
        && parte.split('.').all(|n| {
            !n.is_empty() && n.len() <= 4 && n.bytes().all(|b| b.is_ascii_digit()) && n != "0"
        })
}

/// La parte se è già nell'archivio, senza andare in rete.
pub fn parte_locale(db: &Db, messaggio: i64, parte: &str) -> Risultato<Option<Allegato>> {
    if !parte_valida(parte) {
        return Err(Errore::Uso("numero di parte non valido".into()));
    }
    let Some(info) = db.parte(messaggio, parte)? else {
        return Ok(None);
    };
    let Some((impronta, byte)) = db.allegato_scaricato(messaggio, parte)? else {
        return Ok(None);
    };
    let percorso = cartella_allegati(db).join(&impronta);
    match std::fs::metadata(&percorso) {
        Ok(m) if m.len() == byte => Ok(Some(Allegato {
            percorso,
            tipo: info.tipo,
            nome_file: info.nome_file,
            byte,
            impronta,
        })),
        _ => Ok(None),
    }
}

/// Il percorso della miniatura della prima pagina, se la parte è un PDF già
/// scaricato e la miniatura c'è. L'app la serve come
/// `a10s://localhost/miniatura/<messaggio>/<parte>`; niente rete.
pub fn miniatura(db: &Db, messaggio: i64, parte: &str) -> Risultato<Option<PathBuf>> {
    if !parte_valida(parte) {
        return Err(Errore::Uso("numero di parte non valido".into()));
    }
    let Some((impronta, _)) = db.allegato_scaricato(messaggio, parte)? else {
        return Ok(None);
    };
    let p = cartella_allegati(db).join(nome_miniatura(&impronta));
    Ok(std::fs::metadata(&p)
        .is_ok_and(|m| m.is_file() && m.len() > 0)
        .then_some(p))
}

/// Il nome della pagina `n` di un PDF disegnata per l'anteprima: accanto al file, stesso hash.
pub fn nome_pagina(impronta: &str, n: u32) -> String {
    format!("{impronta}.pagina-{n}.jpg")
}

/// La pagina `n` (da 1) di un PDF già scaricato, disegnata la prima volta che serve e poi
/// tenuta nell'archivio. `None` se la parte non è nell'archivio, non è un PDF con quella
/// pagina o non si disegna (anche senza poppler-utils). Niente rete.
pub fn pagina(db: &Db, messaggio: i64, parte: &str, n: u32) -> Risultato<Option<PathBuf>> {
    let Some(a) = parte_locale(db, messaggio, parte)? else {
        return Ok(None);
    };
    let pagine = db.misure(messaggio, parte)?.and_then(|m| m.pagine).unwrap_or(0);
    if n == 0 || n > pagine {
        return Ok(None);
    }
    let dest = cartella_allegati(db).join(nome_pagina(&a.impronta, n));
    if std::fs::metadata(&dest).is_ok_and(|m| m.is_file() && m.len() > 0) {
        return Ok(Some(dest));
    }
    Ok(sistema::pagina_pdf_esito(&a.percorso, n, &dest)
        .unwrap_or(false)
        .then_some(dest))
}

/// Vero se il contenuto è un PDF: dal tipo, oppure dalla firma `%PDF-` nei primi
/// 1024 byte (c'è chi manda i PDF come `application/octet-stream`).
fn e_pdf(tipo: &str, dati: &[u8]) -> bool {
    tipo.eq_ignore_ascii_case("application/pdf")
        || dati[..dati.len().min(1024)]
            .windows(5)
            .any(|w| w == b"%PDF-")
}

/// Le misure di una parte appena archiviata. Per i PDF chiama poppler (pagine e
/// miniatura, una volta per contenuto); se manca, i campi restano `None` e si
/// riprovano con `completa_archivio`.
fn misura(tipo: &str, dati: &[u8], cartella: &Path, impronta: &str) -> MisureParte {
    let mut m = MisureParte::default();
    if let Some((l, a)) = misure::misure_immagine(dati) {
        m.larghezza = Some(l);
        m.altezza = Some(a);
        return m;
    }
    if tipo.to_ascii_lowercase().starts_with("image/") {
        // Un'immagine che non si legge (SVG, formati rari): guardata, misure ignote.
        m.larghezza = Some(0);
        m.altezza = Some(0);
        return m;
    }
    if !e_pdf(tipo, dati) {
        return m;
    }
    let file = cartella.join(impronta);
    m.pagine = match sistema::pagine_pdf_esito(&file) {
        Ok(Some(n)) => Some(n),
        Ok(None) => Some(0),
        Err(()) => None,
    };
    let dest = cartella.join(nome_miniatura(impronta));
    m.miniatura = if std::fs::metadata(&dest).is_ok_and(|x| x.len() > 0) {
        Some(true)
    } else if m.pagine == Some(0) {
        // Protetto o rotto: pdftoppm non farebbe meglio di pdfinfo.
        Some(false)
    } else {
        sistema::miniatura_pdf_esito(&file, &dest).ok()
    };
    m
}

/// Completa le misure delle parti già nell'archivio che non le hanno ancora: quelle
/// scaricate prima di questa versione, o quando poppler-utils mancava. Niente rete;
/// da chiamare una volta all'avvio, in un thread qualunque. Restituisce quante ha guardato.
pub fn completa_archivio(db: &Db) -> Risultato<usize> {
    let righe: Vec<(i64, String, String, String)> = db
        .conn()
        .prepare(
            "SELECT a.messaggio_id, a.parte, a.impronta, p.tipo FROM allegati a
             JOIN parti p ON p.messaggio_id = a.messaggio_id AND p.parte = a.parte
             WHERE a.larghezza IS NULL
               AND (lower(p.tipo) LIKE 'image/%' OR a.pagine IS NULL OR a.miniatura IS NULL)",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let cartella = cartella_allegati(db);
    let mut fatte = 0;
    for (messaggio, parte, impronta, tipo) in righe {
        // Per la firma e le intestazioni delle immagini basta l'inizio del file.
        let mut testa = Vec::new();
        let Ok(f) = std::fs::File::open(cartella.join(&impronta)) else {
            continue;
        };
        use std::io::Read;
        if f.take(1024 * 1024).read_to_end(&mut testa).is_err() {
            continue;
        }
        let m = misura(&tipo, &testa, &cartella, &impronta);
        if m == MisureParte::default() {
            // Né immagine né PDF: niente da guardare, la prossima volta si salta.
            db.salva_misure(
                messaggio,
                &parte,
                &MisureParte {
                    larghezza: Some(0),
                    altezza: Some(0),
                    ..MisureParte::default()
                },
            )?;
        } else {
            db.salva_misure(messaggio, &parte, &m)?;
        }
        fatte += 1;
    }
    Ok(fatte)
}

/// Sopra questa dimensione un'immagine ha una miniatura (giro 2): sotto, l'immagine stessa
/// pesa poco anche aperta (un logo, una firma, un'icona).
pub const MINIATURA_DA: i64 = 150 * 1024;

/// Un'immagine dell'archivio che aspetta la miniatura.
#[derive(Debug, Clone)]
pub struct ImmagineDaRidurre {
    pub messaggio: i64,
    pub parte: String,
    /// Il file nell'archivio.
    pub file: PathBuf,
    /// Dove va la miniatura (`nome_miniatura`), accanto al file.
    pub miniatura: PathBuf,
}

/// Le immagini dell'archivio più grandi di `MINIATURA_DA` senza miniatura, al massimo
/// `quanti`, dalle più recenti. La miniatura la fa chi chiama (l'app la fa con gdk-pixbuf),
/// poi lo dice con `miniatura_fatta`: nella vista l'allegato avrà `thumb`, come i PDF.
pub fn immagini_da_ridurre(db: &Db, quanti: usize) -> Risultato<Vec<ImmagineDaRidurre>> {
    let cartella = cartella_allegati(db);
    let righe: Vec<(i64, String, String)> = db
        .conn()
        .prepare_cached(
            "SELECT a.messaggio_id, a.parte, a.impronta FROM allegati a
             JOIN parti p ON p.messaggio_id = a.messaggio_id AND p.parte = a.parte
             WHERE a.miniatura IS NULL AND a.byte >= ?1
               AND lower(p.tipo) LIKE 'image/%' AND lower(p.tipo) NOT LIKE '%svg%'
             ORDER BY a.messaggio_id DESC LIMIT ?2",
        )?
        .query_map(params![MINIATURA_DA, quanti as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    Ok(righe
        .into_iter()
        .map(|(messaggio, parte, impronta)| ImmagineDaRidurre {
            messaggio,
            parte,
            file: cartella.join(&impronta),
            miniatura: cartella.join(nome_miniatura(&impronta)),
        })
        .collect())
}

/// Dopo `immagini_da_ridurre`: la miniatura c'è (`fatta`), oppure l'immagine non si riduce e
/// non si riprova.
pub fn miniatura_fatta(db: &Db, messaggio: i64, parte: &str, fatta: bool) -> Risultato<()> {
    db.salva_misure(messaggio, parte, &MisureParte { miniatura: Some(fatta), ..MisureParte::default() })
}

/// Che cosa si legge dentro un allegato, dal tipo e dal nome del file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dentro {
    Pdf,
    Documento,
    Testo,
    Immagine,
    Niente,
}

fn dentro(tipo: &str, nome_file: Option<&str>) -> Dentro {
    let tipo = tipo.trim().to_ascii_lowercase();
    let est = nome_file
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if tipo == "application/pdf" || est == "pdf" {
        Dentro::Pdf
    } else if tipo.contains("officedocument") || tipo.contains("opendocument")
        || matches!(est.as_str(), "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp")
    {
        Dentro::Documento
    } else if (tipo.starts_with("text/") && !tipo.contains("html"))
        || matches!(est.as_str(), "txt" | "csv" | "md" | "ics" | "json" | "xml" | "log")
    {
        Dentro::Testo
    } else if (tipo.starts_with("image/") && !tipo.contains("svg"))
        || matches!(est.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    {
        Dentro::Immagine
    } else {
        Dentro::Niente
    }
}

/// Legge il testo e i codici di al massimo `quanti` contenuti dell'archivio che non sono
/// ancora stati guardati, dal più recente. Niente rete; i programmi che mancano (pdftotext,
/// unzip, zbarimg) fanno saltare quel tipo di file, che si riprova la volta dopo.
/// Restituisce quanti ne ha guardati.
pub fn completa_contenuti(db: &Db, quanti: usize) -> Risultato<usize> {
    let righe: Vec<(String, String, Option<String>)> = db
        .conn()
        .prepare(
            "SELECT a.impronta, max(p.tipo), max(p.nome_file) FROM allegati a
             JOIN parti p ON p.messaggio_id = a.messaggio_id AND p.parte = a.parte
             JOIN messaggi m ON m.id = a.messaggio_id
             LEFT JOIN contenuti c ON c.impronta = a.impronta
             WHERE c.impronta IS NULL
             GROUP BY a.impronta ORDER BY max(m.data) DESC",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let cartella = cartella_allegati(db);
    let mut assenti: Vec<Dentro> = Vec::new();
    let mut fatti = 0;
    for (impronta, tipo, nome) in righe {
        if fatti >= quanti {
            break;
        }
        let tipo_dentro = dentro(&tipo, nome.as_deref());
        if assenti.contains(&tipo_dentro) {
            continue;
        }
        let file = cartella.join(&impronta);
        if !file.is_file() {
            continue;
        }
        let esito: Result<(Option<String>, Option<String>), ()> = match tipo_dentro {
            Dentro::Pdf => sistema::testo_pdf(&file).map(|t| (t, None)),
            Dentro::Documento => sistema::testo_documento(&file).map(|t| (t, None)),
            Dentro::Testo => {
                use std::io::Read;
                let mut v = Vec::new();
                let letto = std::fs::File::open(&file)
                    .and_then(|f| f.take((sistema::LIMITE_TESTO * 4) as u64).read_to_end(&mut v));
                Ok((letto.ok().map(|_| sistema::normalizza_testo(&String::from_utf8_lossy(&v))), None))
            }
            Dentro::Immagine => sistema::codice_immagine(&file).map(|c| (None, c)),
            Dentro::Niente => Ok((None, None)),
        };
        match esito {
            Ok((testo, codice)) => {
                let testo = testo.filter(|t| !t.is_empty());
                db.salva_contenuto(&impronta, testo.as_deref(), codice.as_deref())?;
                fatti += 1;
            }
            Err(()) => assenti.push(tipo_dentro),
        }
    }
    Ok(fatti)
}

/// Toglie dall'archivio i file temporanei rimasti da un'interruzione brusca (il
/// processo chiuso mentre scriveva): quelli che cominciano con un punto e sono più
/// vecchi di un'ora. I file con il nome giusto sono sempre interi. Restituisce quanti.
pub fn pulisci_temporanei(db: &Db) -> usize {
    let Ok(voci) = std::fs::read_dir(cartella_allegati(db)) else {
        return 0;
    };
    let mut tolti = 0;
    for v in voci.flatten() {
        let vecchio = v
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|d| d > Duration::from_secs(3600));
        if v.file_name().to_string_lossy().starts_with('.')
            && vecchio
            && std::fs::remove_file(v.path()).is_ok()
        {
            tolti += 1;
        }
    }
    tolti
}

/// Apre la cartella del messaggio in sola lettura e controlla che gli UID siano ancora validi.
fn apri_cartella(s: &mut Sessione, pos: &Posizione) -> Risultato<()> {
    let ex = s.examine(&pos.cartella, ModoExamine::Semplice)?;
    if ex.uidvalidity.is_none() || ex.uidvalidity != pos.uidvalidity {
        // Errore::Uso e non di rete: riprovare su un'altra connessione non serve.
        return Err(Errore::Uso(
            "UIDVALIDITY cambiata: serve prima una sincronizzazione".into(),
        ));
    }
    Ok(())
}

/// Decodifica il contenuto di una parte secondo la sua codifica di trasferimento.
pub fn decodifica(codifica: Option<&str>, dati: Vec<u8>) -> Vec<u8> {
    match codifica.map(str::to_ascii_lowercase).as_deref() {
        Some("base64") => {
            let pulito: Vec<u8> = dati
                .iter()
                .copied()
                .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
                .collect();
            mail_parser::decoders::base64::base64_decode(&pulito).unwrap_or(dati)
        }
        Some("quoted-printable") => {
            mail_parser::decoders::quoted_printable::quoted_printable_decode(&dati).unwrap_or(dati)
        }
        // 7bit, 8bit, binary o niente: il contenuto è già quello.
        _ => dati,
    }
}

/// Scrive il contenuto nell'archivio se non c'è già (prima in un file temporaneo,
/// poi lo rinomina, così un file a metà non resta mai con il nome giusto).
fn archivia(cartella: &Path, impronta: &str, dati: &[u8]) -> Risultato<PathBuf> {
    if !cartella.exists() {
        std::fs::create_dir_all(cartella)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(cartella, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let finale = cartella.join(impronta);
    if std::fs::metadata(&finale).is_ok_and(|m| m.len() == dati.len() as u64) {
        return Ok(finale);
    }
    let temporaneo = cartella.join(format!(
        ".{impronta}.{}.{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    {
        let mut f = std::fs::File::create(&temporaneo)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(dati)?;
        f.sync_all()?;
    }
    std::fs::rename(&temporaneo, &finale)?;
    Ok(finale)
}

/// Dopo questo tempo senza richieste la connessione si richiude e la prossima ne
/// apre una nuova: una connessione ferma da molto può essere morta in silenzio
/// (sospensione, cambio di rete) e si aspetterebbe il tempo massimo per saperlo.
const INATTIVITA: Duration = Duration::from_secs(120);

/// Chiude da un altro thread la connessione di un `Richieste`, anche nel mezzo di
/// un download (alla chiusura dell'app): la lettura in corso finisce subito con un
/// errore e il file non entra nell'archivio. Dopo, quel `Richieste` non si collega
/// più: per ricominciare se ne crea uno nuovo.
#[derive(Clone, Debug)]
pub struct Interruttore(pub(crate) Arc<Contatori>);

impl Interruttore {
    pub fn chiudi(&self) {
        self.0.interrompi();
    }
}

/// Com'è andato `Richieste::mantieni`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mantenuta {
    /// Nessuna connessione aperta.
    Nessuna,
    /// Aperta e viva.
    Viva,
    /// Il server l'aveva chiusa: ora è chiusa anche qui.
    ChiusaDalServer,
    /// Il NOOP non ha avuto risposta (rete assente, sospensione): chiusa.
    Caduta(String),
}

/// Le richieste dell'app su una connessione riusata: il primo allegato paga
/// l'accesso (su Gmail più di un secondo), i successivi solo il FETCH. Un errore di
/// rete fa riprovare una volta su una connessione nuova.
///
/// Un `Richieste` per thread, insieme al suo `Db`.
pub struct Richieste {
    cfg: Config,
    contatori: Arc<Contatori>,
    sessione: Option<Sessione>,
    /// Cartella aperta con EXAMINE sulla connessione.
    aperta: Option<String>,
    usata: Instant,
}

impl Richieste {
    pub fn new(cfg: Config) -> Self {
        Richieste {
            cfg,
            contatori: Arc::new(Contatori::default()),
            sessione: None,
            aperta: None,
            usata: Instant::now(),
        }
    }

    /// L'interruttore per chiudere la connessione da un altro thread.
    pub fn interruttore(&self) -> Interruttore {
        Interruttore(self.contatori.clone())
    }

    /// Chiude la connessione, se è aperta: il LOGOUT parte, ma la risposta si aspetta
    /// al massimo 300 ms (`imap::ATTESA_LOGOUT`).
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

    /// Vero se c'è una connessione aperta.
    pub fn collegata(&self) -> bool {
        self.sessione.is_some()
    }

    /// Apre la connessione adesso (accesso compreso), se non c'è: così la prossima
    /// richiesta non paga l'accesso.
    pub fn collega(&mut self) -> Risultato<()> {
        if self.sessione.is_none() {
            self.sessione = Some(sincronizza::apri_sessione(&self.cfg, self.contatori.clone())?);
            self.aperta = None;
            self.usata = Instant::now();
        }
        Ok(())
    }

    /// Da chiamare ogni tanto mentre la connessione è ferma (giro 2): se il server l'ha
    /// chiusa la chiude anche qui, subito, invece di lasciarla in CLOSE-WAIT fino al
    /// prossimo clic; se il server ha scritto, o è ferma da più di `noop_dopo`, manda un
    /// NOOP, che la tiene viva e dice se risponde ancora.
    pub fn mantieni(&mut self, noop_dopo: Duration) -> Mantenuta {
        let Some(s) = self.sessione.as_mut() else {
            return Mantenuta::Nessuna;
        };
        let segnale = s.segnale();
        if segnale == crate::rete::Segnale::Chiusa {
            self.butta();
            return Mantenuta::ChiusaDalServer;
        }
        if segnale == crate::rete::Segnale::Niente && self.usata.elapsed() < noop_dopo {
            return Mantenuta::Viva;
        }
        // una risposta a un NOOP arriva subito: non si tiene ferma la connessione dei clic
        let timeout = s.timeout;
        s.timeout = timeout.min(Duration::from_secs(10));
        let esito = s.noop();
        s.timeout = timeout;
        match esito {
            Ok(_) => {
                self.usata = Instant::now();
                Mantenuta::Viva
            }
            Err(e) => {
                self.butta();
                if segnale == crate::rete::Segnale::Dati {
                    // di solito il «* BYE» prima della chiusura
                    Mantenuta::ChiusaDalServer
                } else {
                    Mantenuta::Caduta(e.to_string())
                }
            }
        }
    }

    /// Esegue `f` con la cartella di `pos` aperta, riprovando una volta su una
    /// connessione nuova se la rete ha dato errore.
    fn con_cartella<T>(
        &mut self,
        pos: &Posizione,
        mut f: impl FnMut(&mut Sessione) -> Risultato<T>,
    ) -> Risultato<T> {
        if self.usata.elapsed() > INATTIVITA {
            self.chiudi();
        }
        let mut tentativo = 0;
        loop {
            tentativo += 1;
            let esito = (|| {
                if self.sessione.is_none() {
                    self.sessione = Some(sincronizza::apri_sessione(
                        &self.cfg,
                        self.contatori.clone(),
                    )?);
                }
                let s = self.sessione.as_mut().expect("sessione appena aperta");
                if self.aperta.as_deref() != Some(pos.cartella.as_str()) {
                    self.aperta = None;
                    apri_cartella(s, pos)?;
                    self.aperta = Some(pos.cartella.clone());
                }
                f(s)
            })();
            self.usata = Instant::now();
            match esito {
                Err(e) if e.di_rete() && tentativo == 1 => self.butta(),
                Err(e) => {
                    if e.di_rete() {
                        self.butta();
                    }
                    return Err(e);
                }
                Ok(v) => return Ok(v),
            }
        }
    }

    /// Il contenuto decodificato di una parte (allegato, immagine incorporata, anche
    /// un testo), dato l'id locale del messaggio e il numero della parte ("2", "1.2").
    /// Se è già nell'archivio non si va in rete; altrimenti si scarica con BODY.PEEK,
    /// a pezzi da 4 MB, e si scrive nell'archivio solo quando è arrivato tutto.
    ///
    /// I byte scaricati contano nella quota del giorno (`scaricamento::quota`). Se la
    /// parte non arriva, si segna fra quelle fallite: la coda dei download in
    /// background la ripropone dopo un'attesa crescente, non subito.
    pub fn parte(&mut self, db: &mut Db, messaggio: i64, parte: &str) -> Risultato<Allegato> {
        let esito = self.parte_dal_server(db, messaggio, parte);
        if esito.is_err()
            && parte_valida(parte)
            && db.parte(messaggio, parte).ok().flatten().is_some()
        {
            // Senza pretese: se anche il database non risponde, l'errore vero è l'altro.
            let _ = db.segna_parte_fallita(messaggio, parte);
        }
        esito
    }

    fn parte_dal_server(
        &mut self,
        db: &mut Db,
        messaggio: i64,
        parte: &str,
    ) -> Risultato<Allegato> {
        if let Some(a) = parte_locale(db, messaggio, parte)? {
            return Ok(a);
        }
        let info = db
            .parte(messaggio, parte)?
            .ok_or_else(|| Errore::Uso("parte sconosciuta per questo messaggio".into()))?;
        let pos = db
            .posizione(messaggio)?
            .ok_or_else(|| Errore::Uso("il messaggio non è più sul server".into()))?;
        let grezzo = self.con_cartella(&pos, |s| {
            let mut dati = Vec::with_capacity(info.byte as usize);
            let mut inizio = 0u64;
            loop {
                let attributo = if info.byte <= PEZZO {
                    format!("(UID BODY.PEEK[{parte}])")
                } else {
                    format!("(UID BODY.PEEK[{parte}]<{inizio}.{PEZZO}>)")
                };
                let mut arrivato: Option<Vec<u8>> = None;
                s.uid_fetch(&pos.uid.to_string(), &attributo, None, |d| {
                    if let Some(d) = d
                        && d.uid == Some(pos.uid)
                        && let Some((_, b)) = d.sezioni.into_iter().find(|(n, _)| n == parte)
                    {
                        arrivato = Some(b);
                    }
                })?;
                let pezzo = arrivato.ok_or_else(|| {
                    Errore::Uso("il server non ha mandato la parte chiesta".into())
                })?;
                let n = pezzo.len() as u64;
                dati.extend_from_slice(&pezzo);
                inizio += n;
                if info.byte <= PEZZO || n < PEZZO {
                    return Ok(dati);
                }
            }
        })?;
        let _ = db.aggiungi_scaricati(grezzo.len() as u64);
        let dati = decodifica(info.codifica.as_deref(), grezzo);
        let impronta = impronta::sha256_esadecimale(&dati);
        let cartella = cartella_allegati(db);
        let percorso = archivia(&cartella, &impronta, &dati)?;
        db.salva_allegato(messaggio, parte, &impronta, dati.len() as u64)?;
        let m = misura(&info.tipo, &dati, &cartella, &impronta);
        if m != MisureParte::default() {
            db.salva_misure(messaggio, parte, &m)?;
        }
        Ok(Allegato {
            percorso,
            tipo: info.tipo,
            nome_file: info.nome_file,
            byte: dati.len() as u64,
            impronta,
        })
    }

    /// Scarica il testo (semplice e HTML) dei messaggi indicati che non lo hanno
    /// ancora; gli altri si saltano. Restituisce quanti testi ha salvato.
    pub fn scarica_testi(&mut self, db: &mut Db, messaggi: &[i64]) -> Risultato<usize> {
        let lista = db.corpi_per_messaggi(messaggi)?;
        let mut per_cartella: BTreeMap<String, (Posizione, Vec<CorpoDaScaricare>)> =
            BTreeMap::new();
        for (pos, corpo) in lista {
            per_cartella
                .entry(pos.cartella.clone())
                .or_insert_with(|| (pos, Vec::new()))
                .1
                .push(corpo);
        }
        let opz = Opzioni::default();
        let mut r = Rapporto::default();
        for (pos, corpi) in per_cartella.values() {
            let esito = self.con_cartella(pos, |s| {
                let e = sincronizza::scarica_lista_corpi(s, db, corpi, &opz, &mut r);
                if e.is_err() {
                    // Il lotto a metà si annulla prima di riprovare.
                    db.annulla();
                }
                e
            });
            if let Err(e) = esito {
                db.annulla();
                return Err(e);
            }
        }
        Ok(r.corpi)
    }
}

impl Richieste {
    /// Il messaggio intero così com'è sul server, per «Salva»: `UID FETCH <uid>
    /// BODY.PEEK[]` (sola lettura, non lo segna come letto), a pezzi da 4 MB se è
    /// grande. Niente si salva nell'archivio: i byte vanno all'app, che li scrive
    /// dove vuole (con il nome di `nome_eml_messaggio`). Contano nella quota del giorno.
    pub fn messaggio_intero(&mut self, db: &mut Db, messaggio: i64) -> Risultato<Vec<u8>> {
        let pos = db
            .posizione(messaggio)?
            .ok_or_else(|| Errore::Uso("il messaggio non è più sul server".into()))?;
        let dimensione: Option<u64> = db
            .conn()
            .query_row(
                "SELECT dimensione FROM messaggi WHERE id = ?1",
                params![messaggio],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten()
            .map(|n| n.max(0) as u64);
        // A pezzi se è grande o se la misura non si sa.
        let intero_in_una = dimensione.is_some_and(|d| d <= PEZZO);
        let dati = self.con_cartella(&pos, |s| {
            let mut dati = Vec::with_capacity(dimensione.unwrap_or(0) as usize);
            let mut inizio = 0u64;
            loop {
                let attributo = if intero_in_una {
                    "(UID BODY.PEEK[])".to_string()
                } else {
                    format!("(UID BODY.PEEK[]<{inizio}.{PEZZO}>)")
                };
                let mut arrivato: Option<Vec<u8>> = None;
                s.uid_fetch(&pos.uid.to_string(), &attributo, None, |d| {
                    if let Some(d) = d
                        && d.uid == Some(pos.uid)
                        && d.intero.is_some()
                    {
                        arrivato = d.intero;
                    }
                })?;
                let pezzo = arrivato.ok_or_else(|| {
                    Errore::Uso("il server non ha mandato il messaggio chiesto".into())
                })?;
                let n = pezzo.len() as u64;
                dati.extend_from_slice(&pezzo);
                inizio += n;
                if intero_in_una || n < PEZZO {
                    return Ok(dati);
                }
            }
        })?;
        let _ = db.aggiungi_scaricati(dati.len() as u64);
        Ok(dati)
    }
}

/// Il nome del file per «Salva», come nel prototipo: `{id}-{oggetto}.eml`.
///
/// - `id`: i primi 6 caratteri esadecimali dell'MD5 del Message-ID senza parentesi
///   angolari e senza spazi intorno; senza Message-ID, dell'unione con `\n` di data
///   (ISO 8601, come `date` nella vista), indirizzo del mittente e oggetto.
/// - `oggetto`: i caratteri vietati nei nomi di file (`/ \ : * ? " < > |`) e quelli
///   di controllo diventano spazi, gli spazi di fila uno solo; oltre 60 caratteri si
///   accorcia senza spezzare una parola. «Re:» e «I:» restano (senza i due punti).
///   Vuoto: `senza oggetto`.
pub fn nome_eml(
    message_id: Option<&str>,
    data: Option<&str>,
    mittente: Option<&str>,
    oggetto: Option<&str>,
) -> String {
    let mid = message_id
        .unwrap_or("")
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    let base = if mid.is_empty() {
        [
            data.unwrap_or(""),
            mittente.unwrap_or(""),
            oggetto.unwrap_or(""),
        ]
        .join("\n")
    } else {
        mid.to_string()
    };
    let id = &impronta::md5_esadecimale(base.as_bytes())[..6];
    let pulito: String = oggetto
        .unwrap_or("")
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut oggetto = pulito.split_whitespace().collect::<Vec<_>>().join(" ");
    if oggetto.chars().count() > 60 {
        // I primi 61 caratteri, poi via l'ultima parola se è tagliata (o lo spazio finale).
        let primi: String = oggetto.chars().take(61).collect();
        let corto = match primi.rfind(char::is_whitespace) {
            Some(i) => primi[..i].trim_end().to_string(),
            None => String::new(),
        };
        oggetto = if corto.is_empty() {
            oggetto.chars().take(60).collect()
        } else {
            corto
        };
    }
    if oggetto.is_empty() {
        oggetto = "senza oggetto".into();
    }
    format!("{id}-{oggetto}.eml")
}

/// `nome_eml` per un messaggio del database (Message-ID, data, primo mittente, oggetto).
pub fn nome_eml_messaggio(db: &Db, messaggio: i64) -> Risultato<String> {
    let (mid, data, oggetto): (Option<String>, Option<i64>, Option<String>) = db
        .conn()
        .query_row(
            "SELECT message_id, data, oggetto FROM messaggi WHERE id = ?1",
            params![messaggio],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| Errore::Uso("messaggio sconosciuto".into()))?;
    let mittente: Option<String> = db
        .conn()
        .query_row(
            "SELECT i.email FROM partecipanti p JOIN indirizzi i ON i.id = p.indirizzo_id
             WHERE p.messaggio_id = ?1 AND p.ruolo = 'da' ORDER BY p.posizione LIMIT 1",
            params![messaggio],
            |r| r.get(0),
        )
        .optional()?;
    Ok(nome_eml(
        mid.as_deref(),
        data.map(tempo::iso).as_deref(),
        mittente.as_deref(),
        oggetto.as_deref(),
    ))
}

impl Drop for Richieste {
    fn drop(&mut self) {
        self.chiudi();
    }
}

/// `Richieste::parte` su una connessione usata una volta sola.
pub fn parte(cfg: &Config, db: &mut Db, messaggio: i64, parte: &str) -> Risultato<Allegato> {
    Richieste::new(cfg.clone()).parte(db, messaggio, parte)
}

/// `Richieste::scarica_testi` su una connessione usata una volta sola.
pub fn scarica_testi(cfg: &Config, db: &mut Db, messaggi: &[i64]) -> Risultato<usize> {
    Richieste::new(cfg.clone()).scarica_testi(db, messaggi)
}

#[cfg(test)]
mod prove {
    use super::*;

    /// Giro 2: le immagini grandi aspettano la miniatura, le piccole e gli SVG no; una volta
    /// detto com'è andata non tornano più.
    #[test]
    fn immagini_da_ridurre_e_fatte() {
        let dir = std::env::temp_dir().join(format!("motore-riduci-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::apri(&dir.join("prova.sqlite")).unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO messaggi(id, chiave, aggiunto) VALUES (1, 'a', 0), (2, 'b', 0);
                 INSERT INTO parti(messaggio_id, parte, tipo, ruolo) VALUES
                   (1, '2', 'image/jpeg', 'allegato'), (1, '3', 'image/png', 'allegato'),
                   (2, '2', 'image/svg+xml', 'allegato'), (2, '3', 'IMAGE/JPEG', 'allegato'),
                   (2, '4', 'application/pdf', 'allegato');
                 INSERT INTO allegati(messaggio_id, parte, impronta, byte, scaricato) VALUES
                   (1, '2', 'grande', 2000000, 0), (1, '3', 'piccola', 20000, 0),
                   (2, '2', 'svg', 900000, 0), (2, '3', 'grande', 2000000, 0),
                   (2, '4', 'pdf', 900000, 0);",
            )
            .unwrap();
        let v = immagini_da_ridurre(&db, 10).unwrap();
        let chi: Vec<(i64, &str)> = v.iter().map(|i| (i.messaggio, i.parte.as_str())).collect();
        assert_eq!(chi, vec![(2, "3"), (1, "2")], "le grandi, dalla più recente");
        assert!(v[0].miniatura.ends_with(nome_miniatura("grande")) && v[0].file.ends_with("grande"));
        miniatura_fatta(&db, 2, "3", true).unwrap();
        miniatura_fatta(&db, 1, "2", false).unwrap();
        assert!(immagini_da_ridurre(&db, 10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn numeri_di_parte() {
        assert!(parte_valida("1"));
        assert!(parte_valida("1.2.10"));
        assert!(!parte_valida(""));
        assert!(!parte_valida("1..2"));
        assert!(!parte_valida("0"));
        assert!(!parte_valida("1]<0.9> X"));
        assert!(!parte_valida("../x"));
    }

    #[test]
    fn nomi_dei_file_eml() {
        // L'esempio delle note: Message-ID senza parentesi e senza spazi intorno.
        assert!(
            nome_eml(
                Some(" <case-inoltro-0@esempio.it> "),
                None,
                None,
                Some("Preventivo impianto")
            )
            .starts_with("4f6f3f-")
        );
        assert_eq!(
            nome_eml(
                Some("case-inoltro-0@esempio.it"),
                None,
                None,
                Some("Preventivo impianto")
            ),
            "4f6f3f-Preventivo impianto.eml"
        );
        // Senza Message-ID: data, mittente e oggetto uniti da \n (confrontato con md5sum).
        assert_eq!(
            nome_eml(
                None,
                Some("2026-10-27T09:00:00.000Z"),
                Some("alice@esempio.it"),
                Some("Preventivo impianto")
            ),
            "005bea-Preventivo impianto.eml"
        );
        let n = |o: &str| {
            let s = nome_eml(Some("x@y"), None, None, Some(o));
            s[7..s.len() - 4].to_string()
        };
        assert_eq!(n("Re: I: conto / fattura?"), "Re I conto fattura");
        assert_eq!(n("a\tb\r\nc   d"), "a b c d");
        assert_eq!(n(r#"<"x"> | *y*"#), "x y");
        assert_eq!(n(""), "senza oggetto");
        assert_eq!(n(" ::: "), "senza oggetto");
        // Oltre 60 caratteri: si accorcia a fine parola.
        let lungo = "Verbale della riunione di condominio del dodici ottobre con allegati";
        assert_eq!(
            n(lungo),
            "Verbale della riunione di condominio del dodici ottobre con"
        );
        assert!(n(lungo).chars().count() <= 60);
        // Il 61° carattere è uno spazio: restano esattamente 60 caratteri.
        let giusto = format!("{} parola", "a".repeat(60));
        assert_eq!(n(&giusto), "a".repeat(60));
        // Una parola sola più lunga di 60: si taglia a 60.
        assert_eq!(n(&"è".repeat(80)), "è".repeat(60));
        assert_eq!(
            n("Perché sì — preventivo n° 3"),
            "Perché sì — preventivo n° 3"
        );
    }

    #[test]
    fn pdf_riconosciuti() {
        assert!(e_pdf("application/PDF", b""));
        assert!(e_pdf("application/octet-stream", b"%PDF-1.7\n"));
        assert!(e_pdf("application/octet-stream", b"\xef\xbb\xbf  %PDF-1.4"));
        assert!(!e_pdf("image/png", b"\x89PNG"));
        assert!(nome_di_contenuto(&"a".repeat(64)));
        assert!(!nome_di_contenuto(&nome_miniatura(&"a".repeat(64))));
        assert!(!nome_di_contenuto(".tmp"));
    }

    #[test]
    fn decodifiche() {
        assert_eq!(
            decodifica(Some("base64"), b"Q2lh\r\nbyDDqA==\r\n".to_vec()),
            "Ciao è".as_bytes()
        );
        assert_eq!(
            decodifica(
                Some("Quoted-Printable"),
                b"Perch=C3=A9 s=\r\n=C3=AC".to_vec()
            ),
            "Perché sì".as_bytes()
        );
        assert_eq!(decodifica(Some("7bit"), b"abc".to_vec()), b"abc");
        assert_eq!(decodifica(None, b"abc".to_vec()), b"abc");
    }
}
