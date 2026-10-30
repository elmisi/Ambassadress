# Dati del prototipo

| File | Cosa contiene | Chi lo vede |
| --- | --- | --- |
| `synthetic.js` | casella inventata, generata da `generate-synthetic.mjs` | tutti: canvas, sviluppo, git |
| `attachments/` | file veri (PDF, immagini, fogli, inviti) per gli allegati sintetici, generati insieme a `synthetic.js` | tutti |
| `testi-lunghi.mjs` | i testi delle email lunghe (relazioni, verbali, passaggi di consegne), importati dal generatore | tutti |
| `cases/*.json` | casi sintetici che riproducono un problema visto con i dati veri; `cases/make-*.mjs` li rigenera | tutti |
| `private/*.js` | export della tua casella vera (`tools/gmail-export.py`) | solo questo computer |
| `private/attachments/` | gli allegati veri salvati dall'export (`--no-attachments` per non salvarli) | solo questo computer |

`private/` è in `.gitignore`. In `.claude/settings.json` è vietato a Claude leggerla, modificarla o toccarla da shell.

## Usare i dati veri

```sh
# una volta: password per le app di Google in ~/.secrets/gmail-app-password.txt
# e l'indirizzo in ~/.secrets/gmail-user.txt
python3 tools/gmail-export.py --limit 1000          # scrive mockups/data/private/real.js
python3 tools/gmail-export.py --mbox takeout.mbox   # in alternativa, da Google Takeout
python3 tools/gmail-export.py --testi               # senza collegarsi: testo degli allegati salvati e codici (QR, codici a barre)
```

Se `real.js` c'è già, lo script scarica solo le email nuove: dalla più recente fino alla prima già presente nel file, al massimo `--limit`, e le aggiunge. Se le nuove sono più di `--limit` prende le più vecchie tra queste e dice quante ne restano: basta rilanciarlo. `--full` riscrive il file da zero con le `--limit` più recenti.

Poi apri il prototipo aggiungendo `?data=real` all’indirizzo, per esempio
`mockups/v9/project/Main.dc.html?data=real`.
In alto compare «Dati veri, solo in locale». Se il file manca il prototipo non mostra niente invece di ripiegare sui dati sintetici.

## Quando trovi un bug con i dati veri

1. Premi **Segnala**, in alto o sotto il messaggio che si vede male.
2. Scegli il tipo e scrivi cosa è successo descrivendo la forma, non il contenuto.
3. Controlla il riquadro scuro: è esattamente quello che consegni. Con i dati veri il contenuto delle email è già sostituito (lettere in x/X, cifre in 9, indirizzi e link in riferimenti come `⟦P3⟧`).
4. Copia il report e incollalo nella chat.

Dal report si ricostruisce un caso in `cases/<nome>.json` (modello: `cases/_modello.json`, oppure uno script `cases/make-<nome>.mjs` quando il caso è grande), si rigenera `synthetic.js` con `node mockups/data/generate-synthetic.mjs` e si corregge il prototipo finché il caso sintetico si vede bene. Nei casi tutto è inventato: persone, aziende, progetti, indirizzi e testi; del caso vero si tiene solo la forma.

Dalla v11 la casella sintetica imita la posta vera: ogni risposta cita tutto il thread sotto, con le intestazioni del programma di chi scrive (Gmail in italiano e in inglese, Outlook in italiano e in inglese, Apple Mail, iPhone, Thunderbird), firme di ogni forma e testi lunghi. Ci sono anche due thread a più persone scritti apposta: «Infiltrazioni dal terrazzo» (una persona solo in copia che non deve comparire nella lista, una risposta scritta dentro la citazione) e «Passaggio di consegne progetto Orione» (quattro persone, citazioni fino a quattro livelli).

