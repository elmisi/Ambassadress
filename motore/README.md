# Il motore: banco di prova

Primo passo del piano in `docs/implementazione.md`. Un eseguibile da riga di comando, senza interfaccia, che sincronizza una casella IMAP in un database SQLite locale e misura tutto. **Questa versione è solo lettura**: non cambia niente sul server, la scrittura arriva dopo.

## Come si lancia

```bash
cd motore
cargo build --release
./target/release/motore sincronizza --limite 200   # prima volta: i 200 più recenti
./target/release/motore sincronizza --limite 200   # di nuovo: solo le differenze
./target/release/motore stato                      # conteggi del database locale
./target/release/motore cerca preventivo           # quanti messaggi trova la ricerca
./target/release/motore ascolta --durata 600       # IDLE per dieci minuti, poi si ferma
```

| comando | cosa fa |
| --- | --- |
| `sincronizza` (o `sync`) | scarica e aggiorna la cartella con tutti i messaggi |
| `stato` | conteggi del database: messaggi, thread, persone, allegati, testi, ultimi giri |
| `cerca PAROLE` | quanti messaggi trova la ricerca FTS5 (solo il numero) |
| `ascolta` | sincronizza, poi resta in IDLE e sincronizza quando arriva qualcosa |

Opzioni principali: `--limite N` (quanti messaggi recenti tenere sincronizzati, predefinito 1000), `--tutti`, `--corpi N` (per quanti dei più recenti scaricare il testo, predefinito 100), `--cartella NOME`, `--lotto N`, `--timeout SECONDI`, `--db FILE`, `--server HOST[:PORTA]`, `--credenziali FILE`. Per `ascolta`: `--controllo SECONDI` (predefinito 240) e `--durata SECONDI`. `motore aiuto` le elenca tutte.

- **Credenziali**: il programma legge `~/.secrets/gmail-user.txt` e `~/.secrets/gmail-app-password.txt` (le stesse di `tools/gmail-export.py`). Con `--credenziali FILE` legge invece un file di due righe, utente e password, per le prove. Non le stampa mai, neanche negli errori.
- **Database**: `motore/private/ambassadress.sqlite` (in `.gitignore`, cartella 0700, file 0600).
- **Uscita**: solo conteggi, tempi, byte, estensioni del server ed errori. Mai oggetti, indirizzi, nomi, etichette o testi. Una prova automatica (`tests/riga_di_comando.rs`) lo controlla sull'output vero.

## La garanzia della sola lettura

Un solo punto scrive sul socket: `rete::Connessione::invia`. Prima di scrivere passa il comando a `guardia::Guardia::verifica`, che lavora su una **lista di comandi ammessi** e ferma tutto il resto con un errore, prima che esca un solo byte.

- Ammessi: CAPABILITY, ID, LOGIN, AUTHENTICATE, ENABLE (solo CONDSTORE e QRESYNC), COMPRESS DEFLATE, NAMESPACE, LIST, STATUS, EXAMINE, FETCH e UID FETCH, SEARCH e UID SEARCH, IDLE, DONE (solo dentro IDLE), NOOP, LOGOUT.
- In FETCH ogni attributo è controllato: passano UID, FLAGS, INTERNALDATE, RFC822.SIZE, RFC822.HEADER, ENVELOPE, BODYSTRUCTURE, MODSEQ, X-GM-*, `BODY.PEEK[…]` e `BINARY.PEEK[…]`. `BODY[…]`, `BINARY[…]`, `RFC822` e `RFC822.TEXT` (che segnano come letto) vengono fermati, come ogni attributo sconosciuto. Fra i modificatori passano solo CHANGEDSINCE e VANISHED.
- Fermati, fra gli altri: SELECT, STORE, APPEND, COPY, MOVE, EXPUNGE, DELETE, CREATE, RENAME, SUBSCRIBE, CLOSE, UNSELECT, UID STORE/COPY/MOVE/EXPUNGE.
- Ogni invio è una riga sola: un CR, LF o NUL in mezzo viene fermato, così non si può nascondere un secondo comando dentro un argomento, e niente letterali.
- Le cartelle si aprono solo con EXAMINE. Se il server rispondesse `[READ-WRITE]` la sincronizzazione si ferma.
- I messaggi d'errore della guardia nominano il comando, mai gli argomenti (LOGIN contiene la password).

