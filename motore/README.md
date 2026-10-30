# Il motore: banco di prova

Primo passo del piano in `docs/implementazione.md`. Una libreria con un eseguibile da riga di comando, senza interfaccia, che sincronizza una casella IMAP in un database SQLite locale e misura tutto, più le funzioni che servono all'app desktop (sotto, «API per l'app») e un finto server IMAP con la casella sintetica del prototipo. **Questa versione è solo lettura**: non cambia niente sul server, la scrittura arriva dopo.

## Come si lancia

```bash
cd motore
cargo build --release
./target/release/motore sincronizza --limite 200   # prima volta: i 200 più recenti
./target/release/motore sincronizza --limite 200   # di nuovo: solo le differenze
./target/release/motore stato                      # conteggi del database locale
./target/release/motore cerca preventivo           # quanti messaggi trova la ricerca
./target/release/motore ascolta --durata 600       # IDLE per dieci minuti, poi si ferma
./target/release/motore allegati                   # totali degli allegati, solo numeri
```

| comando | cosa fa |
| --- | --- |
| `sincronizza` (o `sync`) | scarica e aggiorna la cartella con tutti i messaggi |
| `stato` | conteggi del database: messaggi, thread, persone, allegati, testi, ultimi giri |
| `cerca PAROLE` | quanti messaggi trova la ricerca FTS5 (solo il numero) |
| `ascolta` | sincronizza, poi resta in IDLE e sincronizza quando arriva qualcosa |
| `allegati` | totali degli allegati: numero e peso, scaricati e da scaricare, per tipo e per anno (solo il database, niente rete) |

Opzioni principali: `--limite N` (quanti messaggi recenti tenere sincronizzati, predefinito 1000), `--tutti`, `--corpi N` (per quanti dei più recenti scaricare il testo, predefinito 100), `--cartella NOME`, `--lotto N`, `--timeout SECONDI`, `--db FILE`, `--server HOST[:PORTA]`, `--credenziali FILE`. Per `ascolta`: `--controllo SECONDI` (predefinito 240) e `--durata SECONDI`. `motore aiuto` le elenca tutte.

- **Credenziali**: il programma legge `~/.secrets/gmail-user.txt` e `~/.secrets/gmail-app-password.txt` (le stesse di `tools/gmail-export.py`). Con `--credenziali FILE` legge invece un file di due righe, utente e password, per le prove. Non le stampa mai, neanche negli errori.
- **Database**: `motore/private/ambassadress.sqlite` (in `.gitignore`, cartella 0700, file 0600).
- **Uscita**: solo conteggi, tempi, byte, estensioni del server ed errori. Mai oggetti, indirizzi, nomi, etichette o testi. Una prova automatica (`tests/riga_di_comando.rs`) lo controlla sull'output vero.

## API per l'app

Tutto è I/O bloccante e si può chiamare da un thread qualunque. **Un `Db` per thread** (`Db::apri(percorso)`): SQLite in WAL lascia leggere la vista mentre il servizio scrive.

La configurazione è la stessa della riga di comando:

```rust
use motore::sincronizza::Config;
use motore::rete::{Indirizzo, Sicurezza};
let cfg = Config {
    indirizzo: Indirizzo::gmail(),          // oppure { host, porta, sicurezza: Sicurezza::InChiaro } verso 127.0.0.1
    credenziali: motore::credenziali::leggi(Some(&file))?,   // Credenziali { utente: Segreto, password: Segreto }
    timeout: Duration::from_secs(60),
};
```

`Sicurezza::InChiaro` la libreria non la limita: è l'app a doverla usare solo verso 127.0.0.1, come fa la riga di comando.

### Servizio in background e notizie (`motore::servizio`, `motore::sincronizza`)

```rust
pub struct OpzioniServizio {           // Default
    pub sincronizza: sincronizza::Opzioni,   // limite, corpi, lotti… (come la riga di comando)
    pub ascolto: ascolta::OpzioniAscolto,    // controllo dell'IDLE, rinnovo, attese
    pub attesa_minima: Option<Duration>,     // fra un tentativo e l'altro offline (1 s)
    pub attesa_massima: Option<Duration>,    // (5 min)
    pub giro_ogni: Option<Duration>,         // giro 2: un giro anche senza novità (l'app: 3 min)
}
impl Servizio {
    pub fn avvia(cfg: Config, percorso_db: PathBuf, opz: OpzioniServizio, notizie: Sender<Notizia>) -> Servizio;
    pub fn sincronizza_ora(&self);           // anche mentre l'ascolto è in IDLE
    pub fn ferma(self);                      // in meno di mezzo secondo; anche il Drop lo ferma
}
pub const INTERVALLO_CONTATTO: Duration;     // 60 s

pub enum Fase { Intestazioni, Testi }
pub enum Notizia {
    Inizio,                                              // comincia un giro
    Passo { fase: Fase, fatti: usize, totale: usize },   // «Scarico la posta, X di N»
    Fine { nuovi: usize, aggiornati: usize, spariti: usize, testi: usize },
    Offline { motivo: String, riprovo_tra: Option<Duration> },   // «Offline»
    AccessoRifiutato { motivo: String },                 // «La password non funziona più»
    Errore { motivo: String },                           // database, guardia, uso
    InLinea,                                             // IDLE attivo: la posta nuova arriva da sola
    Contatto { alle: i64 },                              // il server ha risposto (secondi Unix)
}
```

Il servizio sincronizza subito, poi apre l'IDLE e risincronizza a ogni novità (quelle arrivate insieme diventano un giro solo). Dopo un errore di rete manda `Offline` e riprova con attese crescenti; quando l'IDLE si ricollega manda `InLinea` e risincronizza. Se il server rifiuta utente o password manda `AccessoRifiutato` una volta e si ferma: niente tentativi a ripetizione, che su Gmail possono bloccare l'account. Per riprovare l'app avvia un servizio nuovo con le credenziali nuove.

**L'ultimo contatto** (proposta 2 delle note): `Contatto { alle }` arriva dopo un giro riuscito, quando l'IDLE si collega e a ogni controllo periodico dell'ascolto (DONE + NOOP, che è anche il rinnovo dell'IDLE), al massimo una ogni `INTERVALLO_CONTATTO`. Per l'ora dopo un aggiornamento a mano conta anche `Fine`, che può arrivare meno di un minuto dopo l'ultimo `Contatto`. `sincronizza_ora` fa un giro subito anche con l'IDLE aperto: usa la connessione di lavoro, l'ascolto resta dov'è (provato: Inizio e Fine in 0,1–0,2 s sul finto server, nessuna connessione nuova).

**Chiusura veloce** (proposta 10): `ferma` chiude la lettura dei socket di ascolto e di lavoro (`Contatori::interrompi`, `shutdown` di lettura: chi aspetta il server si sveglia subito), manda DONE e LOGOUT senza aspettare la risposta oltre 300 ms (`imap::ATTESA_LOGOUT`) e aspetta i thread al massimo 450 ms; se non hanno finito finiscono da soli poco dopo, e una transazione SQLite a metà si annulla da sola. Misurato sul finto server: **0,9 ms** con l'IDLE aperto, **0,4 ms** a metà della prima sincronizzazione con il server lento (0,4 s per risposta). Le connessioni aperte dopo `interrompi` si rifiutano, e un giro interrotto non manda `Offline`. Il LOGOUT parte sempre; se il server lo riceve dipende da cosa arriva prima: chiudendo il socket con la risposta al DONE ancora da leggere, il sistema manda un RST e il server può perderlo (succede sul finto server con quello dell'ascolto).

Chi usa `sincronizza::sincronizza` direttamente riceve le stesse notizie (tranne `InLinea` e `Contatto`) mettendo un `Sender<Notizia>` in `Opzioni::notizie`; `Opzioni::stop` (un `Arc<AtomicBool>`) gli dice di non riprovare più. La riga di comando non cambia.

### Testi e allegati su richiesta (`motore::su_richiesta`)

