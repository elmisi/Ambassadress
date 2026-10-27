# Mockup v12

Note dopo la v11, fatte nella v12.

- **L'icona nella favicon e nel logo.** Oggi nella v11 la scheda del browser non ha una favicon e il logo in alto a sinistra è una «A» bianca in un cerchio rosso. Si usa l'icona scelta (sigillo H2 su fascia di pergamena L3, file in `mockups/icona/`): `sigillo.svg` per la favicon, dove la fascia sparirebbe, e il sigillo accanto al nome nel logo. Anche in `mockups/index.html`.
- **Dal pannello «Prototipo» si torna alla scelta della versione.** Nel pannello che si apre dal pulsante «Dati sintetici» / «Dati veri» un link a `mockups/index.html`, per passare a un'altra versione senza riscrivere l'indirizzo.
- **«Segnala» salva la segnalazione in un file SQLite.** Oggi nella v11 «Segnala» prepara il report da copiare o scaricare in `.json`, e va incollato in chat a mano.
  - **Come.** Un piccolo server locale, `tools/segnalazioni.py`, solo libreria standard di Python, acceso a mano quando si prova il prototipo. Il prototipo gli manda la segnalazione con un clic («Salva la segnalazione») e mostra la conferma con il numero («Segnalazione 12 salvata»). Se il server non è acceso restano «Copia il report» e «Scarica .json» come oggi, con un avviso che dice come accenderlo. Vale anche dal canvas, se il browser lascia parlare la pagina con il server locale; se no, si ripiega su copia e scarica.
  - **Cosa si salva.** Tutto quello che il report contiene oggi (tipo, descrizione, dati del messaggio già sostituiti con i dati veri, stato del prototipo, errori), più la versione del prototipo, la fonte dei dati (sintetici o veri), la data, e lo stato della segnalazione.
  - **Dove.** `segnalazioni/segnalazioni.sqlite`, nel repository ma in `.gitignore` e fuori da `mockups/data/private/`, così Claude lo può leggere per lavorare sulle segnalazioni. Con i dati veri il contenuto è già sostituito (lettere in x, cifre in 9, indirizzi in `⟦P3⟧`) prima di uscire dal prototipo.
  - **Stato.** Lo gestisce Claude quando lavora sulle segnalazioni: nuova, in lavorazione, risolta (con la versione che la risolve), non riproducibile, scartata, con una nota. Lo stesso script serve anche da riga di comando per elencarle, leggerle e cambiare lo stato. Nel prototipo non si aggiunge altro.
  - **Procedura.** In `CLAUDE.md` va scritto come si lavora sulle segnalazioni: leggere le nuove, ricostruire il caso sintetico, correggere, aggiornare lo stato.
- **«Leggi tutto» senza trascinare.** Oggi nella v11 la riga di «Leggi tutto» si trascina più su o più giù e il taglio si sposta per quella persona. Il trascinamento si toglie: il punto del taglio deve venire giusto da solo, con regole che funzionano bene in automatico. Da provare sui testi lunghi dei dati sintetici (verbale, passaggio di consegne, relazione del terrazzo, itinerario) e su quelli brevi:
  - si taglia solo un testo davvero lungo, intorno ai 1.500 caratteri (oggi dai 680), e solo se quello che resta nascosto è almeno un terzo del testo, altrimenti si mostra tutto;
  - il taglio cade a fine paragrafo, mai a metà di un elenco puntato o numerato, mai subito dopo un titoletto («Situazione rilevata», «1. Dove siamo»): in quel caso si va al paragrafo dopo;
  - resta visibile almeno il primo paragrafo che dice qualcosa, non solo il saluto («Buongiorno Alex,»);
  - resta, perché è automatica, la regola che se con una persona apro sempre tutto i suoi messaggi non si tagliano più.
- **Nascondere anche le firme fatte di immagini.** Oggi nella v11 si nascondono solo le immagini dentro il testo con un riferimento interno (Content-ID); i loghi e le icone delle firme che arrivano come allegati (`image001.png`, `logo.gif`, `firma.jpg`, le icone dei social) compaiono tra gli allegati del messaggio, uno per ogni email della stessa persona. Quando si riesce a capirlo, si nascondono con la firma e tornano con «Mostra firma»:
  - immagini piccole (sotto i 40 KB circa) con nomi da firma: `image001.png`, `Outlook-….png`, logo, firma, signature, banner, facebook, linkedin, instagram;
  - la stessa immagine (stesso nome e dimensione) che torna in più messaggi della stessa persona;
  - non spariscono dalla vista «Allegati», dove restano sotto la persona come «immagini della firma», per non perdere niente.

  Serve un caso sintetico con firme Outlook con logo e icone dei social, ripetute in più messaggi, e un'immagine vera piccola nello stesso messaggio, che invece deve restare visibile.

