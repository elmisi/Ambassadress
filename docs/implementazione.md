# Implementazione

Come si costruisce l'app vera dopo i mockup (v1 … v12). Requisiti: `desiderata.md`, più quelli dati all'inizio di questa fase: rete che non perde un colpo con un IMAP impeccabile, visualizzazione degli allegati perfetta, veloce, leggera, resa il più vicina possibile al prototipo.

## Decisioni prese

- **Prima piattaforma: desktop Linux.** App Tauri, che usa il motore web del sistema (WebKitGTK su Linux): niente browser incorporato, pacchetto di pochi MB. Lo stesso motore può servire l'interfaccia anche nel browser, in locale. Mac e Windows dopo, il telefono più avanti.
- **Caselle: IMAP generico, provato su Gmail.** Il motore parla IMAP standard e usa le estensioni di Gmail quando ci sono (`X-GM-THRID`, `X-GM-MSGID`, `X-GM-LABELS`). La prima casella vera è la Gmail dell'utente.
- **Dipendenze: poche e scelte.** Da librerie mature solo MIME (`mail-parser`), protocollo IMAP, TLS (`rustls`) e SQLite (con FTS5); tutto il resto scritto nel progetto. Ogni dipendenza nuova va motivata.
- **Motore in Rust.** Veloce, poca memoria, nessuna pausa per la pulizia della memoria, un solo eseguibile; si integra con Tauri.

## Regole decise (2026-10-28)

- **Più caselle: una lista sola.** Le persone sono persone, da qualunque casella scrivano. La risposta parte dall'indirizzo a cui ti hanno scritto; con più di una casella il riquadro della risposta dice da quale.
- **Notifiche del desktop: solo dalle persone.** Niente per servizi e newsletter, niente per i thread in cui sei solo in copia dei messaggi altrui.
- **Cartelle ed etichette di Gmail: filtri della ricerca.** Lo spam non si vede, il cestino è dove va la pulizia dei servizi, le etichette dell'utente diventano filtri in Cerca. Nessuna vista nuova.
- **Storico: tutto, a strati.** Intestazioni di tutta la casella; testi degli ultimi 12 mesi subito, il resto in background a bassa priorità; allegati grandi quando si aprono. I numeri si confermano col banco di prova.
- **Eliminato altrove: sparisce in silenzio.** Se era l'ultimo messaggio dell'argomento, l'argomento va via. Resta cercabile nel cestino finché il server lo tiene.
- **Immagini esterne delle newsletter: bloccate.** «Mostra immagini» le mostra e vale da lì in poi per quel mittente.
- **«Segnala» resta nell'app vera** per tutto il periodo di prova, con lo stesso report reso anonimo e lo stesso SQLite.
- **Nome breve sotto l'icona: «a10s».**
- **Prima la sola lettura.** La prima versione dell'app legge soltanto dal server IMAP, per non fare danni: le azioni («letto», «archivia», «chiudi», invio) restano locali o arrivano dopo, con la parte di scrittura.

## Architettura

```
┌────────────── interfaccia (HTML/CSS/JS) ──────────────┐
│  il linguaggio visivo della v12                       │
└───────────────▲───────────────────────────────────────┘
                │ legge solo dal database locale
┌───────────────┴──────── motore (Rust) ────────────────┐
│ sincronizzazione IMAP · invio SMTP · analisi MIME      │
│ thread, persone, citazioni e firme · ricerca           │
│ archivio allegati · registro delle operazioni          │
│        SQLite (FTS5 per la ricerca)                    │
└────────────────────────────────────────────────────────┘
```

- **L'interfaccia non aspetta mai la rete**: legge dal database locale, il motore sincronizza in background.
- **Rete**:
  - sincronizzazione per differenze con CONDSTORE (RFC 7162), QRESYNC dove c'è;
  - una connessione in IDLE per la posta nuova e una per il lavoro, con un controllo periodico che si accorge di un IDLE morto;
  - riconnessione con attese crescenti;
  - gestione di UIDVALIDITY senza duplicati.
- **Registro delle operazioni**: «letto», «archivia», «invia» e gli altri si applicano subito in locale e vanno in una coda ripetibile senza danni, che si svuota verso il server quando c'è rete. Offline l'app funziona uguale.
- **Allegati**:
  - salvati per contenuto, così lo stesso file in più email occupa spazio una volta sola e le versioni dello stesso documento si riconoscono dal contenuto;
  - all'arrivo si scarica la struttura del messaggio, poi gli allegati piccoli subito e quelli grandi quando si aprono;
  - anteprima di immagini e PDF dentro l'app, con il visore PDF come modulo caricato quando serve;
  - Office con un modulo esterno;
  - il testo dei PDF va nella ricerca.
- **Resa**:
  - le email tra persone si ripuliscono di citazioni e firme partendo dall'HTML, riconoscendo i contenitori dei programmi (Gmail, Outlook, Apple Mail), e si mostrano con lo stile del prototipo;
  - newsletter e notifiche si mostrano come sono, in un riquadro isolato, senza immagini esterne.
- **Le regole del prototipo passano nel motore**: thread, persone, `splitQuote`, «Leggi tutto», immagini della firma, mappa degli stati (`mockup_v10.md`). I dati sintetici e i casi delle segnalazioni diventano i test automatici.
- **Interfaccia**: aggiorna solo quello che cambia e disegna solo le righe sullo schermo (il runtime dei mockup ridisegna tutto e non regge decine di migliaia di email).
- **Moduli**: le funzioni rare (import da altri programmi, rubrica, anteprima Office, AI locale) sono processi o moduli separati, caricati quando servono e chiusi dopo.

