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

Casi presenti:

- `citazioni-annidate`: risposta sopra tre livelli di citazione con firma in mezzo.
- `inviti-calendario`: un collega con 39 messaggi, quasi tutti inviti, spostamenti e registrazioni di riunioni.
- `servizi-automatici`: notifiche di revisione codice, issue, build, sicurezza, ricevute, pacchi, newsletter e social.
- `copia-e-citazione-a-capo`: messaggi dove sei solo in copia con tanti destinatari, e intestazione «Il giorno … ha scritto:» spezzata su più righe.
- `indirizzi-multipli`: una persona con un indirizzo vecchio, fermo da due anni, e uno nuovo in uso.
- `argomenti-per-thread`: un thread che cambia oggetto a metà (legato da `In-Reply-To` e `References`), due «Fattura» diverse con la stessa persona, un messaggio lungo per «Leggi tutto» e una domanda rimasta senza risposta.

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
    attachments: [{ filename, mimeType, size, inline, contentId, path, preview }],  // path: relativo a mockups/data
    otherParts: [], headers: [[nome, valore]],
    flags: { seen, flagged },
    gmail: { threadId, labels: [] },
    size, problems: []
  }]
};
```

`shiftToNow` (solo nei dati sintetici) sposta le date in modo che l'ultimo messaggio sia di oggi.