Dalla v12 ci sono due casi di firme fatte di immagini: «Manutenzione caldaia, preventivo sostituzione» (Paolo Bassi, Outlook: il logo `image001.png` e due icone social `image002.png` e `image003.png`, piccole e uguali in tre messaggi, con il segnaposto `[cid:…]` nel testo; nello stesso thread `targhetta caldaia.jpg`, 31 KB, è una foto vera e deve restare visibile) e «Documenti per il 730» (Studio Conti, un logo `.gif` e un banner `.jpg` in un solo messaggio, riconosciuti dal nome). Le immagini piccole hanno un file SVG piccolo (logo con le iniziali o icona tonda).

Dalla v13: una seconda casella, `alex@studiomoro.esempio.it` (IMAP, non Gmail), elencata in `accounts` e tra gli alias, con due thread («Bagno di via Merulana, sopralluogo», «Campionatura cotto, ordine 3381»); le etichette dell'utente (Condominio, Casa, Lavoro, Viaggi, Tasse) aggiunte per oggetto; un messaggio di spam (`\Spam`) che non deve comparire da nessuna parte; le newsletter con due immagini esterne e un pixel che traccia.

Dalla v14: i PDF hanno più pagine (circa una ogni 95 KB della misura dichiarata, da 1 a 24) con una prima pagina riconoscibile, e accanto la miniatura `NNN.jpg` della prima pagina, fatta con `pdftoppm` come la farà il motore (senza `poppler-utils` i PDF restano senza miniatura). Le immagini hanno le misure in pixel (`width`, `height`), come il motore le leggerà dal file; un'immagine con `noDims` nel caso resta senza misure, come un file non ancora scaricato.

Casi presenti:

- `citazioni-annidate`: risposta sopra tre livelli di citazione con firma in mezzo.
- `inviti-calendario`: un collega con 39 messaggi, quasi tutti inviti, spostamenti e registrazioni di riunioni.
- `servizi-automatici`: notifiche di revisione codice, issue, build, sicurezza, ricevute, pacchi, newsletter e social.
- `copia-e-citazione-a-capo`: messaggi dove sei solo in copia con tanti destinatari, e intestazione «Il giorno … ha scritto:» spezzata su più righe.
- `indirizzi-multipli`: una persona con un indirizzo vecchio, fermo da due anni, e uno nuovo in uso.
- `inoltro-e-copia-a-meta` (v14): un preventivo di Ilaria Venturi inoltrato a Tommaso Orsini dentro lo stesso thread di Gmail, con la risposta di Tommaso; e una cena di classe dove Nadia Pellegrino entra in copia al quarto messaggio. Da Tommaso e da Nadia la parte di prima è chiusa in una riga.
- `immagini-piccole` (v14): un'impresa con due colleghi, Marta Sereni e Luca Gualtieri, con il logo `ATT00001.png` (52 KB ma 200×80 px), un `marchio.png` senza misure ripetuto dai due indirizzi dello stesso dominio, una foto vera, una schermata vera di 35 KB (820×460, resta visibile) e un'icona di 96×96 in un messaggio senza firma.
- `argomenti-per-thread`: un thread che cambia oggetto a metà (legato da `In-Reply-To` e `References`), due «Fattura» diverse con la stessa persona, un messaggio lungo per «Leggi tutto» e una domanda rimasta senza risposta.

Dalla v15, per le prove di `app/NOTE.md` («Prove da fare»), cinque casi fatti da `cases/make-prove-v15.mjs`. Le risposte hanno `"quote": true`: il generatore aggiunge sotto la citazione del messaggio prima dello stesso thread, nel formato del programma di chi scrive (`from.client`), come per i thread del generatore. Un allegato con `fileKey` è lo stesso file di un altro con la stessa chiave: stessi byte, anche con un altro nome.