## Da verificare nel banco di prova

- Quali estensioni offre davvero Gmail (CONDSTORE sì, QRESYNC probabilmente no) e quanto costa la prima sincronizzazione.
- Dove salvare sul server gli stati che IMAP non ha («chiuso»): parole chiave IMAP dove il server le accetta; su Gmail probabilmente un'etichetta nascosta.
- Accesso a Gmail: password per app per l'uso personale. Per distribuire l'app serve OAuth, con la verifica di sicurezza di Google per l'accesso alla posta; senza verifica, in modalità «test», l'accesso andrebbe rinnovato ogni settimana.
- Le bozze: una per argomento, salvata spesso in locale e caricata sul server di rado, per non moltiplicarle come fa Gmail.

## Banco di prova: cosa ha detto (2026-10-28)

Il motore in sola lettura è in `motore/` (dettagli e come si lancia nel suo README).
- **Struttura e dipendenze.** Rust, nessun runtime async: due thread con I/O bloccante e tempi massimi. Cinque dipendenze dirette: `imap-proto` (solo il parser delle risposte), `mail-parser` 0.11, `rustls` con `ring`, `webpki-roots`, `rusqlite` con `bundled`.
- **Sola lettura.**
  - Ogni comando passa da una guardia con la lista dei comandi ammessi, in un solo punto che scrive sul socket.
  - Il corpo si legge solo con `BODY.PEEK`, le cartelle si aprono solo con EXAMINE, e se il server risponde `[READ-WRITE]` ci si ferma.
  - 33 test con un finto server IMAP, `clippy` pulito.
- **Gmail.**
  - Estensioni: CONDSTORE sì, **QRESYNC no**, IDLE sì, COMPRESS=DEFLATE sì, X-GM-EXT-1 sì.
  - La casella ha 23.379 messaggi.
  - 200 messaggi con 100 testi si scaricano in 4,8 s (2 MB); un secondo giro senza novità costa 0,7 s e 2 KB.
  - Stime per tutto: intestazioni circa 25 MB e meno di un minuto, testi circa 400 MB e una decina di minuti.
- **Deciso dopo il banco di prova.** «Chiuso» su Gmail si salva con un'etichetta (`X-GM-LABELS`), che si vede anche dentro Gmail. Arriva con la parte di scrittura; fino ad allora «chiuso» resta locale.
- **Ancora da decidere.**
  - Se COMPRESS vale una dipendenza in più.
  - Se IDLE resta su «Tutti i messaggi» o va su INBOX.

## Client in sola lettura: com'è fatto (2026-10-28)

L'app è in `app/` (dettagli, prove e cosa manca nel suo README).

- **Tauri 2 su WebKitGTK**, un eseguibile di 11 MB in release. L'interfaccia è la v13 copiata in `app/ui/` con le modifiche per l'app, sullo stesso runtime dei mockup, che riceve i dati dal motore attraverso `ponte.js`. Il carattere è dentro l'app, niente rete per i font.
- **Dati**: il motore (`vista`) scrive il JSON nello stesso formato di `AMB_DATA`, quindi il modello del prototipo funziona senza cambiare. La sincronizzazione è il `Servizio` del motore (sincronizza, poi IDLE, si ricollega da solo) con una finestra dei 3.000 messaggi più recenti, testi compresi.
- **Allegati**: protocollo `a10s://localhost/allegato/<messaggio>/<parte>`. Il motore li scarica la prima volta con BODY.PEEK su una connessione riusata, poi li tiene in un archivio per contenuto (SHA-256). I PDF si vedono dentro l'app con il visore di WebKitGTK; «Apri» passa una copia a `xdg-open`.
- **Casella**: primo avvio vero (accesso di prova, poi salvataggio). La password sta in un file 0600; il portachiavi del sistema verrà dopo.
- **Sola lettura**: la guardia del motore resta l'unico punto che parla col server. «Letto», «archivia» e le bozze restano nell'interfaccia, «Invia» dice che la scrittura non c'è ancora.
- **Prove** senza la posta vera:
  - nel browser con il motore finto di `ponte.js`;
  - nella finestra vera col finto server IMAP del motore, che serve i dati sintetici come email vere, guidata da script (`A10S_PROVA_JS`) e fotografata su Xvfb.
- **Velocità**: l'interfaccia ridisegna tutto a ogni clic. Con 3.000 messaggi regge, con 10.000 un ridisegno costa circa 250 ms. Il disegno delle sole righe sullo schermo resta da fare, insieme allo storico oltre la finestra.

## Ordine di lavoro

1. **Banco di prova del motore**: sincronizzazione, coda delle operazioni, misure sulla casella vera in sola lettura (stampa solo conteggi e tempi). Prove con rete che cade, sospensione, cambio di wifi.
2. **Client in sola lettura**: persone, thread, ricerca, allegati, con la resa della v12 e le schermate della v13 (primo avvio, stato della rete, account, tema scuro, newsletter). *Fatto in `app/`, provato col finto server; da usare sulla casella vera e raccogliere note.*
3. **Scrittura**: «letto», «chiudi», «archivia» verso il server con la coda offline; poi risposta, bozze e invio.
4. **Moduli**: import, rubrica, anteprime Office; poi, se serve, l'AI locale.

Ogni passo si usa per un po' e si raccolgono note, come con i mockup.
