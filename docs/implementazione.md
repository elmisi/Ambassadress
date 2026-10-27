# Implementazione

Come si costruisce l'app vera dopo i mockup (v1 … v12). Requisiti: `desiderata.md`, più quelli dati all'inizio di questa fase: rete che non perde un colpo con un IMAP impeccabile, visualizzazione degli allegati perfetta, veloce, leggera, resa il più vicina possibile al prototipo.

## Decisioni prese

- **Prima piattaforma: desktop Linux.** App Tauri, che usa il motore web del sistema (WebKitGTK su Linux): niente browser incorporato, pacchetto di pochi MB. Lo stesso motore può servire l'interfaccia anche nel browser, in locale. Mac e Windows dopo, il telefono più avanti.
- **Caselle: IMAP generico, provato su Gmail.** Il motore parla IMAP standard e usa le estensioni di Gmail quando ci sono (`X-GM-THRID`, `X-GM-MSGID`, `X-GM-LABELS`). La prima casella vera è la Gmail dell'utente.
- **Dipendenze: poche e scelte.** Da librerie mature solo MIME (`mail-parser`), protocollo IMAP, TLS (`rustls`) e SQLite (con FTS5); tutto il resto scritto nel progetto. Ogni dipendenza nuova va motivata.
- **Motore in Rust.** Veloce, poca memoria, nessuna pausa per la pulizia della memoria, un solo eseguibile; si integra con Tauri.

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

## Ordine di lavoro

1. **Banco di prova del motore**: sincronizzazione, coda delle operazioni, misure sulla casella vera in sola lettura (stampa solo conteggi e tempi). Prove con rete che cade, sospensione, cambio di wifi.
2. **Client in sola lettura**: persone, thread, ricerca, allegati, con la resa della v12.
3. **Azioni**: «letto», «chiudi», «archivia» con la coda offline; poi risposta, bozze e invio.
4. **Moduli**: import, rubrica, anteprime Office; poi, se serve, l'AI locale.

Ogni passo si usa per un po' e si raccolgono note, come con i mockup.
