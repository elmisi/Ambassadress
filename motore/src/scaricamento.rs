//! Gli allegati in background (proposta 6 delle note): quello che serve all'app per
//! scaricarli tutti a poco a poco, un pezzo alla volta sulla sua connessione delle
//! richieste (`su_richiesta::Richieste::parte`), così un clic passa sempre davanti.
//!
//! - `coda`: le prossime parti da scaricare, dal messaggio più recente al più
//!   vecchio, con in testa quelle che l'interfaccia ha sullo schermo. Le parti
//!   fallite tornano solo dopo un'attesa crescente (`db::attesa_dopo_fallimento`).
//! - `quota`: i byte scaricati oggi (UTC) rispetto a un limite; contano anche gli
//!   allegati aperti a mano e i messaggi interi di «Salva».
//! - `pausa`: se adesso conviene fermarsi (quota finita, connessione a consumo,
//!   meno di 5 GB liberi sul disco dei dati).
//! - `totali`: quanti allegati ci sono e quanto pesano, scaricati e no, per tipo e
//!   per anno. Solo numeri: li stampa anche `motore allegati`.
//!
//! Un download interrotto non lascia mai un file a metà nell'archivio: la parte
//! arriva tutta in memoria, poi si scrive in un file temporaneo e si rinomina.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};

use crate::db::Db;
use crate::errore::Risultato;
use crate::{sistema, tempo};

/// La quota giornaliera che l'app usa: 1 GB (Gmail blocca oltre circa 2,5 GB al giorno).
pub const QUOTA_CONSIGLIATA: u64 = 1024 * 1024 * 1024;

/// Sotto questo spazio libero sul disco dei dati i download in background si fermano.
pub const SPAZIO_MINIMO: u64 = 5 * 1024 * 1024 * 1024;

/// Le parti che contano come allegati da scaricare: allegati e immagini incorporate.
/// I testi li gestisce la sincronizzazione; le «altre» parti (calendario, rapporti di
/// consegna) l'interfaccia non le mostra come allegati.
const RUOLI: &str = "('allegato', 'incorporato')";

/// Una parte da scaricare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaScaricare {
    /// Id locale del messaggio (`localId` nella vista).
    pub messaggio: i64,
    pub parte: String,
    pub tipo: String,
    /// Byte decodificati stimati dalla dimensione sul server.
    pub byte: u64,
}

fn da_scaricare(r: &rusqlite::Row<'_>) -> rusqlite::Result<DaScaricare> {
    let byte: i64 = r.get::<_, Option<i64>>(3)?.unwrap_or(0);
    let codifica: Option<String> = r.get(4)?;
    Ok(DaScaricare {
        messaggio: r.get(0)?,
        parte: r.get(1)?,
        tipo: r.get(2)?,
        byte: crate::vista::decodificati(byte, codifica.as_deref()).max(0) as u64,
    })
}

/// Le condizioni comuni: parte vera, messaggio ancora sul server, non nell'archivio,
/// non fallita da poco.
fn condizioni() -> String {
    format!(
        "p.ruolo IN {RUOLI} AND m.eliminato IS NULL
         AND EXISTS (SELECT 1 FROM posizioni x WHERE x.messaggio_id = m.id)
         AND NOT EXISTS (SELECT 1 FROM allegati a WHERE a.messaggio_id = p.messaggio_id AND a.parte = p.parte)
         AND NOT EXISTS (SELECT 1 FROM parti_fallite f WHERE f.messaggio_id = p.messaggio_id
                         AND f.parte = p.parte AND f.prossimo > ?1)"
    )
}