```rust
pub struct Allegato {
    pub percorso: PathBuf,          // il file decodificato nell'archivio
    pub tipo: String,               // tipo MIME dalla struttura, in minuscolo
    pub nome_file: Option<String>,
    pub byte: u64,                  // dimensione decodificata
    pub impronta: String,           // SHA-256 in esadecimale (è il nome del file)
}
impl Richieste {                    // una connessione riusata, una per thread
    pub fn new(cfg: Config) -> Richieste;
    pub fn parte(&mut self, db: &mut Db, messaggio: i64, parte: &str) -> Risultato<Allegato>;
    pub fn scarica_testi(&mut self, db: &mut Db, messaggi: &[i64]) -> Risultato<usize>;
    pub fn messaggio_intero(&mut self, db: &mut Db, messaggio: i64) -> Risultato<Vec<u8>>;   // per «Salva»
    pub fn interruttore(&self) -> Interruttore;  // per chiudere da un altro thread
    pub fn chiudi(&mut self);                     // LOGOUT, risposta attesa al massimo 300 ms
    pub fn collegata(&self) -> bool;              // giro 2: c'è una connessione aperta
    pub fn collega(&mut self) -> Risultato<()>;   // giro 2: la apre adesso, accesso compreso
    pub fn mantieni(&mut self, noop_dopo: Duration) -> Mantenuta;   // giro 2: per le connessioni ferme
}
pub enum Mantenuta { Nessuna, Viva, ChiusaDalServer, Caduta(String) }
impl Interruttore { pub fn chiudi(&self); }       // Clone + Send: interrompe anche un download in corso
pub fn parte(cfg: &Config, db: &mut Db, messaggio: i64, parte: &str) -> Risultato<Allegato>;   // connessione usa e getta
pub fn scarica_testi(cfg: &Config, db: &mut Db, messaggi: &[i64]) -> Risultato<usize>;
pub fn parte_locale(db: &Db, messaggio: i64, parte: &str) -> Risultato<Option<Allegato>>;     // senza rete
pub fn miniatura(db: &Db, messaggio: i64, parte: &str) -> Risultato<Option<PathBuf>>;         // senza rete
pub fn pagina(db: &Db, messaggio: i64, parte: &str, n: u32) -> Risultato<Option<PathBuf>>;   // giro 2: la pagina n di un PDF, senza rete
pub fn completa_contenuti(db: &Db, quanti: usize) -> Risultato<usize>;   // giro 2: testo e codici, senza rete
pub fn nome_eml(message_id: Option<&str>, data: Option<&str>, mittente: Option<&str>, oggetto: Option<&str>) -> String;
pub fn nome_eml_messaggio(db: &Db, messaggio: i64) -> Risultato<String>;
pub fn completa_archivio(db: &Db) -> Risultato<usize>;   // misure delle parti scaricate prima, senza rete
pub fn pulisci_temporanei(db: &Db) -> usize;             // file temporanei rimasti da un'interruzione brusca
pub fn cartella_allegati(db: &Db) -> PathBuf;            // <cartella del db>/allegati
pub fn nome_miniatura(impronta: &str) -> String;         // <impronta>.miniatura.jpg
pub fn nome_pagina(impronta: &str, n: u32) -> String;    // <impronta>.pagina-<n>.jpg
pub fn nome_di_contenuto(nome: &str) -> bool;            // 64 cifre esadecimali: i file di contenuto
```

