# Ambassadress

Un client di posta ripensato da zero. Ci sono i mockup navigabili (`mockups/v1` … `mockups/v14`), uno per giro di lavoro, il motore in Rust (`motore/`) e l'app desktop in sola lettura (`app/`, Tauri). Cosa vuole l'utente: `docs/desiderata.md`. Come si costruisce l'app vera: `docs/implementazione.md`. Le note di ogni giro: `docs/mockup_vN.md` (dalla v10 alla v13), poi `docs/app_giroN.md` (dal giro 1 dell'app, che contiene la v14); la mappa di stati ed eventi è in `docs/mockup_v10.md`.

## Regole fisse

- **Sempre in italiano**, in chat e nei documenti. Tono piano, mai saccente.
- **Mai i dati veri.** L'export della casella Gmail sta in `mockups/data/private/` (in `.gitignore`, con deny in `.claude/settings.json`). Non leggerlo, cercarlo, stamparlo o caricarlo, neanche per verifica. Si lavora solo con `mockups/data/synthetic.js`. Anche quando l'utente incolla esempi, nei dati sintetici nomi, aziende, indirizzi e testi sono sempre inventati.
- **Mai `~/.secrets/`.** La password Gmail la legge solo `tools/gmail-export.py`, che stampa solo conteggi.
- **Link locali sempre.** In ogni risposta sui mockup dare i link `file:///home/alessandro/elmisi/Ambassadress/mockups/...` (indice e versione, più `?data=real` per i dati veri).
- **Leggero e pulito.** Ogni miglioria aggiunge il minimo: un'etichetta o un comportamento automatico, non un pulsante, una vista o un'impostazione. Zero configurazioni. Quando si propone qualcosa, dire cosa si aggiunge all'interfaccia.
- **Commit e pubblicazione solo su richiesta.** Niente `Co-Authored-By` né altre righe che indicano Claude come autore, nei commit e nei documenti. Non c'è un file `VERSION`: niente semver. Messaggi di commit in italiano, brevi.

## Come si lavora un giro