/// Le prossime `quante` parti da scaricare. Prima quelle di `in_testa` (nell'ordine
/// dato: l'interfaccia ci mette quelle che ha sullo schermo), se servono ancora;
/// poi tutte le altre dal messaggio più recente al più vecchio. Le parti fallite
/// non tornano prima della loro attesa, neanche se sono in testa: un clic su un
/// allegato chiama comunque `Richieste::parte` direttamente.
pub fn coda(db: &Db, quante: usize, in_testa: &[(i64, String)]) -> Risultato<Vec<DaScaricare>> {
    let conn = db.conn();
    let ora = tempo::adesso();
    let colonne = "p.messaggio_id, p.parte, p.tipo, p.byte, p.codifica";
    let mut out: Vec<DaScaricare> = Vec::new();
    {
        let mut st = conn.prepare(&format!(
            "SELECT {colonne} FROM parti p JOIN messaggi m ON m.id = p.messaggio_id
             WHERE {} AND p.messaggio_id = ?2 AND p.parte = ?3",
            condizioni()
        ))?;
        for (m, p) in in_testa {
            if out.len() >= quante {
                return Ok(out);
            }
            if out.iter().any(|d| d.messaggio == *m && d.parte == *p) {
                continue;
            }
            if let Some(d) = st.query_row(params![ora, m, p], da_scaricare).optional()? {
                out.push(d);
            }
        }
    }
    if out.len() >= quante {
        return Ok(out);
    }
    let mut st = conn.prepare(&format!(
        "SELECT {colonne} FROM parti p JOIN messaggi m ON m.id = p.messaggio_id
         WHERE {}
         ORDER BY COALESCE(m.data_interna, m.data) DESC, m.id DESC, length(p.parte), p.parte
         LIMIT ?2",
        condizioni()
    ))?;
    let resto = (quante - out.len() + in_testa.len()) as i64;
    let righe: Vec<DaScaricare> = st
        .query_map(params![ora, resto], da_scaricare)?
        .collect::<Result<_, _>>()?;
    for d in righe {
        if out.len() >= quante {
            break;
        }
        if !out
            .iter()
            .any(|x| x.messaggio == d.messaggio && x.parte == d.parte)
        {
            out.push(d);
        }
    }
    Ok(out)
}

/// I byte scaricati oggi rispetto al limite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quota {
    /// Il giorno (UTC) a cui si riferisce: `2026-10-28`.
    pub giorno: String,
    pub scaricati: u64,
    pub limite: u64,
    /// Quanti byte si possono ancora scaricare oggi (0 se il limite è passato).
    pub restano: u64,
}

/// La quota di oggi. Il giorno è quello UTC: in Italia ricomincia all'una o alle due di notte.
pub fn quota(db: &Db, limite: u64) -> Risultato<Quota> {
    let giorno = tempo::giorno(tempo::adesso());
    let scaricati = db.scaricati_il(&giorno)?;
    Ok(Quota {
        giorno,
        scaricati,
        limite,
        restano: limite.saturating_sub(scaricati),
    })
}

/// Perché i download in background sono fermi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pausa {
    /// Quota del giorno finita: si riprende domani.
    Quota,
    /// NetworkManager dice che la connessione è a consumo.
    AConsumo,
    /// Meno di `SPAZIO_MINIMO` liberi sul disco del database.
    Disco,
}

/// Se adesso i download in background devono aspettare, e perché. Guarda la quota
/// (`limite` byte al giorno), la connessione (`busctl`) e lo spazio libero (`df`)
/// sul disco del database. Se `busctl` o `df` non rispondono, quella condizione non ferma.
pub fn pausa(db: &Db, limite: u64) -> Risultato<Option<Pausa>> {
    if quota(db, limite)?.restano == 0 {
        return Ok(Some(Pausa::Quota));
    }
    if sistema::connessione_a_consumo() {
        return Ok(Some(Pausa::AConsumo));
    }
    let cartella = db
        .percorso
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    if sistema::spazio_libero(cartella).is_some_and(|b| b < SPAZIO_MINIMO) {
        return Ok(Some(Pausa::Disco));
    }
    Ok(None)
}