- `da-ritrovare` (prova 1): «Sito dello studio», nove messaggi tra Valerio Pace (`valerio@pacedesign.esempio.it`), Teo Marras (`teo@pacedesign.esempio.it`) e Alex, con la decisione («il blog lo chiudiamo», «dal 1° dicembre passiamo a Nuvola Esempio») nel sesto, senza parole dell'oggetto; un capitolato condiviso da «Giulia Ferraro (tramite Google Drive)» `<drive-shares-dm-noreply@google.com>` e un suo commento da «Giulia Ferraro via Google Documenti» `<comments-noreply@docs.google.com>`, più Giulia Ferraro vera (`giulia.ferraro@solaria-impianti.esempio.it`); la ricetta dei pizzoccheri di Ermanno Villa, gennaio 2023, unico messaggio di quel mittente; la convocazione degli Orti del Parco arrivata a settembre per l'«assemblea del 12 ottobre»; codici brevi (pratica 2026/0417 e prot. 88213 dal Comune, CIG B4A7E2915C dal Consorzio di Bonifica alla seconda casella, codice cliente 7730021 da Gas Esempio) e nomi di file strani (`SCN_0047_20260918_planim-cat-fg812-part77-sub5.pdf`, `Capitolato_arredi_sala_consiliare_DEF(2).pdf`). Le domande della prova sono in `ritrovamento.json` (sotto).
- `thread-difficili` (prova 2): «Weekend in Umbria» con Irene Valli e Bruno Carli, dove al quarto messaggio Bruno cambia l'oggetto in «Umbria: si cambia, andiamo a Spello» restando nello stesso thread; «Tavolo su misura per la sala» di Fabrizio Bruni, che finisce con una domanda ad Alex da leggere; «Cantina in affitto» con Claudia Mancini, che finisce con una domanda di Alex di una settimana prima dell'ultimo messaggio della casella («aspetti una risposta»).
- `allegati-nascosti` (prova 3): un QR `qrcode.png` di 200×200 («in allegato il QR per l'ingresso», Michela Rota); uno screenshot `image.png` di 280×170 incollato nel testo (incorporato, con Content-ID e HTML, Ettore Bonfanti); la scansione stretta del codice a barre di un bollettino, `scansione.jpg` 900×100 (Patrizia Fumagalli); la stessa bozza dello statuto in tre messaggi dello stesso thread (Renato Fusco); lo stesso preventivo con due nomi, `PREV_2026_0311.pdf` da Colori Ciampi e `preventivo imbianchino soggiorno.pdf` da Sergio Alberti; due `documento.pdf` diversi, da Michela Rota e da Renato Fusco. Il QR e il codice a barre sono codici veri (dal giro 2 dell'app): il QR dice «FIERA FV-2026-0412» (la matrice viene da `qrencode`, scritta nel generatore), il codice a barre è l'EAN-13 8001234567897; `zbarimg` li legge nei `.eml` del finto server. Dal 30 ottobre il QR e il codice a barre del bollettino portano `code` («QR-Code», «EAN-13»), e i preventivi di Colori Ciampi e di Forniture Esempio hanno `text`, il contenuto del PDF, per la ricerca dentro gli allegati.
- `pulizia-importanti` (prova 6): messaggi importanti fatti solo di testo dentro i gruppi della pulizia: il codice di ritiro di un locker di Spedizioni Lampo prima dell'avviso «Pacco ritirato» (finisce in «Spedizioni concluse»), il promemoria di una visita cardiologica (Poliambulatorio Salus Esempio) e la ricevuta della quota della Scuola di Musica Esempio, entrambi con `List-Unsubscribe` e più vecchi di 14 giorni (newsletter). Le esclusioni: la programmazione del Cinema Esempio con la stella, una fattura PDF da Hosting Esempio (mittente newsletter), il thread della richiesta 48213 di Fibra Esempio in cui Alex ha risposto. Quattro «Fattura n. … del …» di Acqua Esempio (`avvisi@fornitore-acqua.example`), uguali a parte numeri e date, per la pulizia che impara. Dopo le prove (30 ottobre): Dominiamo Esempio (`noreply@dominiamo.example`), un registrar che manda una promozione e la conferma del rinnovo di `studiomoro.esempio.it`, fatta solo di testo; Chiara Vela (`chiara.vela@officinavela.example`) con cui Alex scrive, e due «Aggiornamento ai soci» di Officina Vela da `aggiornamenti@officinavela.example`, mandati con una piattaforma di newsletter.
- `indirizzi-condivisi` (prova 7): Jira da `jira@acme-esempio.atlassian.net` (`Auto-Submitted`, `Precedence: bulk`) con tre issue: ACM-142 «Errore nel login con SSO» e ACM-157 «Export CSV senza intestazioni» hanno messaggi con parole delle persone («ha commentato», «ti ha menzionato», «commented on», «mentioned you», «ha creato») e senza («changed the status», «assigned … to you», «Automation for Jira»), ACM-163 è di Luca Neri. Nomi: «Sara Conti (Acme)», «S. Conti (Acme)», «Sara Conti Bassi (Acme)», «Davide Russo (Acme)», «Luca Neri (Acme)». Sara Conti scrive anche dal suo indirizzo `sara.conti@acme-esempio.it`, prima come «Sara Conti» poi come «Sara Conti Bassi», e su Repohub (`notifications@repohub.example`, la pull request #88 con un commento). Un altro Luca Neri, `luca.neri@studio-altro.example`, scrive del corso di fotografia: è un'altra persona. `info@forniture-esempio.it`, senza intestazioni automatiche, firmato da Paola («Forniture Esempio - Paola») e da Gianni («Forniture Esempio»), alla seconda casella. Con i dati della v14 ACM-142 cade tra gli avvisi di sicurezza, per la parola «login» nell'oggetto.

## Le domande della prova 1

`ritrovamento.json` è un elenco di domande scritte come le scriverebbe l'utente, ciascuna con i messaggi che devono uscire tra i primi risultati:

```js
[{ q: 'assemblea del 12 ottobre', top: 3, note: 'cosa prova',
   expect: [{ from: 'indirizzo del mittente', subject: 'oggetto esatto',
              text: 'frase nel testo del messaggio, fuori dalla citazione (facoltativa)',
              attachment: 'nome esatto di un allegato (facoltativo)' }] }]
```

`text` e `attachment` servono quando mittente e oggetto non bastano a dire quale messaggio è (le risposte di un thread hanno tutte lo stesso oggetto). Ogni `expect` corrisponde a un solo messaggio di `synthetic.js`.

La prova gira nel prototipo (dalla v15), con Playwright: `support.js` mette l'istanza della tavola in `window.__dcInst`; per ogni domanda si imposta `state.q`, si chiama `search(state)` e si guarda se gli `expect` stanno nei primi `top` di `searchShown` (gli indici dei messaggi in `AMB_DATA.messages`). Con la v15: 23 domande su 23 (le ultime tre cercano dentro gli allegati e tra i codici).

## Formato

Un file `.js` che assegna `window.AMB_DATA` (così si carica anche aprendo l'HTML dal disco):

```js
window.AMB_DATA = {
  schema: 1,
  meta: { source: 'synthetic' | 'gmail-imap' | 'mbox', count, shiftToNow },
  account: { address, aliases: [] },
  messages: [{
    id, date, dateRaw, internalDate,
    from: { name, address }, sender, replyTo: [], to: [], cc: [], bcc: [],
    subject, messageId, inReplyTo, references: [],
    text, html, truncated,
    attachments: [{ filename, mimeType, size, inline, contentId, path, preview, width, height, pages, thumb, text, code }],  // path e thumb: relativi a mockups/data; text: il testo dentro il file (per la ricerca); code: il tipo di codice se l'immagine è un QR o un codice a barre («QR-Code», «EAN-13»)
    otherParts: [], headers: [[nome, valore]],
    flags: { seen, flagged },
    gmail: { threadId, labels: [] },
    size, problems: []
  }]
};
```

`shiftToNow` (solo nei dati sintetici) sposta le date in modo che l'ultimo messaggio sia di oggi.