1. **Note.** Durante l'uso della versione corrente l'utente detta osservazioni: vanno in `mockups/PROSSIMO-GIRO.md` (crearlo se manca), una voce per punto, in grassetto il titolo, poi cosa succede oggi nella versione corrente e cosa si vuole. In questa fase si scrive solo nel file, **non si implementa niente**.
2. **Mappa.** Quando una nota tocca lo stato degli argomenti, aggiornare la mappa stati × eventi (in `docs/mockup_v10.md` c'è quella attuale, da copiare e aggiornare). Se una combinazione non è chiara, chiedere con domande a scelta, con la proposta consigliata per prima.
3. **Proposte.** Se l'utente lo chiede, analizzare tutto quello che l'app fa più le note, e proporre migliorie **una alla volta**, ciascuna con contesto e motivazione, aspettando il sì o il no prima della successiva. Quelle accettate finiscono nel file.
4. **«Vai».** Solo quando l'utente dice di procedere si costruisce la versione nuova (sotto).
5. **Chiusura del giro.** Il file delle note si sposta in `docs/mockup_vN.md`, con in fondo «Com'è andata nella vN»: scostamenti dalle note, bug trovati, cosa resta aperto. Si aggiorna questo file, poi commit e push se richiesti.

## Costruire la versione nuova

- **Caricare la skill `frontend-design:frontend-design`** prima di toccare l'aspetto. Il linguaggio visivo è quello della v7 in poi (sotto): estenderlo, non tornare a card, chip, maiuscoletti e accento blu.
- Copiare `mockups/v(N-1)/project/` in `mockups/vN/project/`; in `Main.dc.html` aggiornare `this.version`. Ricreare i symlink `support.js -> ../../support.js` e `data -> ../../data`. Aggiornare `canvas.json` (titolo «Ambassadress mockups vN» e nome della tavola).
- Aggiungere la versione in cima a `mockups/index.html` e nella tabella di `mockups/README.md` («ancora da pubblicare» finché non c'è il canvas).
- Se serve un caso nuovo nei dati: `mockups/data/cases/<nome>.json` (modello `_modello.json`) per riprodurre una segnalazione, oppure un thread nel generatore per dare realismo; descriverlo in `mockups/data/README.md`, rigenerare con `node mockups/data/generate-synthetic.mjs`. I dati sono comuni a tutte le versioni.
- **I dati sintetici devono sembrare posta vera, non una chat** (richiesta dell'utente dopo la v10): ogni risposta cita tutto il thread sotto, nel formato del programma di chi scrive (`clientOf` e `quoteBlock` nel generatore: Gmail it/en, Outlook it/en, Apple Mail, iPhone, Thunderbird); firme di ogni forma (`signatureOf`); testi lunghi in `mockups/data/testi-lunghi.mjs`. Le scelte di stile usano un generatore casuale a parte (`rq`), così aggiungere thread non sposta il resto.
- **Provare tutto prima di dire che è pronto**: Playwright via `python3 -m http.server -d mockups 8799` (la porta 8765 è occupata da un altro progetto dell'utente: non toccarla) e anche da `file://`. Nessun errore in console, ogni voce delle note provata, anche a 390 px di larghezza senza scorrimento orizzontale. Dopo aver rigenerato `synthetic.js`, disattivare la cache del browser (CDP `Network.setCacheDisabled`), altrimenti si vedono i dati vecchi. Svuotare il `localStorage` tra una prova e l'altra quando serve partire puliti. Alla fine cancellare `.playwright-mcp/` e fermare il server: con `kill` sui PID trovati da un pattern che non trova sé stesso, per esempio `pgrep -f '[h]ttp.server -d mockups'`: `pkill -f` o `pgrep -f` con il testo semplice trovano anche la shell che li lancia e la terminano.
- Il resoconto finale all'utente: link locali, cosa c'è, bug corretti, dove ci si è discostati dalle note e perché, cosa resta aperto, e le decisioni che spettano a lui (commit, canvas, note del giro).

## Come sono fatti i mockup

- **`.dc.html`** è il formato del canvas Claude Design. In locale gira con `mockups/support.js`, un runtime minimo scritto per questo progetto: `{{buchi}}`, `<sc-for>`, `<sc-if>`, eventi `on*` (in minuscolo), stato `DCLogic`. A ogni `setState` ridisegna tutto (`replaceChildren`) e rimette il fuoco solo per `id`: i campi che devono tenere il fuoco hanno un `id`, e le scorciatoie vanno agganciate a `window`, non a un elemento. La pagina è viva in `<x-dc>` prima del primo disegno: un `src` o un `onLoad` con un `{{buco}}` partirebbe così com'è scritto, quindi si scrive `sc-src` / `sc-onLoad` (dalla v14). Se una tavola usa qualcosa che il runtime non copre, estendere `support.js`.
- **Stato salvato** nel `localStorage`, chiave `ambassadress:<fonte>` (`synthetic` o `real`), comune alle versioni. I campi persistenti della v10 sono in `persisted()`; `change(etichetta, patch)` li modifica con l'avviso «Annulla» (Ctrl+Z). «Torna allo stato iniziale» dal pulsante «Dati sintetici» / «Dati veri» azzera tutto, con «Annulla».
- **Bug con i dati veri**: l'utente preme «Segnala», il report ha il contenuto già sostituito (lettere in x, cifre in 9, indirizzi in `⟦P3⟧`). Dal report si ricostruisce un caso sintetico che riproduce la forma, poi si corregge il prototipo finché il caso si vede bene. Non chiedere mai di incollare email vere.
- **Segnalazioni (dalla v12).** Con `python3 tools/segnalazioni.py` acceso (porta 8797), «Segnala» salva in `segnalazioni/segnalazioni.sqlite` (in `.gitignore`, si può leggere: il contenuto dei dati veri è già sostituito). Gli stati li gestisce Claude:
  1. `python3 tools/segnalazioni.py elenco` per le aperte (nuova, in-lavorazione); `mostra N` per il report intero (`--json` per il solo JSON).
  2. `stato N in-lavorazione` quando ci si mette mano.
  3. Si ricostruisce il caso sintetico (sopra) e si corregge il prototipo, di solito nella versione in corso.
  4. Si chiude con `stato N risolta --versione vN --nota "…"`, oppure `non-riproducibile` o `scartata`, sempre con una nota che dice perché.
  5. Nel resoconto all'utente si dice quali segnalazioni sono state chiuse.
  Il server non si accende da solo: lo accende l'utente quando prova il prototipo. Per le prove con Playwright si può accendere in background e poi fermarlo, cancellando le segnalazioni di prova.
- **Canvas**: ogni versione ha un canvas privato su claude.ai, pubblicato con root `mockups/vN/`. Dei dati si pubblica solo `data/synthetic.js`, mai `support.js` né `private/`. Pubblicare solo se l'utente lo chiede; poi mettere il link in `mockups/README.md`.
- **Icona**: sigillo H2 su fascia di pergamena L3, file in `mockups/icona/`, canvas nel README. Dalla v12 la favicon e il logo accanto al nome sono L3 (`icona-senza-fondo.svg` ritagliata intorno alla fascia, incorporata come `data:` nella pagina); L4, con i bordi del rotolo più scuri, è l'alternativa. Il nome breve sotto l'icona è «a10s».

## Linguaggio visivo (dalla v7)

- Fondo carta, tre colonne: persone, conversazione, indice degli argomenti. Carattere Atkinson Hyperlegible Next.
- Il colore ha un significato: ogni argomento ha il suo, una striscia a sinistra della conversazione («il filo»). Il rosso ceralacca segna solo «da leggere». Tutto il resto è inchiostro. Stati premuti morbidi, niente testo bianco su nero.
- Box dei messaggi larghi 3/4 della colonna, testo 15 px con interlinea 1,45, righe vuote di fila ridotte a una.
- Niente riquadri intorno all'argomento attivo (escono dallo schermo con le email lunghe): il suo filo si fa più spesso e il titolo si evidenzia nell'indice.
- Nella conversazione di una persona, i messaggi degli altri hanno le loro iniziali nei loro colori; il nome porta alla loro conversazione.
- Una risposta scritta dentro la citazione si mostra con le righe citate in piccolo, rientrate e senza `>`.
- Le immagini della firma (sotto i 40 KB, con un nome da firma o uguali in più messaggi della stessa persona) non stanno tra gli allegati del messaggio: tornano con «Mostra firma», sotto la firma di testo; in Allegati stanno in fondo, in «Immagini delle firme».
- Tutti i colori passano da variabili CSS (`:root`), con il tema scuro in `:root[data-theme="dark"]` e in `prefers-color-scheme` (dalla v13): niente colori scritti a mano nelle viste. Al buio le tinte delle persone si invertono.
- Accanto al titolo dell'argomento niente righe di meta, solo piccole etichette quando servono («prima: …», «in archivio», «aspetti una risposta da N giorni»).

## Decisioni prese sul prodotto

- **La conversazione è per persona; dentro, un blocco per argomento**, dal più recente, tutti mostrati senza «carica altri». **Il thread è sempre intero** (dalla v11): la persona sceglie quali thread vedi, non quali messaggi. Un thread compare da chi ci ha scritto e da chi ha ricevuto un mio messaggio (A: o Cc:), non da chi è solo in copia dei messaggi altrui. Un messaggio da leggere accende il pallino su tutte le persone del thread. Un argomento è un **thread** (`In-Reply-To`, `References`, thread di Gmail), non un oggetto: l'oggetto conta solo per i messaggi senza riferimenti. Il titolo è l'oggetto più recente, con «prima: …» se è cambiato.
- **Tre pulsanti fissi sotto ogni argomento**, sempre nello stesso posto: «Rispondi» / «Riprendi la bozza», «letto» / «da leggere», «Archivia» / «Ripristina». Agiscono sull'argomento intero. Niente azioni sui singoli messaggi, niente «Elimina»: la cancellazione è una pulizia semi-automatica delle notifiche dei servizi. Niente «Rispondi a tutti», niente «Silenzia», niente regole speciali per chi è solo in copia.
- **Stati di un argomento** (mappa completa in `docs/mockup_v10.md`): «letto» chiude l'argomento e resta chiuso anche dopo il ricaricamento; una email da altri lo riapre, anche dall'archivio (torna tutto). I miei messaggi non riaprono. Chiuso è per argomento, condiviso tra le persone. Ripristina → chiuso e letto. «da leggere» su un archiviato → ripristina e segna da leggere. Dopo «letto» o «Archivia» l'argomento attivo passa al prossimo da leggere della stessa persona.
- **Risposta**: va agli stessi destinatari dell'ultimo messaggio, io escluso, ognuno con la sua ×, più un campo per aggiungerne; il riquadro avvisa di chi non riceverà la risposta e di chi è nuovo. Una bozza per argomento, salvata mentre scrivo, segnata accanto alla persona e nel riassunto.
- **Destinatari nei box**: con un solo destinatario non si mostrano; con più di uno tutti, A: e Cc:, nome ed email, i primi 10 senza aprire niente.
- **«Leggi tutto»** (dalla v12, senza trascinamento): si taglia solo un testo di almeno 1.500 caratteri, a fine paragrafo, se resta nascosto almeno un terzo; mai dopo il saluto, un titoletto o i due punti, mai dentro un elenco. Se con una persona apro sempre tutto, i suoi messaggi non si tagliano più.
- **«Aspetti una risposta»**: etichetta più una voce compatta in basso nella lista persone; solo se il mio messaggio chiede qualcosa («?»), è degli ultimi 30 giorni e sono passati 3 giorni lavorativi.
- **Tastiera**: ↑ ↓ argomento, Maiusc+↑ ↓ persona, Invio apre o chiude, R / L / A i tre pulsanti, «/» cerca, Esc, Ctrl+Z, Ctrl+Invio. La lettera si vede nel suggerimento del pulsante; niente pannello delle scorciatoie.

## A che punto siamo

La v13 (`docs/mockup_v13.md`) ha chiuso i giri di soli mockup. Dal 28 ottobre 2026 si usa l'app sulla casella vera: il giro 1 dell'app (`docs/app_giro1.md`) ha prodotto la v14 del prototipo e dell'app; il giro 2 è aperto in `app/NOTE.md`. Decisioni, architettura e ordine di lavoro sono in `docs/implementazione.md` (desktop Linux con Tauri, motore in Rust, IMAP generico provato su Gmail, poche dipendenze scelte). Restano aperti dai mockup:

- canvas della v10, v11, v12 e v13 da pubblicare;
- le regole della mappa da estendere ai servizi;
- allegati delle bozze persi al ricaricamento (resta il nome).

Il lavoro riparte dall'ordine in `docs/implementazione.md`. Il motore in sola lettura è in `motore/`, l'app Tauri in sola lettura è in `app/`: per entrambi vedi il README. Ora si usa l'app sulla casella vera e la si mette a punto a giri, come i mockup (sotto).

Come si lavora un giro dell'app (deciso il 28 ottobre 2026):
1. **Note.** L'utente usa l'app e detta osservazioni: vanno in `app/NOTE.md`, una voce per punto (titolo in grassetto, cosa succede oggi nell'app, cosa si vuole). In questa fase si scrive solo nel file. I problemi che si vedono solo sulla casella vera si riproducono con un caso sintetico nel finto server.
2. **Prototipo prima, se serve.** Le voci che cambiano l'interfaccia passano prima da una versione nuova del mockup (v14, v15…), costruita come sopra. In fondo a `app/NOTE.md` la sezione «Cosa tocca il prototipo» dice quali sono.
3. **Ok al prototipo.** Si va avanti solo quando l'utente approva il mockup.
4. **Implementazione nell'app.** Si porta tutto in `app/` (le voci del prototipo e quelle solo dell'app), provato col finto server.
5. **Chiusura.** Le note vanno in `docs/app_giroN.md` con «Com'è andato il giro»; si aggiornano questo file e i README, poi commit se richiesto. L'app non si copia a ogni giro: cambia sul posto.

App e prototipo portano lo stesso numero, provvisorio (deciso il 29 ottobre 2026): la v14 è la prima per l'app, senza lo storico prima. Si cambia in `this.version` di `app/ui/index.html` insieme al mockup nuovo; il pannello «Prototipo» dell'app lo mostra.

Regole per l'app:
- **Mai i dati veri dell'app.** `~/.local/share/a10s/` (casella, password, database, allegati), `~/.local/share/it.elmisi.a10s/` (lo stato dell'interfaccia) e `~/.cache/a10s/` non si leggono, come `motore/private/`; ci sono i deny in `.claude/settings.json`.
- **Prove solo col finto server** del motore e i dati sintetici, con `A10S_DATI` in una cartella di prova.
- **Nelle prove, mai il desktop vero**: `xdg-open`, `xdg-user-dir` e `notify-send` vanno sostituiti da script finti in testa al `PATH`. Una volta un PDF sintetico è finito nella cartella Scaricati vera.
- **`ui/index.html` dell'app parte dalla v13 ma ora vive per conto suo.** Le modifiche all'interfaccia si fanno lì; i mockup restano per le idee.
- **Fermare i processi in un comando separato.** Il comando che li ferma va lanciato da solo. Se nella stessa riga c'è anche il comando che avvia il processo, `pgrep -f` trova la shell stessa e la chiude.