- `messaggio` è l'id locale (`localId` nella vista), `parte` il numero IMAP ("2", "1.2"): solo cifre e punti, il resto si rifiuta prima di comporre il comando.
- **Allegati**: se messaggio e parte sono già nella tabella `allegati` e il file c'è, niente rete. Altrimenti EXAMINE della cartella (con il controllo di UIDVALIDITY), `UID FETCH n (UID BODY.PEEK[parte])`, a pezzi da 4 MiB (`BODY.PEEK[parte]<inizio.4194304>`) sopra quella misura, così nessuna risposta deve arrivare tutta entro il tempo massimo. Poi decodifica (base64, quoted-printable; 7bit, 8bit e binary come sono), SHA-256, scrittura in un file temporaneo rinominato alla fine (0600, cartella 0700). Stesso contenuto, un file solo.
- **Perché SHA-256 e non FNV-1a**: il contenuto lo manda chiunque. Con un'impronta non crittografica chi scrive può costruire un allegato con la stessa impronta di un altro e sostituirlo nell'archivio. SHA-256 è scritto qui (`src/impronta.rs`, provato con i vettori del NIST), niente dipendenze nuove; con lui la dimensione nella chiave non serve.
- **`Richieste`** tiene aperta una connessione: su Gmail l'accesso costa più di un secondo, quindi il primo allegato lo paga e i successivi no. Un errore di rete fa riprovare una volta su una connessione nuova; dopo due minuti senza richieste la connessione si richiude da sola alla richiesta dopo (una connessione ferma da tanto può essere morta in silenzio). Sul finto server 115 allegati arrivano in 0,17 s con `Richieste` e in 19 s aprendo una connessione per ognuno.
- **Testi**: la prima parte `text/plain` e la prima `text/html` dei messaggi indicati che non li hanno ancora (gli altri si saltano), come durante la sincronizzazione.
- **Misure delle immagini** (proposta 5): appena una parte è nell'archivio, `misure::misure_immagine` legge larghezza e altezza dalle prime righe del file, senza librerie: PNG (IHDR), GIF, JPEG (il primo SOF0..SOF15, saltando DHT, JPG e DAC), WebP (VP8, VP8L, VP8X), BMP (anche OS/2 e con l'altezza negativa). Si guarda il contenuto, non il tipo dichiarato (nei sintetici ci sono PNG dichiarati `image/jpeg`). Vanno in `allegati.larghezza/altezza`; un'immagine che non si legge (SVG, formati rari) ha 0.
- **PDF** (proposta 7): `pdfinfo` per le pagine (`allegati.pagine`) e `pdftoppm -f 1 -l 1 -singlefile -scale-to 360 -jpeg -jpegopt quality=72` per la miniatura della prima pagina, scritta una volta per contenuto accanto al file, `<impronta>.miniatura.jpg`, passando da un file temporaneo. Un PDF protetto o rotto ha 0 pagine e niente miniatura, senza errori; se poppler-utils manca i campi restano vuoti e `completa_archivio` li riprova. Ogni programma ha un tempo massimo (10 s pdfinfo, 20 s pdftoppm) e non passa da una shell. Sul finto server, in debug, 63 PDF e 28 immagini si scaricano e si misurano in 2,7 s.
- **Pagine per l'anteprima** (giro 2): il visore di WebKitGTK porta gli strumenti per scrivere sul PDF e ignora `#toolbar=0`, quindi l'app mostra le pagine come immagini. `pagina` disegna la pagina `n` con `pdftoppm -f n -l n -singlefile -scale-to-x 1600 -scale-to-y -1 -jpeg -jpegopt quality=82` la prima volta che serve, accanto al file (`<impronta>.pagina-<n>.jpg`); oltre l'ultima pagina, o senza poppler, niente.
- **Testo e codici** (giro 2, tabella `contenuti`): `completa_contenuti` guarda i contenuti dell'archivio non ancora guardati, dal messaggio più recente: il testo dei PDF (`pdftotext -l 30 -enc UTF-8`), dei documenti DOCX, XLSX, PPTX, ODT, ODS, ODP (`unzip -p` delle parti con il testo: `word/document.xml`, intestazioni e piè di pagina, `xl/sharedStrings.xml`, le slide, `content.xml`; i tag diventano spazi, i paragrafi righe) e dei file di testo, al massimo 10.000 caratteri con gli spazi ridotti; per le immagini il tipo del primo codice che trova `zbarimg -q` («QR-Code», «EAN-13»), senza il contenuto. Una riga per contenuto anche quando non c'è niente; se un programma manca quel tipo si salta e si riprova la volta dopo. La vista li dà come `text` e `code` dell'allegato.
- **Connessioni ferme** (giro 2): dopo un po' di inattività Gmail manda `* BYE` e chiude; il socket restava in CLOSE-WAIT finché un clic non lo usava. `mantieni` guarda il socket senza leggere e senza aspettare (`peek` non bloccante): se il server ha chiuso, chiude anche qui (`ChiusaDalServer`); se ha scritto qualcosa, o la connessione è ferma da più di `noop_dopo`, manda un NOOP con al massimo 10 s di attesa, che la tiene viva o dice che non risponde (`Caduta`). `collega` la riapre prima che serva.
- **Fallimenti e quota**: una parte che non arriva va in `parti_fallite` e la coda del background la ripropone dopo 1 minuto, 2, 4… fino a 6 ore; quando arriva ne esce. I byte presi dal server (parti e messaggi interi, anche aperti a mano) vanno in `scaricati`, per giorno UTC.
- **`Interruttore`**: chiude la connessione di un `Richieste` da un altro thread, anche nel mezzo di un download (provato: il thread torna in 0,4 ms, l'archivio resta vuoto, la parte va fra quelle da riprovare). Dopo, quel `Richieste` non si collega più.
- **Messaggio intero** (proposta 8): `UID FETCH <uid> (UID BODY.PEEK[])`, che la guardia ammetteva già; sopra i 4 MiB (o se la misura non si sa) a pezzi, `BODY.PEEK[]<inizio.4194304>`. Non lo segna come letto e non lo salva nell'archivio: i byte vanno all'app.
- **Nome del file per «Salva»**: `nome_eml` fa come il prototipo: i primi 6 caratteri esadecimali dell'MD5 del Message-ID senza parentesi angolari e senza spazi intorno (senza Message-ID: data ISO come `date` della vista, indirizzo del mittente e oggetto uniti da `\n`), un trattino, l'oggetto con `/ \ : * ? " < > |` e i controlli cambiati in spazi, gli spazi di fila ridotti a uno, accorciato a 60 caratteri a fine parola; vuoto: `senza oggetto`. `<case-inoltro-0@esempio.it>` → `4f6f3f-…`. L'MD5 è in `src/impronta.rs`, provato con gli esempi della RFC 1321 e con testo UTF-8.

### Storico (`motore::storico`)

```rust
pub struct Storico;   // una connessione sua
impl Storico {
    pub fn new(cfg: Config, finestra: usize) -> Self;
    pub fn passo(&mut self, db: &mut Db, lotto: usize) -> Risultato<Passo>;
    pub fn chiudi(&mut self);
    pub fn interruttore(&self) -> Interruttore;
}
pub struct Passo { pub intestazioni: usize, pub testi: usize, pub restano: usize, pub byte: u64, pub finito: bool, pub aspetta: bool }
pub fn stato(db: &Db) -> Risultato<Option<StatoStorico>>;   // sul server, in locale, senza testo
```

Giro 2: i messaggi più vecchi della finestra della sincronizzazione, a lotti, su una connessione a parte. Prima le intestazioni e la struttura di tutta la casella, dal più recente indietro, poi i testi, sempre dal più recente; gli allegati li prende già il background degli allegati. Ogni `passo` fa un lotto e torna, così chi lo chiama decide le pause (quota, connessione a consumo, disco) e la posta nuova passa davanti. Prende solo gli UID più vecchi dei `finestra` più recenti: la sincronizzazione e lo storico non scaricano mai lo stesso messaggio. La lista degli UID mancanti si fa una volta per sessione (un `UID SEARCH ALL`); lo stato sta nel database, quindi dopo una chiusura si riparte da quello che manca. Se UIDVALIDITY è cambiata si ferma finché la sincronizzazione non ha ricollegato i messaggi. I byte contano nella quota del giorno. Un testo che non arriva non si richiede a ogni passo: si riprova alla sessione dopo.

### Allegati in background (`motore::scaricamento`, `motore::sistema`)

Il download lo fa l'app con la sua connessione delle richieste, una parte alla volta con `Richieste::parte`, così un clic passa sempre davanti. Al motore chiede cosa scaricare e se fermarsi:

```rust
pub struct DaScaricare { pub messaggio: i64, pub parte: String, pub tipo: String, pub byte: u64 /* stimati */ }
pub fn coda(db: &Db, quante: usize, in_testa: &[(i64, String)]) -> Risultato<Vec<DaScaricare>>;
pub struct Quota { pub giorno: String, pub scaricati: u64, pub limite: u64, pub restano: u64 }
pub fn quota(db: &Db, limite: u64) -> Risultato<Quota>;
pub enum Pausa { Quota, AConsumo, Disco }
pub fn pausa(db: &Db, limite: u64) -> Risultato<Option<Pausa>>;
pub const QUOTA_CONSIGLIATA: u64;   // 1 GiB
pub const SPAZIO_MINIMO: u64;       // 5 GiB
pub enum Categoria { Pdf, Immagini, Documenti, Fogli, Archivi, Altro }   // .nome(): "PDF", "immagini"…
pub fn categoria(tipo: &str, nome_file: Option<&str>) -> Categoria;
pub struct Conto { pub scaricati: u64, pub byte_scaricati: u64, pub da_scaricare: u64, pub byte_da_scaricare: u64 }
pub struct Totali { pub tutti: Conto, pub per_tipo: Vec<(Categoria, Conto)>, pub per_anno: Vec<(i64, Conto)>, pub archivio: u64, pub in_attesa: u64 }
pub fn totali(db: &Db) -> Risultato<Totali>;

// motore::sistema: programmi esterni; se mancano o falliscono, «niente» senza errore
pub fn connessione_a_consumo() -> bool;              // busctl … NetworkManager Metered: 1 e 3 sì, 2 e 4 no, 0 o errore = no
pub fn spazio_libero(cartella: &Path) -> Option<u64>;   // df -B1 --output=avail
pub fn pagine_pdf(pdf: &Path) -> Option<u32>;        // pdfinfo
pub fn miniatura_pdf(pdf: &Path, destinazione: &Path) -> bool;   // pdftoppm
pub const LIMITE_TESTO: usize;                      // 10.000 caratteri del testo di un allegato
// e per `completa_contenuti`, dentro il crate: pdftotext, unzip, zbarimg
```

- **`coda`**: allegati e immagini incorporate (non i testi, non le «altre» parti come il calendario), dei messaggi ancora sul server, non nell'archivio, non falliti da poco; dal messaggio più recente al più vecchio. Prima quelle di `in_testa`, nell'ordine dato (l'interfaccia ci mette quelle che ha sullo schermo), se servono ancora; doppioni e parti sconosciute si saltano. Una parte fallita non torna prima della sua attesa, neanche in testa: un clic chiama comunque `Richieste::parte`.
- **`quota`** e **`pausa`**: il giorno è quello UTC (in Italia ricomincia all'una o alle due di notte). `pausa` guarda nell'ordine la quota, la connessione a consumo e lo spazio sul disco del database; per la riga «in pausa: connessione a consumo» del pannello delle caselle.
- **`totali`**: per le parti scaricate i byte veri, per le altre la stima dalla struttura (base64 tolto); `archivio` conta lo stesso contenuto una volta; `in_attesa` le parti fallite che aspettano. Li stampa `motore allegati`.
- **Mai un file a metà**: la parte arriva tutta in memoria, poi va in un file temporaneo (che comincia con un punto) rinominato alla fine; un download interrotto non scrive niente.

`motore allegati --db FILE` stampa solo questi numeri. Sulla casella sintetica, dopo averne scaricati quattro:

```text
Allegati      125 · 462.4 KB · scaricati 4 (8.6 KB) · da scaricare 121 (453.8 KB)
Per tipo
  PDF         63 · 338.2 KB · scaricati 3 (8.3 KB) · da scaricare 60 (330.0 KB)
  immagini    28 · 64.9 KB · scaricati 0 (0 B) · da scaricare 28 (64.9 KB)
  documenti   3 · 5.5 KB · scaricati 0 (0 B) · da scaricare 3 (5.5 KB)
  fogli       15 · 49.2 KB · scaricati 0 (0 B) · da scaricare 15 (49.2 KB)
  archivi     2 · 390 B · scaricati 0 (0 B) · da scaricare 2 (390 B)
  altro       14 · 4.3 KB · scaricati 1 (311 B) · da scaricare 13 (3.9 KB)
Per anno
  2026        124 · 460.0 KB · scaricati 4 (8.6 KB) · da scaricare 120 (451.4 KB)
  2024        1 · 2.4 KB · scaricati 0 (0 B) · da scaricare 1 (2.4 KB)
Archivio      8.6 KB su disco (lo stesso contenuto una volta sola) · in attesa di riprovare 0
Oggi          11.7 KB scaricati dal server, quota 1.00 GB (giorno UTC 2026-10-28)
```

### Vista per l'interfaccia (`motore::vista`)

```rust
pub struct Account { pub indirizzo: String, pub nome: String, pub alias: Vec<String> }
pub struct Casella { pub indirizzo: String, pub tipo: String /* "gmail" | "imap" */, pub server: String }
pub struct OpzioniVista {
    pub account: Account,
    pub caselle: Vec<Casella>,
    pub prefisso_allegati: String,      // es. "a10s://localhost/allegato/"
    pub limite: Option<usize>,          // solo gli N più recenti
    pub solo: Option<Vec<i64>>,         // solo questi id locali (dopo scarica_testi)
    pub dal: Option<Dal>,               // giro 2: solo i cambi dopo questa versione
    pub html_solo_senza_testo: bool,    // giro 2: l'HTML solo dove non c'è il testo semplice
    pub testi_allegati: bool,           // giro 2: falso = il testo degli allegati a parte
    pub finestra: Option<usize>,        // giro 2, storico: solo gli N più recenti e i loro thread
}
pub struct Dal { pub generazione: String, pub versione: i64 }
impl Account { pub fn da_config(cfg: &Config, nome: &str, alias: &[String]) -> Account; }
impl Casella { pub fn da_config(cfg: &Config, db: &Db) -> Risultato<Casella>; }   // "gmail" se il server ha X-GM-EXT-1
pub fn json(db: &Db, opz: &OpzioniVista) -> Risultato<String>;
pub fn testi_allegati(db: &Db, opz: &OpzioniVista) -> Risultato<String>;   // {"generazione","versione","delta","texts":{url: testo}}
pub enum Storia { Persona(String), Cerca(Domanda) }
pub struct Domanda { pub parole: Vec<String>, pub almeno: usize, pub da: Vec<String>, pub oggetto: Vec<String>,
                     pub persone: Vec<String>, pub prima: Option<i64>, pub con_allegati: bool, pub etichetta: Option<String> }
pub fn storia(db: &Db, opz: &OpzioniVista, s: &Storia, quanti: usize) -> Risultato<String>;   // {"storia":true,…}
```

**La finestra e lo storico a richiesta** (giro 2). Con tutta la casella nel database la pagina non riceve tutto: con `finestra` arrivano gli N messaggi più recenti e gli altri dei loro thread (al massimo 200 per thread), così un thread resta intero; l'indice `messaggi_quando` li legge senza ordinare la casella. Le differenze contano solo i cambi dentro la finestra, quindi lo storico che arriva non fa rimandare tutto. Il resto lo dà `storia`, fuori dalla finestra e con gli altri messaggi dei loro thread: i messaggi di una persona (un indirizzo fra i partecipanti), o quelli che una ricerca potrebbe trovare. La ricerca usa l'indice FTS5 `ricerca` (oggetto, persone, testo) e `ricerca_allegati` (il testo dentro gli allegati, dallo schema 6), con le parole come inizio di parola e «almeno N di queste»; poi «da», «oggetto», persone, «prima di», con allegati, etichetta. Trova più del giusto: i criteri esatti li applica la pagina. Senza nessun criterio non manda niente.

**Solo le differenze** (giro 2). Rifare tutta la vista a ogni cambio non scala: sulla casella vera di 3.000 messaggi erano 77 MB e quasi due secondi, a ogni messaggio nuovo e a ogni «letto». Ora:
- la tabella `cambi` ha, per ogni messaggio, il numero del suo ultimo cambio, da un contatore che cresce. La riempiono **trigger di SQLite**, così nessun punto del motore che scrive se la può dimenticare: messaggi (solo le colonne che la vista mostra, quindi il MODSEQ di un giro senza cambi non conta), testi, partecipanti, etichette, intestazioni, parti, allegati (scaricati, misurati), testo degli allegati (per contenuto: tutti i messaggi con quel file), thread, eliminazioni. Nei trigger si usa l'UPSERT e non `INSERT OR REPLACE`: un `INSERT OR IGNORE` esterno imporrebbe la sua regola al trigger e il cambio si perderebbe (provato);
- la radice della vista ha `generazione` (scritta in `meta` alla creazione del database) e `versione`; con `dal` arrivano solo i messaggi cambiati dopo, con `"delta": true` e `tolti` (le chiavi dei messaggi spariti); se la generazione è un'altra (casella cambiata) o i cambi sono più di 2.000, tutto;
- la vista legge in una transazione sola, così versione e messaggi sono dello stesso istante mentre la sincronizzazione scrive da un'altra connessione;
- con `html_solo_senza_testo` l'HTML viaggia solo per i messaggi senza testo semplice, gli unici dove l'interfaccia lo usa (gli altri hanno `"html": null` e `htmlBytes`); con `testi_allegati` falso il testo dentro gli allegati arriva a parte, con `testi_allegati`, anche lui tutto o per differenze.

Restituisce una stringa nel formato di `window.AMB_DATA` (`mockups/data/README.md`): `{"schema":1,"meta":{"source":"imap","exportedAt":…,"count":N},"account":…,"accounts":[…],"messages":[…]}`, messaggi dal più vecchio al più recente. Per ogni messaggio:

- `id` è l'identità stabile (`gm:<X-GM-MSGID>` su Gmail, `mid:<Message-ID>|<byte>` altrove), che non cambia con UIDVALIDITY; `localId` è l'id nel database, quello che vogliono `parte` e `scarica_testi`.
- `date` (intestazione Date, `null` se illeggibile) e `internalDate`, in ISO; `from`, `to`, `cc`, `bcc`; `subject` (`null` se vuoto); `messageId`, `inReplyTo`, `references` tra parentesi angolari.
- `text` con gli a capo in `\n`, `html` o `null`. Senza testo scaricato: `text` vuoto e `"bodyMissing": true`. `"truncated": true` se una parte superava 1 MiB.
- `attachments`: `filename`, `mimeType`, `size` (decodificata: vera se già scaricato, altrimenti stimata dalla struttura), `inline`, `contentId`, `part`, `url` = prefisso + `localId` + `/` + parte. Nell'HTML i `cid:` delle immagini incorporate diventano lo stesso url. Per le parti già scaricate, quando si sanno: `"width"` e `"height"` (pixel, immagini), `"pages"` (PDF), `"thumb"` (PDF con la miniatura: l'url con `miniatura/` al posto di `allegato/` finale, per esempio `a10s://localhost/miniatura/<localId>/<parte>`; con un prefisso che non finisce in `allegato/` manca). Per esempio `{"filename":"Contratto.pdf",…,"part":"2","url":"a10s://localhost/allegato/1/2","pages":5,"thumb":"a10s://localhost/miniatura/1/2"}` e `{…,"url":"a10s://localhost/allegato/1/3","width":255,"height":360}`.
- `headers`: Sender, Reply-To, List-Id, List-Unsubscribe, List-Unsubscribe-Post, Precedence, Auto-Submitted, quando ci sono (le usa il prototipo per calendario e newsletter).
- `flags` (`seen`, `flagged`), `gmail` (`threadId` in esadecimale da X-GM-THRID, altrove la chiave del thread; `labels`), `problems`, `otherParts` (per esempio `text/calendar`).
- I messaggi eliminati altrove non ci sono. Le stringhe sono scritte da `src/json.rs` (niente serde): escape di `"` e `\`, `\u00XX` per i controlli, `\u2028`/`\u2029`, e `<` prima di `/` o `!` come `\u003c`, così il JSON si può mettere anche dentro un `<script>`.

**Tempi** (release, finto server): 405 messaggi in 5 ms (346 KB); 3.240 messaggi in 57 ms (4,2 MB), gli ultimi 500 in 13 ms. In debug circa cinque volte tanto.

## Il finto server IMAP e i dati sintetici

Per provare il motore e l'app senza Gmail.

```bash
node tools/sintetici-in-eml.mjs                       # dalla radice: scrive motore/prove/sintetici/
cd motore
cargo run --example finto_server                      # 127.0.0.1:1143, cartella prove/sintetici
cargo run --example finto_server -- --cartella prove/sintetici --porta 1143 --arrivi 20 --cadute 120
```

All'avvio stampa utente e password, finti: **`alex.moro@esempio.it`** e **`finta-password`** (l'utente è l'indirizzo dell'account sintetico, così i messaggi «miei» del prototipo restano miei; l'alias è `alex@studiomoro.esempio.it`). Per il motore: `--server 127.0.0.1:1143 --in-chiaro --credenziali FILE` con un file di due righe, utente e password.

| opzione | cosa fa |
| --- | --- |
| `--cartella DIR` | i `.eml` e `.meta` (predefinita `motore/prove/sintetici`) |
| `--porta N` | porta su 127.0.0.1 (1143) |
| `--utente U`, `--password P` | credenziali accettate |
| `--arrivi SECONDI` | un messaggio nuovo ogni SECONDI, notificato in IDLE con EXISTS: prima quelli tenuti da parte, poi copie dei vecchi con Message-ID e X-GM-MSGID nuovi; data di adesso |
| `--riserva N` | con `--arrivi`, quanti dei più recenti tenere da parte all'avvio (5) |
| `--cadute SECONDI` | fa cadere tutte le connessioni ogni SECONDI |
| `--pausa SECONDI` | dopo una caduta chiude subito le connessioni nuove per SECONDI (5): così si vede «Offline» |
| `--copie N` | moltiplica la casella per N, per le misure |
| `--nel-passato` | giro 2, con `--copie`: le copie vanno indietro negli anni, una prima dell'altra, e gli UID seguono le date come su Gmail: una casella con lo storico |
| `--lento MS` | aspetta MS millisecondi prima di rispondere a ogni comando (server lento) |
| `--chiudi-ferme SECONDI` | giro 2: chiude con `* BYE Autologout`, come Gmail, le connessioni senza comandi da SECONDI, anche quelle in IDLE da SECONDI senza un IDLE nuovo |
| `--verboso` | stampa ogni comando ricevuto (la password di LOGIN no) |

**Il server** (`examples/finto_server/`, in `examples` così non finisce nell'eseguibile dell'app) è in chiaro e solo su 127.0.0.1. Risponde a CAPABILITY (con X-GM-EXT-1, CONDSTORE, IDLE, ENABLE, ID), ID, LOGIN, ENABLE, NAMESPACE, LIST (con `\All` su «[Gmail]/Tutti i messaggi»), EXAMINE (UIDVALIDITY, UIDNEXT, HIGHESTMODSEQ, `[READ-ONLY]`), SEARCH e UID SEARCH, FETCH e UID FETCH con UID, FLAGS, INTERNALDATE, RFC822.SIZE, RFC822.HEADER, MODSEQ, X-GM-MSGID/THRID/LABELS, BODYSTRUCTURE, `BODY[…]`/`BODY.PEEK[…]` (`HEADER.FIELDS (…)`, `HEADER.FIELDS.NOT`, `HEADER`, `TEXT`, parti annidate come `1.2.1`, `n.MIME`, `<inizio.quanti>`), CHANGEDSINCE, IDLE/DONE, NOOP, LOGOUT. La **BODYSTRUCTURE è calcolata dal messaggio**: multipart annidati con i parametri, disposizione e nome del file, Content-ID, descrizione, codifica, byte e righe. Spam e cestino restano fuori da «Tutti i messaggi», come su Gmail. I comandi che scrivono ricevono NO e restano registrati, così le prove possono dire che non ne è arrivato nessuno. `BODY.PEEK[]` (anche a pezzi) dà il messaggio intero così com'è nel `.eml`. Per le prove ci sono anche `finto::messaggio_con_allegati` (un messaggio con allegati veri in base64, per esempio i PDF di più pagine e i JPEG di `mockups/data/attachments/`), `Opzioni::ritardo` (lo stesso di `--lento`) e `Server::connessioni_aperte`.

**I dati** (`tools/sintetici-in-eml.mjs`) vengono da `mockups/data/synthetic.js` e `mockups/data/attachments/`; lo script non legge altro. Per ogni messaggio un `.eml` e un `.meta` (`uid`, `x-gm-msgid`, `x-gm-thrid` dal `threadId`, `etichetta`, `flag`, `internaldate`, e le impronte attese di testo, HTML e allegati, per le prove), più `account.meta`. Le date si spostano come nel prototipo (`meta.shiftToNow`): di giorni interi, così l'ultimo messaggio cade oggi; `--senza-spostamento` le lascia com'erano. Il MIME imita i programmi di chi scrive (`from.client`): Gmail, Outlook (windows-1252 quando basta, parole codificate in base64, `Content-Description`), Apple Mail e iPhone, Thunderbird (8bit), servizi e newsletter (HTML in base64); confini nello stile di ciascuno, `X-Mailer`/`User-Agent`; 7bit, 8bit, quoted-printable o base64; oggetti e nomi con parole codificate RFC 2047, nomi di file RFC 2231; `multipart/alternative` per testo e HTML (più `text/calendar` per gli inviti), `multipart/mixed` per gli allegati, `multipart/related` con Content-ID per le immagini incorporate. Gli allegati sono file veri nel formato dichiarato: i PDF e gli `.ics` del prototipo, `.docx` e `.xlsx` veri costruiti dai `.txt` e `.csv`, immagini PNG disegnate dalle SVG sintetiche, zip veri; senza file, un contenuto inventato dall'anteprima. Le dimensioni sono quelle vere dei file, piccole, non quelle scritte nei dati (25 MB per uno zip). Dalla v14 i PDF hanno più pagine (63 PDF, da 1 a 24 pagine): le prove confrontano le pagine lette da `pdfinfo` con `pages` di `synthetic.js`. La copia in `motore/prove/sintetici/` per il server lanciato a mano va rigenerata con lo script per averli (le prove la generano da sole in `target/`).


## La garanzia della sola lettura

Un solo punto scrive sul socket: `rete::Connessione::invia`. Prima di scrivere passa il comando a `guardia::Guardia::verifica`, che lavora su una **lista di comandi ammessi** e ferma tutto il resto con un errore, prima che esca un solo byte.

- Ammessi: CAPABILITY, ID, LOGIN, AUTHENTICATE, ENABLE (solo CONDSTORE e QRESYNC), COMPRESS DEFLATE, NAMESPACE, LIST, STATUS, EXAMINE, FETCH e UID FETCH, SEARCH e UID SEARCH, IDLE, DONE (solo dentro IDLE), NOOP, LOGOUT.
- In FETCH ogni attributo è controllato: passano UID, FLAGS, INTERNALDATE, RFC822.SIZE, RFC822.HEADER, ENVELOPE, BODYSTRUCTURE, MODSEQ, X-GM-*, `BODY.PEEK[…]` e `BINARY.PEEK[…]`. `BODY[…]`, `BINARY[…]`, `RFC822` e `RFC822.TEXT` (che segnano come letto) vengono fermati, come ogni attributo sconosciuto. Fra i modificatori passano solo CHANGEDSINCE e VANISHED.
- Fermati, fra gli altri: SELECT, STORE, APPEND, COPY, MOVE, EXPUNGE, DELETE, CREATE, RENAME, SUBSCRIBE, CLOSE, UNSELECT, UID STORE/COPY/MOVE/EXPUNGE.
- Ogni invio è una riga sola: un CR, LF o NUL in mezzo viene fermato, così non si può nascondere un secondo comando dentro un argomento, e niente letterali.
- Le cartelle si aprono solo con EXAMINE. Se il server rispondesse `[READ-WRITE]` la sincronizzazione si ferma.
- I messaggi d'errore della guardia nominano il comando, mai gli argomenti (LOGIN contiene la password).

Le prove: `src/guardia.rs` (31 comandi ammessi, 34 rifiutati, righe multiple, IDLE/DONE) e `tests/guardia_in_rete.rs`, che manda 17 comandi vietati su una connessione vera al finto server e verifica che il server non ne riceva nessuno e che la connessione resti usabile. Tutte le prove di sincronizzazione controllano poi l'elenco dei comandi arrivati al server.

## Come funziona

**Sincronizzazione** (`src/sincronizza.rs`), a ogni giro:

1. connessione TLS, LOGIN, ID, ENABLE QRESYNC o CONDSTORE;
2. la cartella con l'attributo `\All` («[Gmail]/Tutti i messaggi»; su altri server, se manca, INBOX);
3. EXAMINE, con `(QRESYNC (uidvalidity modseq))` se il server lo offre e si ha già uno stato, altrimenti `(CONDSTORE)`;
4. se UIDVALIDITY è cambiata, si chiede l'identità di ogni UID (X-GM-MSGID su Gmail; Message-ID, data, mittente, oggetto e dimensione altrove) e si ricollegano i messaggi già scaricati ai nuovi UID, senza duplicati e senza riscaricarli;
5. se UIDNEXT, numero di messaggi e HIGHESTMODSEQ sono quelli di prima, non c'è altro da fare; altrimenti `UID SEARCH ALL` e confronto con gli UID locali: gli spariti si segnano come eliminati altrove, i mancanti della finestra (gli N più recenti) si scaricano;
6. flag ed etichette cambiati: `UID FETCH 1:* (UID FLAGS MODSEQ X-GM-LABELS) (CHANGEDSINCE m)`; con QRESYNC arrivano già dentro EXAMINE; senza CONDSTORE si rileggono tutti;
7. intestazioni (`BODY.PEEK[HEADER.FIELDS (…)]`), BODYSTRUCTURE, FLAGS, INTERNALDATE, RFC822.SIZE e su Gmail X-GM-MSGID, X-GM-THRID, X-GM-LABELS, dal più recente indietro, a lotti di 200, ognuno in una transazione;
8. il testo dei più recenti: la prima parte `text/plain` e la prima `text/html`, con `BODY.PEEK[n]<0.1048576>` (al massimo 1 MiB per parte). Gli allegati non si scaricano: si registrano dalla struttura (tipo, nome, byte, Content-ID);
9. lo stato della cartella (UIDVALIDITY, UIDNEXT, HIGHESTMODSEQ) si salva solo alla fine.

**Ascolto** (`src/ascolta.rs`): una connessione dedicata in IDLE, separata da quella di lavoro. Ogni `--controllo` secondi (e comunque entro 25 minuti, sotto i 29 di RFC 2177) esce da IDLE con DONE e manda un NOOP, ciascuno con 20 secondi al massimo: se non risponde, la connessione è morta anche se il sistema non se n'è accorto. Dopo un errore si ricollega con attese crescenti (1 s, 2 s, 4 s… fino a 5 minuti, con un po' di variazione). Quando il server segnala novità, la connessione di lavoro sincronizza.

**Robustezza**:

- tempo massimo su ogni lettura (`--timeout`, predefinito 60 s) e sulla connessione; nessun `unwrap` sui dati di rete;
- il flusso si divide in risposte complete (letterali compresi) prima di analizzarle: una risposta che il parser non capisce si conta come malformata e la sincronizzazione va avanti. Il suo UID va in `mancanti` e al giro dopo si richiede senza BODYSTRUCTURE, che è la parte più delicata; dopo 3 tentativi si lascia stare;
- un errore di rete durante il giro fa ripartire la passata su una connessione nuova (fino a 3 volte, con attese crescenti). Ogni lotto è una transazione e l'identità del messaggio è unica nel database, quindi la ripresa non duplica niente;
- se Gmail risponde NO a un FETCH (succede quando un messaggio non è leggibile) si tengono quelli arrivati;
- LOGIN rifiutato: ci si ferma subito, senza ritentare.
- le etichette di Gmail che arrivano tra virgolette si liberano degli escape (`\\Sent` → `\Sent`, `\"` → `"`): `imap-proto` le restituisce come sono. Gmail manda le etichette di sistema come atomi: con la casella vera il problema c'era solo per i nomi con virgolette o barre.

**Database** (`src/db.rs`), pensato per l'app vera:

| tabella | contenuto |
| --- | --- |
| `messaggi` | identità stabile (`gm:<X-GM-MSGID>` o `mid:<Message-ID>\|<byte>`), thread, Message-ID, In-Reply-To, References, oggetto, date, dimensione, letto/importante/risposto/bozza, altre parole chiave, MODSEQ, List-Id, «automatico» (newsletter e notifiche), stato del testo, eliminato altrove |
| `posizioni` | (cartella, UID) → messaggio: cambia con UIDVALIDITY, il messaggio no |
| `thread` | `gm:<X-GM-THRID>` su Gmail, altrove la radice dei References |
| `indirizzi`, `partecipanti` | persone per indirizzo, e chi compare in ogni messaggio con che ruolo (da, a, cc, ccn, rispondi_a) |
| `etichette` | etichette di Gmail (decodificate da UTF-7 modificato) |
| `parti` | ogni parte foglia della struttura: tipo, charset, codifica, byte, nome del file, disposizione, Content-ID, ruolo (testo, html, allegato, incorporato) |
| `corpi` | testo e HTML scaricati |
| `intestazioni` | Sender, Reply-To, List-*, Precedence, Auto-Submitted così come sono, per la vista (dallo schema 2) |
| `allegati` | messaggio e parte → SHA-256 e byte del contenuto decodificato nell'archivio `allegati/` (dallo schema 2); `larghezza`, `altezza`, `pagine`, `miniatura` (dallo schema 3: NULL non ancora guardato, 0 guardato ma illeggibile) |
| `contenuti` | impronta → testo letto dentro l'allegato e tipo di codice (dallo schema 4; una riga vuol dire «guardato») |
| `parti_fallite` | messaggio e parte → tentativi e prossimo tentativo (secondi Unix), per il background (dallo schema 3) |
| `scaricati` | giorno UTC → byte presi dal server per parti e messaggi interi: la quota (dallo schema 3) |
| `ricerca` | FTS5 su oggetto, persone e testo (`unicode61`, senza accenti) |
| `cartelle`, `mancanti`, `sincronizzazioni`, `meta` | stato per cartella, UID da riprovare, registro dei giri, capacità del server |

Lo schema 2 aggiunge solo tabelle, quindi un database vecchio si aggiorna da solo all'apertura. I messaggi scaricati prima però non hanno le righe di `intestazioni`: per averle serve un database nuovo. Lo schema 3 aggiunge due tabelle e quattro colonne ad `allegati` (`ALTER TABLE … ADD COLUMN` all'apertura, se mancano): un database dello schema 2 si apre e resta com'era; le misure degli allegati già scaricati le aggiunge `su_richiesta::completa_archivio`. Lo schema 4 aggiunge solo la tabella `contenuti`, che si riempie in background. Lo schema 5 aggiunge `cambi` e i suoi trigger: un database di prima parte da zero (la prima vista è comunque intera).

## Dipendenze

Cinque dirette, come deciso: MIME, protocollo IMAP, TLS (due crate) e SQLite. Nessun runtime async. L'eseguibile in release pesa 5,4 MB (4,8 MB senza simboli), quasi tutto SQLite, ring e le radici di Mozilla.

| crate | perché |
| --- | --- |
| `imap-proto` 0.17 | Il parser delle risposte IMAP usato da `imap` e `async-imap`, quindi provato su molti server veri. Copre quello che serve: CONDSTORE (MODSEQ, HIGHESTMODSEQ), QRESYNC (VANISHED), ENABLE, ID, le estensioni di Gmail (X-GM-MSGID, X-GM-THRID, X-GM-LABELS), BODYSTRUCTURE. Dipende solo da `nom`. È solo un parser: il client (comandi, etichette, letterali, tempi, IDLE) è scritto qui, ed è quello che permette la guardia in un punto solo. |
| `mail-parser` 0.11 | Intestazioni, parole codificate RFC 2047, set di caratteri, quoted-printable e base64. La 1.0 chiede Rust 1.98: si resta sulla 0.11 finché c'è Rust 1.94. Senza la feature `full_encoding` (che porterebbe `encoding_rs`): i set di caratteri più rari, soprattutto asiatici, non si decodificano. |
| `rustls` 0.23 | TLS in Rust puro, con `ring` per la crittografia (niente OpenSSL, niente cmake come con `aws-lc-rs`). Solo `std` e `tls12` fra le feature. |
| `webpki-roots` | Le autorità di certificazione di Mozilla, dentro l'eseguibile: niente lettura dei certificati di sistema, stesso comportamento su ogni macchina. Si aggiornano con il crate. |
| `rusqlite` 0.41 | SQLite compilato dentro (`bundled`, che comprende FTS5), più la cache delle istruzioni preparate. |

**Perché non gli altri client IMAP.** `imap` (3.0) è sincrono ma fermo da anni in alfa e porta `regex`, `chrono`, `ouroboros` e `native-tls` di base; i comandi passano come stringhe libere da più metodi, quindi la guardia andrebbe messa sotto, sul flusso di byte. `async-imap` richiede un runtime (tokio o async-std) e una catena di dipendenze molto più lunga. `imap-codec`/`imap-next` hanno tipi molto curati (comodi per una lista di comandi ammessi), ma `imap-codec` 2 è ancora in alfa, non conosce le estensioni di Gmail (una risposta con X-GM-MSGID diventa un errore) e il client asincrono sopra usa tokio.

**Perché niente async.** Servono due connessioni (lavoro e IDLE): bastano due thread con I/O bloccante e un tempo massimo su ogni lettura. Un runtime aggiungerebbe dipendenze e complessità senza un guadagno per due socket.

**Scritto qui** invece che preso da fuori: riga di comando, date (INTERNALDATE, ISO, RFC 5322), UTF-7 modificato, RFC 2231 per i nomi dei file, impronta FNV-1a, SHA-256 per l'archivio degli allegati, MD5 per i nomi dei file di «Salva», le misure delle immagini (PNG, GIF, JPEG, WebP, BMP), lo scrittore JSON della vista, attese crescenti, i finti server (quello dei test e quello con la casella sintetica, con la sua analisi MIME). Nessuna dipendenza nuova per l'app: base64 e quoted-printable degli allegati vengono da `mail-parser`, che c'era già. Pagine e miniature dei PDF, connessione a consumo e spazio libero vengono da programmi del sistema (`pdfinfo`, `pdftoppm`, `busctl`, `df`) chiamati con `std::process::Command`.

## Prove

`cargo test` (76 prove, nessuna rete) e `cargo clippy --all-targets -- -D warnings` sono puliti. Le prove sulla casella sintetica vogliono `node` per generare i `.eml`; senza, si saltano.

- `src/`: guardia, divisione in risposte e letterali, date, UTF-7, intestazioni MIME inventate (anche rotte), quoted-printable, RFC 2231, insiemi di UID; MD5 (RFC 1321 e UTF-8); misure di PNG, GIF, JPEG, WebP (tre varianti) e BMP costruiti nelle prove; nomi dei file `.eml`; uscite di `pdfinfo`, `busctl`, `df` e `zbarimg`, testo dagli XML di Office (entità, paragrafi, limite), programmi assenti, falliti o lenti, PDF rotto; categorie degli allegati; attese dopo i fallimenti; migrazione da un database dello schema 2.
- `tests/common/mod.rs`: un finto server IMAP su localhost, in Rust, con dati inventati, che risponde ai comandi del motore e registra ogni comando ricevuto. Si può guastare a comando.
- `tests/sincronizzazione.rs`: prima sincronizzazione (finestra, testi, allegati, thread, etichette, ricerca); differenze con CONDSTORE (3 nuovi, 2 flag cambiati, 1 eliminato, poi un giro senza novità che non manda né SEARCH né FETCH); differenze con QRESYNC; UIDVALIDITY cambiata su Gmail e su un server senza X-GM-EXT-1 (ricollegati, zero duplicati, niente riscaricato); connessione che cade a metà FETCH (ripresa automatica senza duplicati); interruzione fra due giri; risposte malformate (contate, riprovate senza struttura, recuperate); EXAMINE che risponde READ-WRITE; LOGIN rifiutato (un tentativo solo); server che non saluta (tempo scaduto); limite che cresce.
- `tests/ascolto.rs`: posta nuova durante IDLE e controllo periodico; IDLE che muore in silenzio (DONE senza risposta → caduta → riconnessione → la posta nuova arriva di nuovo); attese crescenti con un massimo.
- `tests/guardia_in_rete.rs` e `tests/riga_di_comando.rs`: vedi sopra.
- `tests/sintetici.rs`, sulla casella sintetica intera e sul finto server dell'esempio:
  - sincronizza tutto: 405 messaggi, 178 thread, 115 allegati, 405 testi, nessun problema; 395 testi, 23 HTML e 115 parti (numero, tipo, nome del file) uguali all'originale, confrontati con le impronte del `.meta`; un secondo giro non trova niente; notizie Inizio, Passo e Fine nell'ordine;
  - la vista ha la forma dei dati sintetici, controllata da JavaScript contro `synthetic.js` (oggetti, mittenti, destinatari, etichette, letti, allegati, Message-ID, thread), `bodyMissing` dove manca il testo, Sender e List-Unsubscribe, `otherParts`, limite;
  - testi su richiesta; tutti i 115 allegati scaricati e decodificati identici all'originale, 101 file nell'archivio (gli uguali una volta sola), una connessione sola per tutti, ripresa dopo una caduta, la seconda volta senza rete, numeri di parte non validi rifiutati;
  - un messaggio scritto a mano con parti annidate su tre livelli, un'immagine incorporata (il `cid:` riscritto nell'HTML), un nome RFC 2231 spezzato e un allegato di 5 MB che arriva in due pezzi;
  - il servizio: sincronizzazione, `InLinea`, posta nuova notificata in IDLE, connessioni che cadono (`Offline`, poi di nuovo `InLinea` e un giro), password sbagliata (una notizia sola e un solo LOGIN);
  - la vista su 3.240 messaggi (la casella moltiplicata per 8), con i tempi;
  - PDF e immagini: i 63 PDF hanno le pagine di `synthetic.js` e una miniatura JPEG con il lato lungo di 360 pixel, una per contenuto; le 28 immagini hanno le misure; la vista ha `pages`, `thumb`, `width` e `height`; tolte misure e miniature, `completa_archivio` le rifà senza rete; la pagina 1 larga 1600 pixel, niente oltre l'ultima; `completa_contenuti` guarda ogni contenuto una volta, il testo di tutti i PDF (anche «bianco latte» e «polipropilene» dei preventivi) e solo i due codici veri, il QR della fiera e l'EAN-13 del bollettino;
  - un messaggio scritto a mano con un PDF di 5 pagine, un JPEG 255×360, un PDF rotto (0 pagine, niente miniatura, nessun errore) e 5 MB di dati: il messaggio intero arriva in due pezzi identico all'originale e resta da leggere, quello piccolo in una richiesta sola; nomi dei file `.eml`; quota;
  - coda, quota, pausa e totali: ordine dal più recente, `in_testa`, parti scaricate che escono, una parte fallita che sparisce dalla coda per un minuto e poi la seconda volta per due, il sottocomando `allegati` che non stampa nomi, oggetti o indirizzi;
  - un download di 6 MB interrotto con `Interruttore` a metà (server lento): niente nell'archivio, niente file temporanei, la parte fra quelle da riprovare; un clic dopo lo scarica intero;
  - il servizio: un solo `Contatto` in un minuto nonostante i controlli dell'IDLE, `sincronizza_ora` durante l'IDLE, `ferma` sotto il mezzo secondo con l'IDLE aperto e a metà sincronizzazione su un server lento, `Richieste::chiudi` che non aspetta il LOGOUT del server lento oltre 0,3 s;
  - la connessione delle richieste chiusa dal server dopo un secondo (`--chiudi-ferme`): `mantieni` la trova chiusa, `collega` la riapre, un NOOP la tiene viva, e l'allegato dopo arriva senza un accesso nuovo;
  - solo le differenze: una differenza vuota senza messaggi, un «letto» che porta un messaggio solo, un giro senza cambi che non cambia la versione, un messaggio sparito tra i tolti con la sua chiave, un allegato scaricato e il suo testo che cambiano i messaggi giusti (il testo non viaggia coi messaggi ma con `testi_allegati`), un'altra generazione che dà tutto; l'HTML ridotto con gli stessi messaggi;
  - un «letto» dato sul server senza avviso in IDLE (il finto server avvisa solo dei messaggi nuovi): con `giro_ogni` arriva al giro dopo e finisce nel database;
  - in ogni prova, ogni comando arrivato al server passa di nuovo dalla guardia, e nessuno contiene `BODY[` senza PEEK, SELECT, STORE, APPEND, EXPUNGE, COPY o MOVE.

## Prova su Gmail (28 ottobre 2026)

Sotto PTY, con `script -qec "…" /dev/null`, sulla casella dell'utente.

- **Server**: imap.gmail.com, TLS 1.3. Estensioni: `IMAP4rev1 UNSELECT IDLE NAMESPACE QUOTA ID XLIST CHILDREN X-GM-EXT-1 UIDPLUS COMPRESS=DEFLATE ENABLE MOVE CONDSTORE ESEARCH UTF8=ACCEPT LIST-EXTENDED LIST-STATUS LITERAL- SPECIAL-USE APPENDLIMIT=35651584`. CONDSTORE sì, **QRESYNC no**, IDLE sì, COMPRESS sì.
- **Cartella**: «[Gmail]/Tutti i messaggi», 23.379 messaggi.
- **Primo giro, `--limite 200`**: 200 nuovi, 100 testi, 39 allegati registrati, nessun problema. Tempi: connessione 115 ms, accesso 1,35 s, cartelle 58 ms, examine 144 ms, elenco UID 206 ms, intestazioni 338 ms (591 messaggi/s), testi 2,57 s; totale 4,78 s. Letti 2,0 MB, scritti 2,5 KB.
- **Secondo giro, stesso limite**: nessuna differenza (UIDNEXT, numero e HIGHESTMODSEQ uguali); 668 ms, 2 KB letti. HIGHESTMODSEQ invariato subito dopo aver scaricato 100 testi conferma che BODY.PEEK non ha toccato i flag.
- **Terzo giro, `--limite 400`**: 200 nuovi in 374 ms (535 messaggi/s), 229 KB per intestazioni e struttura, cioè **1,1 KB per messaggio**; l'elenco degli UID costa circa 150 KB per 23 mila messaggi.
- **Quarto giro, stesso limite**: nel frattempo HIGHESTMODSEQ era cambiato, quindi elenco degli UID (172 ms, 150 KB) e `CHANGEDSINCE` (57 ms): nessun messaggio locale toccato, nessuno nuovo; 833 ms in tutto. EXAMINE dichiara `PERMANENTFLAGS ()`.
- **`stato`** con 400 messaggi: 247 thread (67 con più di un messaggio), 134 indirizzi, 82 mittenti diversi, 9 etichette, 81 allegati in 55 messaggi più 34 immagini incorporate, 158 messaggi automatici, 17 da leggere.
- **`ascolta --controllo 20 --durata 75`**: IDLE connesso, tre controlli con DONE + NOOP in 91–97 ms, fermato da solo dopo 76 s. Nessun processo rimasto acceso.

**Quanto costerebbe scaricare tutto** (stima grezza da queste misure): intestazioni e struttura dei 23.379 messaggi circa 25 MB e un minuto scarso; il database senza testi circa 70 MB. I testi sono circa 16 KB a messaggio: tutti sarebbero intorno ai 400 MB e una decina di minuti con i lotti di adesso.

## Cosa non c'è ancora

- La scrittura, quindi la coda delle operazioni («letto», «archivia», «invia») del passo 1 del piano.
- COMPRESS=DEFLATE: Gmail lo offre e ridurrebbe i byte delle intestazioni di alcune volte, ma serve `flate2`/`miniz_oxide` (Rust puro): da decidere.
- AUTHENTICATE (XOAUTH2): solo LOGIN con la password per le app.
- Una sola cartella, quella con `\All`. Su Gmail le etichette dicono il resto; Cestino e Spam non ci sono.
- Il ciclo che scarica gli allegati e lo storico in background lo fa l'app: il motore dà coda, passi, quota e pausa. Le miniature delle immagini le fa l'app (gdk-pixbuf): il motore dice quali mancano (`su_richiesta::immagini_da_ridurre`, sopra i 150 KB) e le segna fatte. Del testo si prendono solo la prima parte semplice e la prima HTML, fino a 1 MiB ciascuna.
- Le misure delle immagini non guardano l'orientamento EXIF dei JPEG (una foto girata ha larghezza e altezza scambiate).
- Il finto server non descrive le parti `message/rfc822` (manca l'ENVELOPE nella struttura): nei dati sintetici non ci sono. Non ha QRESYNC, apre solo «[Gmail]/Tutti i messaggi» e SEARCH restituisce sempre tutto.
- Nei dati sintetici, dalla v15, c'è un'immagine incorporata (lo screenshot del caso `allegati-nascosti`, in `multipart/related` con il suo Content-ID); le immagini della firma di Outlook restano allegati normali. Il percorso delle immagini incorporate è provato anche con il messaggio scritto a mano in `tests/sintetici.rs`.
- Niente invio di più comandi insieme (pipelining) né ESEARCH: l'elenco degli UID si richiede intero quando qualcosa è cambiato.
- Sospensione del computer e cambio di wifi provati solo come «connessione morta in silenzio» nel finto server, non dal vero.
- Thread fuori da Gmail: si usa la radice dei References, che non basta per i messaggi senza riferimenti.

## Domande aperte

- **Dove salvare «chiuso» su Gmail.** Con EXAMINE Gmail risponde `PERMANENTFLAGS ()`, come prevede la sola lettura: quali parole chiave accetta si vedrà solo con SELECT. Le strade: una parola chiave IMAP (`$Chiuso`), da provare al primo passo di scrittura su un messaggio di prova; un'etichetta nascosta via X-GM-LABELS, che però resta visibile sui messaggi nell'interfaccia di Gmail; oppure solo in locale, perdendo la sincronizzazione fra dispositivi.
- **Quanto scaricare.** Deciso nel giro 2: tutto, a strati (intestazioni, poi testi, poi allegati), con la quota del giorno.
- **COMPRESS**: vale una dipendenza in più?
- **IDLE su «Tutti i messaggi» o su INBOX**: ora è su «Tutti i messaggi», che vede anche la posta inviata da altri dispositivi.
- **Accesso per distribuire l'app**: OAuth con la verifica di Google (vedi `docs/implementazione.md`).