/// I tipi di allegato dei totali.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Categoria {
    Pdf,
    Immagini,
    Documenti,
    Fogli,
    Archivi,
    Altro,
}

impl Categoria {
    pub const TUTTE: [Categoria; 6] = [
        Categoria::Pdf,
        Categoria::Immagini,
        Categoria::Documenti,
        Categoria::Fogli,
        Categoria::Archivi,
        Categoria::Altro,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Categoria::Pdf => "PDF",
            Categoria::Immagini => "immagini",
            Categoria::Documenti => "documenti",
            Categoria::Fogli => "fogli",
            Categoria::Archivi => "archivi",
            Categoria::Altro => "altro",
        }
    }
}

/// Il tipo di un allegato, dal tipo MIME e dall'estensione del nome.
pub fn categoria(tipo: &str, nome_file: Option<&str>) -> Categoria {
    let t = tipo.to_ascii_lowercase();
    let est = nome_file
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let ha = |parole: &[&str]| parole.iter().any(|p| t.contains(p));
    let estensione = |lista: &[&str]| lista.contains(&est.as_str());
    if t == "application/pdf" || est == "pdf" {
        Categoria::Pdf
    } else if t.starts_with("image/")
        || estensione(&[
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "tif", "tiff", "svg",
        ])
    {
        Categoria::Immagini
    } else if ha(&["spreadsheet", "excel", "text/csv", "tab-separated-values"])
        || estensione(&["xls", "xlsx", "xlsm", "ods", "csv", "tsv", "numbers"])
    {
        Categoria::Fogli
    } else if ha(&[
        "zip",
        "x-7z",
        "rar",
        "gzip",
        "x-tar",
        "x-bzip",
        "x-xz",
        "compressed",
    ]) || estensione(&["zip", "7z", "rar", "gz", "tgz", "tar", "bz2", "xz"])
    {
        Categoria::Archivi
    } else if ha(&[
        "msword",
        "wordprocessing",
        "opendocument.text",
        "presentation",
        "powerpoint",
        "rtf",
        "text/plain",
        "text/markdown",
    ]) || estensione(&[
        "doc", "docx", "odt", "rtf", "txt", "md", "pages", "ppt", "pptx", "odp", "key",
    ]) {
        Categoria::Documenti
    } else {
        Categoria::Altro
    }
}

/// Numero e byte di un gruppo di allegati, scaricati e da scaricare.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Conto {
    pub scaricati: u64,
    pub byte_scaricati: u64,
    pub da_scaricare: u64,
    /// Stimati dalla dimensione sul server (base64 tolto).
    pub byte_da_scaricare: u64,
}

impl Conto {
    pub fn numero(&self) -> u64 {
        self.scaricati + self.da_scaricare
    }
    pub fn byte(&self) -> u64 {
        self.byte_scaricati + self.byte_da_scaricare
    }
    fn aggiungi(&mut self, scaricato: Option<u64>, stimati: u64) {
        match scaricato {
            Some(b) => {
                self.scaricati += 1;
                self.byte_scaricati += b;
            }
            None => {
                self.da_scaricare += 1;
                self.byte_da_scaricare += stimati;
            }
        }
    }
}

/// I totali degli allegati (allegati e immagini incorporate dei messaggi ancora sul server).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Totali {
    pub tutti: Conto,
    /// Sempre tutte le categorie, nell'ordine di `Categoria::TUTTE`.
    pub per_tipo: Vec<(Categoria, Conto)>,
    /// Per anno (UTC) del messaggio, dal più recente.
    pub per_anno: Vec<(i64, Conto)>,
    /// Byte dei file nell'archivio: lo stesso contenuto in più email conta una volta.
    pub archivio: u64,
    /// Parti fallite che aspettano il prossimo tentativo.
    pub in_attesa: u64,
}