## Com'è andata nella v12

**Come è fatta.** La v12 è la v11 con le cinque note.

- **Icona.** La favicon e il logo accanto al nome sono L3, il sigillo sulla fascia di pergamena con il contorno sottile (`icona-senza-fondo.svg`), ritagliata intorno alla fascia e incorporata nella pagina, così vale anche sul canvas. Prende il posto della «A» nel cerchio. In `mockups/index.html` c'è come favicon e accanto al titolo. All'inizio era solo il sigillo (`sigillo.svg`); l'utente ha chiesto L3 o L4. A 16 e 32 px le due quasi non si distinguono: L4 ha i bordi del rotolo più scuri, che si vedono solo in grande.
- **Pannello «Prototipo».** In fondo c'è il link «Tutte le versioni del prototipo».
- **Segnalazioni.** `tools/segnalazioni.py`, solo libreria standard, gira su `127.0.0.1:8797`; da `file://` funziona perché risponde con CORS aperto. Quando si apre «Segnala», il prototipo controlla se il server è acceso:
  - se è acceso, c'è solo «Salva la segnalazione»; dopo il salvataggio la finestra si chiude con l'avviso «Segnalazione N salvata»;
  - se è spento, restano «Copia il report» e «Scarica .json», con una riga che dice come accenderlo.

  Il file è `segnalazioni/segnalazioni.sqlite`, in `.gitignore`. Ogni riga ha data, versione, fonte, se è anonima, tipo, «cosa è successo», «cosa ti aspettavi», il report intero, lo stato, la versione che la risolve e una nota. Da riga di comando ci sono `elenco`, `mostra N` e `stato N …`; per «risolta» serve `--versione`.
- **«Leggi tutto».** Il trascinamento è tolto, insieme al passo per persona che imparava. Restano le regole automatiche:
  - si taglia solo da 1.500 caratteri, a fine paragrafo, con almeno 400 caratteri visibili e almeno un terzo nascosto;
  - mai dopo un saluto, un titoletto o una riga che finisce con i due punti;
  - mai tra due voci di un elenco;
  - un testo senza paragrafi si taglia a fine riga o di frase intorno ai 700 caratteri;
  - «se apri sempre tutto, non si taglia più» resta.

  Sui testi lunghi sintetici il taglio cade dopo il primo paragrafo che dice qualcosa: relazione del terrazzo 714 caratteri visibili su 3.097, preventivo 624 su 2.354, passaggio di consegne 644 su 4.787, verbale 517 su 6.393, itinerario 432 su 2.771. Le email sotto i 1.500 caratteri ora si vedono intere: prima si tagliavano dai 680.
- **Immagini della firma.** Un'immagine è della firma se pesa meno di 40 KB e ha un nome da firma (`image001.png`, `Outlook-…`, logo, firma, signature, banner, nomi dei social), oppure se torna uguale, stesso nome e peso, in più messaggi della stessa persona.
  - Non sta tra gli allegati del messaggio. Si vede con «Mostra firma», sotto la firma di testo e prima della citazione.
  - Non conta negli allegati della persona, nella ricerca e nelle anteprime della lista.
  - In Allegati è in fondo, nel gruppo «Immagini delle firme», con «firma di Paolo Bassi, in 3 messaggi»; le copie dello stesso logo sono unite per persona.
  - Nel report c'è `signatureImages`.

**Lavoro in più rispetto alle note**

- Il segnaposto di Outlook `[cid:image001.png@…]` si toglie dal testo, anche dentro la citazione.
- La firma aziendale senza «-- » ora si riconosce anche con «S.n.c.», «S.a.s.», «S.r.l.s.», «Ltd» e «Inc» a fine riga: prima solo con «S.r.l.» e «S.p.A.», e la firma di Bassi restava nel testo.
- Nella tabella `segnalazioni` la data è quella del server, in ora locale; quella del browser resta nel report.

**Da sapere**

- A server spento, il controllo lascia un errore di rete nella console del browser, «connection refused». È l'unico errore in console e non si può evitare.
- Una foto incollata in Outlook si chiama anche lei `image00N.png`: se pesa meno di 40 KB la v12 la prende per un'immagine della firma. Nei casi sintetici la foto vera ha un nome suo.
- Gli allegati sintetici sono rinumerati di nuovo: 401 messaggi, prima 396.
- Il canvas non è provato: il salvataggio va se il browser lascia parlare la pagina con `127.0.0.1`, altrimenti restano copia e scarica.

**Restano aperti**

- Quelli di prima: canvas della v10, v11 e v12 da pubblicare, «eliminato altrove», servizi, allegati delle bozze al ricaricamento, nome breve dell'app.