Le prove: `src/guardia.rs` (29 comandi ammessi, 34 rifiutati, righe multiple, IDLE/DONE) e `tests/guardia_in_rete.rs`, che manda 17 comandi vietati su una connessione vera al finto server e verifica che il server non ne riceva nessuno e che la connessione resti usabile. Tutte le prove di sincronizzazione controllano poi l'elenco dei comandi arrivati al server.

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
| `ricerca` | FTS5 su oggetto, persone e testo (`unicode61`, senza accenti) |
| `cartelle`, `mancanti`, `sincronizzazioni`, `meta` | stato per cartella, UID da riprovare, registro dei giri, capacità del server |

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

**Scritto qui** invece che preso da fuori: riga di comando, date (INTERNALDATE), UTF-7 modificato, RFC 2231 per i nomi dei file, impronta FNV-1a, attese crescenti, il finto server dei test.

## Prove

`cargo test` (33 prove, nessuna rete) e `cargo clippy --all-targets` sono puliti.

- `src/`: guardia, divisione in risposte e letterali, date, UTF-7, intestazioni MIME inventate (anche rotte), quoted-printable, RFC 2231, insiemi di UID.
- `tests/common/mod.rs`: un finto server IMAP su localhost, in Rust, con dati inventati, che risponde ai comandi del motore e registra ogni comando ricevuto. Si può guastare a comando.
- `tests/sincronizzazione.rs`: prima sincronizzazione (finestra, testi, allegati, thread, etichette, ricerca); differenze con CONDSTORE (3 nuovi, 2 flag cambiati, 1 eliminato, poi un giro senza novità che non manda né SEARCH né FETCH); differenze con QRESYNC; UIDVALIDITY cambiata su Gmail e su un server senza X-GM-EXT-1 (ricollegati, zero duplicati, niente riscaricato); connessione che cade a metà FETCH (ripresa automatica senza duplicati); interruzione fra due giri; risposte malformate (contate, riprovate senza struttura, recuperate); EXAMINE che risponde READ-WRITE; LOGIN rifiutato (un tentativo solo); server che non saluta (tempo scaduto); limite che cresce.
- `tests/ascolto.rs`: posta nuova durante IDLE e controllo periodico; IDLE che muore in silenzio (DONE senza risposta → caduta → riconnessione → la posta nuova arriva di nuovo); attese crescenti con un massimo.
- `tests/guardia_in_rete.rs` e `tests/riga_di_comando.rs`: vedi sopra.

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
- Gli allegati non si scaricano (né l'archivio per contenuto), e del testo si prendono solo la prima parte semplice e la prima HTML, fino a 1 MiB ciascuna.
- Niente invio di più comandi insieme (pipelining) né ESEARCH: l'elenco degli UID si richiede intero quando qualcosa è cambiato.
- Sospensione del computer e cambio di wifi provati solo come «connessione morta in silenzio» nel finto server, non dal vero.
- Thread fuori da Gmail: si usa la radice dei References, che non basta per i messaggi senza riferimenti.

## Domande aperte

- **Dove salvare «chiuso» su Gmail.** Con EXAMINE Gmail risponde `PERMANENTFLAGS ()`, come prevede la sola lettura: quali parole chiave accetta si vedrà solo con SELECT. Le strade: una parola chiave IMAP (`$Chiuso`), da provare al primo passo di scrittura su un messaggio di prova; un'etichetta nascosta via X-GM-LABELS, che però resta visibile sui messaggi nell'interfaccia di Gmail; oppure solo in locale, perdendo la sincronizzazione fra dispositivi.
- **Quanto scaricare.** Intestazioni di tutto (poco: 25 MB) e testi solo degli ultimi mesi, o tutto?
- **COMPRESS**: vale una dipendenza in più?
- **IDLE su «Tutti i messaggi» o su INBOX**: ora è su «Tutti i messaggi», che vede anche la posta inviata da altri dispositivi.
- **Accesso per distribuire l'app**: OAuth con la verifica di Google (vedi `docs/implementazione.md`).