/// I totali degli allegati, solo numeri.
pub fn totali(db: &Db) -> Risultato<Totali> {
    let conn = db.conn();
    let mut st = conn.prepare(&format!(
        "SELECT p.tipo, p.nome_file, p.byte, p.codifica, a.byte, COALESCE(m.data_interna, m.data)
         FROM parti p JOIN messaggi m ON m.id = p.messaggio_id
         LEFT JOIN allegati a ON a.messaggio_id = p.messaggio_id AND a.parte = p.parte
         WHERE p.ruolo IN {RUOLI} AND m.eliminato IS NULL"
    ))?;
    let mut tutti = Conto::default();
    let mut per_tipo: BTreeMap<Categoria, Conto> = BTreeMap::new();
    let mut per_anno: BTreeMap<i64, Conto> = BTreeMap::new();
    let mut q = st.query([])?;
    while let Some(r) = q.next()? {
        let tipo: String = r.get(0)?;
        let nome: Option<String> = r.get(1)?;
        let byte: i64 = r.get::<_, Option<i64>>(2)?.unwrap_or(0);
        let codifica: Option<String> = r.get(3)?;
        let scaricato: Option<u64> = r.get::<_, Option<i64>>(4)?.map(|b| b.max(0) as u64);
        let quando: Option<i64> = r.get(5)?;
        let stimati = crate::vista::decodificati(byte, codifica.as_deref()).max(0) as u64;
        tutti.aggiungi(scaricato, stimati);
        per_tipo
            .entry(categoria(&tipo, nome.as_deref()))
            .or_default()
            .aggiungi(scaricato, stimati);
        per_anno
            .entry(quando.map(tempo::anno).unwrap_or(0))
            .or_default()
            .aggiungi(scaricato, stimati);
    }
    let archivio: i64 = conn.query_row(
        "SELECT IFNULL(sum(byte), 0) FROM (SELECT impronta, max(byte) AS byte FROM allegati GROUP BY impronta)",
        [],
        |r| r.get(0),
    )?;
    let in_attesa: i64 = conn.query_row(
        "SELECT count(*) FROM parti_fallite f JOIN parti p
           ON p.messaggio_id = f.messaggio_id AND p.parte = f.parte
         WHERE f.prossimo > ?1",
        params![tempo::adesso()],
        |r| r.get(0),
    )?;
    Ok(Totali {
        tutti,
        per_tipo: Categoria::TUTTE
            .iter()
            .map(|c| (*c, per_tipo.get(c).copied().unwrap_or_default()))
            .collect(),
        per_anno: per_anno.into_iter().rev().collect(),
        archivio: archivio.max(0) as u64,
        in_attesa: in_attesa.max(0) as u64,
    })
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn categorie() {
        use Categoria::*;
        let c = |t: &str, n: &str| categoria(t, Some(n));
        assert_eq!(c("application/pdf", "x"), Pdf);
        assert_eq!(c("application/octet-stream", "Fattura.PDF"), Pdf);
        assert_eq!(c("image/jpeg", "foto"), Immagini);
        assert_eq!(c("application/octet-stream", "IMG_0001.HEIC"), Immagini);
        assert_eq!(
            c(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "a"
            ),
            Fogli
        );
        assert_eq!(
            c("application/vnd.oasis.opendocument.spreadsheet", "a"),
            Fogli
        );
        assert_eq!(c("text/csv", "a"), Fogli);
        assert_eq!(
            c(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "a"
            ),
            Documenti
        );
        assert_eq!(c("application/vnd.ms-powerpoint", "a"), Documenti);
        assert_eq!(c("text/plain", "note"), Documenti);
        assert_eq!(c("application/zip", "a"), Archivi);
        assert_eq!(c("application/x-zip-compressed", "a"), Archivi);
        assert_eq!(c("application/octet-stream", "foto.7z"), Archivi);
        assert_eq!(c("application/ics", "invito.ics"), Altro);
        assert_eq!(categoria("application/octet-stream", None), Altro);
    }
}
