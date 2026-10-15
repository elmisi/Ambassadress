# Mockup v10 · Stati chiari

Note raccolte dopo la v9 in `mockups/PROSSIMO-GIRO.md` e fatte nella v10 (`mockups/v10/project/Main.dc.html`). In fondo, «Com'è andata nella v10» dice dove la v10 si discosta da queste note e cosa resta aperto.

- **Ricerca e messaggi archiviati.** Se nella ricerca trovo un messaggio archiviato e lo apro, nella conversazione della persona non lo vedo, perché è archiviato.
- **«Leggi tutto» e «Riduci».** Spesso non servono, perché «Mostra testo citato» fa meglio. Quando servono, a volte «Leggi tutto» è in un punto che non mi aspetto: oggi nella v9 un messaggio sopra i 300 caratteri si taglia a 240, contando solo i caratteri, anche a metà frase. Per prima cosa il taglio va a fine paragrafo o di frase. Poi lo posso spostare io, trascinando la riga di «Leggi tutto» più su o più giù, e il sistema impara dalle mie azioni: se con una persona lo sposto sempre più in basso, i suoi messaggi si tagliano più in basso; se apro sempre tutto, i suoi messaggi non si tagliano più. Nel mockup l'apprendimento è simulato con questa regola per persona, salvata con il resto dello stato; nel prodotto vero potrebbe farlo l'AI locale.
- **«Segnala» sul singolo messaggio.** Ha smesso di funzionare. Da controllare bene in tutti i punti in cui c'è: in alto, sotto ogni messaggio e nell'avviso di errore («Segnala con i dettagli»), fino al report copiato e scaricato, con i dati sintetici e con quelli veri.
- **Messaggi troppo alti.** In generale i messaggi sono troppo lunghi in verticale, come se l'interlinea fosse troppa. Da compattare senza impastare tutto. Oggi nella v9 il testo ha corpo 15,5 px e interlinea 1,6, le righe vuote dell'email restano tutte, e tra testata, testo e azioni ci sono margini pieni. Da provare: interlinea intorno a 1,45, più righe vuote di fila ridotte a una, margini interni del box più stretti; resta lo spazio tra un paragrafo e l'altro e tra un messaggio e l'altro.
- **Via la riga accanto al titolo dell'argomento.** Il testo tipo «1 messaggio, ultimo 10 giu» non lo legge nessuno: il numero di messaggi non serve e la data dell'ultimo passa inosservata, tanto c'è in ogni messaggio e nel riassunto quando l'argomento è chiuso. Si toglie. Accanto al titolo resta solo, quando serve, l'etichetta «aspetti una risposta».
- **Box dei messaggi più larghi.** A volte i box sembrano prendere solo metà della finestra: oggi nella v9 sono larghi al massimo 640 px. Li porterei sempre a 3/4 della colonna della conversazione. Aiuta anche a renderli meno alti.
- **Errori in console.** Ce ne sono tanti di questo tipo: `Uncaught TypeError: Cannot read properties of undefined (reading 'p:<indirizzo>')`.
- **Letto vuol dire chiuso, anche dopo il ricaricamento.** Quando segno un argomento come letto si chiude, e deve restare chiuso anche se ricarico la pagina. Se arriva una email nuova su quell'argomento si riapre. Oggi nella v9 lo stato chiuso non viene salvato e si perde al ricaricamento.
- **Email nuova su un argomento archiviato.** Vedi la mappa qui sotto: l'argomento torna tutto.
- **Destinatari nei messaggi.** Se il messaggio ha un solo destinatario, nel box non si mostra, perché si vede già in alto. Se ci sono più destinatari in A:, o persone in copia, si mostrano tutti, A: e Cc:, con nome ed email, almeno i primi 10 senza doverli aprire; oltre i 10 un «e altri N» li mostra tutti. Oggi nella v9 i destinatari stanno dietro il pulsante «con N persone» e si vedono solo i nomi.
- **Tre pulsanti fissi sotto ogni argomento.** «Rispondi», «letto» o «da leggere», «Archivia», sempre in quest'ordine e sempre nella stessa posizione. Il pulsante in mezzo cambia solo la scritta: «letto» se ci sono messaggi da leggere, «da leggere» se è tutto letto. Ci sono anche sotto il riassunto di un argomento chiuso. Oggi nella v9 «Segna come letto» compare solo se c'è qualcosa da leggere e «Segna da leggere» è un link a parte, quindi i pulsanti si spostano. Per ogni stato i pulsanti sono nella tabella «Stati» qui sotto.
- **A chi va «Rispondi».** Oggi nella v9 la risposta va solo alla persona di cui guardo la conversazione, senza copia, anche in un argomento con più persone o se l'ultimo messaggio l'ha scritto un altro. Di default la risposta va agli stessi destinatari dell'ultimo messaggio dell'argomento: in A: chi l'ha scritto e gli altri in A:, in Cc: chi era in copia, io escluso. In cima al riquadro di risposta si vedono come nei box (A: e Cc:, nome ed email), ognuno con la sua × per toglierlo, più un campo per aggiungerne. Niente pulsante «Rispondi a tutti»: i pulsanti sotto l'argomento restano tre. Se l'argomento è a due resta «Rispondi a Anna». Se tolgo qualcuno o aggiungo una persona nuova, il riquadro lo dice («Marco non riceverà questa risposta»).
- **Una bozza per argomento.** Oggi nella v9 c'è un solo testo in scrittura per tutta l'app: passa da un argomento all'altro, «Scrivi» lo cancella e il ricaricamento lo perde. Ogni argomento ha al massimo una bozza, più una per ogni nuovo messaggio non inviato; niente cartella «Bozze». La bozza sta dentro l'argomento: riaprendo la risposta riprendo testo, destinatari e allegati. Si salva mentre scrivo, sopravvive al ricaricamento e si allinea con le bozze IMAP aggiornando sempre la stessa, senza crearne di nuove. Si vede nel riassunto di un argomento chiuso («bozza: …»), con un segno discreto accanto alla persona, e «Rispondi» diventa «Riprendi la bozza» nello stesso posto. «Annulla» butta via la bozza con l'avviso «Annulla», come le altre azioni. La bozza non cambia letto, chiuso o archiviato: un argomento chiuso o archiviato può avere una bozza.
- **«Aspetti una risposta».** Se l'ultimo messaggio di un argomento è mio, è andato ad altri e sono passati 3 giorni lavorativi, accanto al titolo (e nel riassunto se è chiuso) compare «aspetti una risposta da 4 giorni». In fondo alla lista persone una voce compatta «Aspetti una risposta (5)» li raccoglie, archiviati compresi. Si toglie con la × sull'etichetta; sparisce da sola quando arriva la risposta. Nessun pulsante in più e nessuna impostazione; più avanti l'AI locale potrà mostrarla solo quando il mio messaggio chiede davvero qualcosa. Non cambia letto, chiuso o archiviato. Da tenere leggera: un'etichetta e una voce, niente di più.
- **Argomenti per thread, non per oggetto.** Oggi nella v9 l'argomento è fatto dai messaggi con lo stesso oggetto: se l'oggetto cambia a metà il thread si spezza in due, e thread diversi con un oggetto generico («Fattura», «Domanda») finiscono insieme, così «letto» e «Archivia» toccano anche quelli che non c'entrano. Si raggruppa per thread (`In-Reply-To`, `References`, thread di Gmail); l'oggetto vale solo per i messaggi senza riferimenti. Il titolo è l'oggetto più recente, con «prima: …» in piccolo se è cambiato. Due thread con lo stesso oggetto restano due argomenti. Niente spostamento a mano per ora: i casi sbagliati passano da «Segnala». Serve un caso sintetico nuovo, con un oggetto cambiato a metà e due «Fattura» diverse.
- **Tastiera.** Oggi nella v9 ci sono solo «/», Esc, Ctrl+Z e Ctrl+Invio. Si aggiungono pochi tasti, con le lettere dei pulsanti: ↓ / ↑ argomento successivo / precedente (un bordo mostra quello attivo), Maiusc+↓ / Maiusc+↑ persona successiva / precedente, Invio apre o chiude l'argomento, R «Rispondi», L «letto» / «da leggere», A «Archivia» / «Ripristina». La lettera si vede nel suggerimento passando sopra il pulsante; niente pannello delle scorciatoie. Non valgono mentre scrivo in un campo.
- **Dopo «letto» o «Archivia», il prossimo da leggere.** L'argomento attivo (il bordo e lo scorrimento) passa al prossimo argomento da leggere della stessa persona, senza aprire o chiudere niente. Se la persona non ha altro da leggere resto dove sono: alla persona successiva passo io, con Maiusc+↓ o col mouse. Niente di nuovo nell'interfaccia.

## Stati ed eventi di un argomento

Un argomento è un thread nella conversazione di una persona. Questa mappa dice cosa succede per ogni stato e ogni evento.

### Regole generali

1. **Tutto si salva.** Letto, chiuso e archiviato sopravvivono al ricaricamento della pagina. Ricaricare non cambia mai lo stato.
2. **Chiuso resta chiuso.** Un argomento chiuso, col nodo o con «letto», resta chiuso finché non lo riapro io o non arriva un messaggio scritto da altri.
3. **I miei messaggi non riaprono.** Un messaggio scritto da me, mandato da qui o da un altro client, non riapre un argomento chiuso e non lo toglie dall'archivio. Aggiorna solo il riassunto.
4. **L'archivio vale per l'argomento intero.** Si archivia e si ripristina tutto l'argomento, non i singoli messaggi. Archiviare segna anche come letto. Quando arriva un messaggio di altri l'argomento esce dall'archivio con tutti i messaggi, come in Gmail. Questo risolve anche la nota «Ricerca e messaggi archiviati».
5. **Aperto e chiuso esistono solo qui.** Letto e archiviato sono flag IMAP e si allineano nei due sensi con Gmail e il telefono. Quello che faccio altrove vale come se l'avessi fatto qui: letto altrove equivale a «letto» (quindi chiude), archiviato altrove equivale ad «Archivia».
6. **Annulla torna indietro esatto.** Riporta lo stato precedente, compreso aperto o chiuso.
7. **«Torna allo stato iniziale»** azzera anche i chiusi: tutti gli argomenti tornano aperti, con i flag di lettura di Gmail.
8. **La persona segue i suoi argomenti.** Ha il pallino se almeno un argomento ha messaggi da leggere (S1 o S4). Va tra «chi ha tutto archiviato» se tutti i suoi argomenti sono archiviati, e ne esce quando uno torna.
9. **I pulsanti non si spostano.** Sotto ogni argomento ci sono sempre tre pulsanti nello stesso ordine; cambia solo la scritta di quello in mezzo e, in archivio, di quello a destra.

### Stati

| | Lettura | Vista | Dove | Pulsanti |
| --- | --- | --- | --- | --- |
| **S1** | da leggere | aperto | conversazioni | Rispondi · letto · Archivia |
| **S2** | letto | aperto | conversazioni | Rispondi · da leggere · Archivia |
| **S3** | letto | chiuso, riassunto in una riga | conversazioni | Rispondi · da leggere · Archivia |
| **S4** | da leggere | chiuso, il riassunto dice «2 da leggere» | conversazioni | Rispondi · letto · Archivia |
| **S5** | letto | nascosto; chiuso quando lo mostro | archivio | Rispondi · da leggere · Ripristina |
| **R** | come prima | aperto, con la risposta in scrittura | conversazioni | Rispondi (premuto) · come prima · Archivia |

S4 nasce solo se chiudo col nodo un argomento che ha ancora messaggi da leggere: si chiude, resta da leggere e la persona tiene il pallino.

### Mappa

«—» vuol dire che l'evento non si può fare da quello stato (il pulsante non c'è o ha l'altra scritta).

**Azioni mie, qui**

| Evento | S1 aperto, da leggere | S2 aperto, letto | S3 chiuso, letto | S4 chiuso, da leggere | S5 archiviato |
| --- | --- | --- | --- | --- | --- |
| letto | → S3 | — | — | → S3 ¹ | — |
| da leggere | — | → S1 | → S1 ² | — | → S1 ³ |
| Chiudi col nodo | → S4 | → S3 | — | — | resta S5 ⁴ |
| Apri col nodo | — | — | → S2 | → S1 | resta S5 ⁴ |
| Rispondi | → R | → R | → R, si apre ⁵ | → R, si apre ⁵ | → R, resta in archivio ⁶ |
| Archivia | → S5, tutto letto | → S5 | → S5 | → S5, tutto letto | — |
| Ripristina | — | — | — | — | → S3 ⁷ |
| Apro dalla ricerca, dagli allegati o dalla linea del tempo | resta S1, scorre al messaggio | resta S2, scorre al messaggio | → S2, scorre al messaggio | → S1, scorre al messaggio | resta S5, mostrato aperto nella conversazione con «in archivio» ⁸ |
| Ricarico la pagina | resta S1 | resta S2 | resta S3 | resta S4 | resta S5 |

**Eventi che arrivano da fuori**

| Evento | S1 aperto, da leggere | S2 aperto, letto | S3 chiuso, letto | S4 chiuso, da leggere | S5 archiviato |
| --- | --- | --- | --- | --- | --- |
| Arriva una email da altri | resta S1, un da leggere in più | → S1 | → S1, si riapre | → S1, si riapre | → S1, torna tutto: i vecchi letti, il nuovo da leggere |
| Arriva un mio messaggio mandato da un altro client | resta S1 | resta S2 | resta S3, riassunto aggiornato | resta S4, riassunto aggiornato | resta S5, il messaggio entra in archivio |
| Letto altrove (tutti i da leggere) | → S3 ⁹ | — | — | → S3 | — |
| Letto altrove (solo alcuni) | resta S1, contatore aggiornato | — | — | resta S4, contatore aggiornato | — |
| Segnato da leggere altrove | resta S1 | → S1 | → S1 | resta S4 | resta S5 ³ |
| Archiviato altrove | → S5 | → S5 | → S5 | → S5 | — |
| Tolto dall'archivio altrove | — | — | — | — | → S3, oppure S1 se ha messaggi da leggere |

**Mentre scrivo una risposta (R)**

| Evento | Cosa succede |
| --- | --- |
| Invia | → S2: il mio messaggio in fondo, tutto quello che c'era sopra diventa letto. Se l'argomento era archiviato resta in archivio (regola 3). |
| Premo di nuovo «Rispondi», o lascio l'argomento | la risposta si chiude e resta come bozza; l'argomento torna allo stato di prima, anche chiuso se era chiuso |
| Annulla la risposta | la bozza si butta via, con l'avviso «Annulla»; l'argomento torna allo stato di prima |
| letto | segna letto, ma l'argomento resta aperto finché scrivo; quando invio o annullo vale la riga corrispondente |
| Chiudi col nodo | — mentre scrivo il nodo non chiude, come già oggi nella v9 |
| Archivia | → S5, la risposta si chiude e resta come bozza nell'argomento archiviato |
| Arriva una email da altri | la risposta resta aperta; il messaggio nuovo compare sopra, segnato come nuovo, con un avviso discreto |
| Arriva un mio messaggio da un altro client | compare sopra, nient'altro |
| Letto altrove | nessun effetto visibile finché scrivo; quando invio o annullo vale la riga corrispondente della mappa |
| Archiviato altrove | la risposta resta aperta finché invio o annullo, poi l'argomento va in archivio |
| Ricarico la pagina | resta tutto: l'argomento e la risposta si riaprono com'erano, con la bozza |

### Note

1. I pulsanti stanno anche sotto il riassunto di un argomento chiuso, quindi «letto» si preme senza doverlo aprire.
2. «da leggere» vuol dire «voglio ritrovarlo», quindi l'argomento si riapre.
3. Un argomento archiviato non ha messaggi da leggere. Premere «da leggere» lo ripristina e lo segna da leggere. Se invece Gmail segna da leggere un messaggio archiviato, qui non cambia niente finché l'argomento resta in archivio, perché in Gmail resta archiviato anche lui.
4. Aprire o chiudere un argomento archiviato, mentre lo guardo con «Mostra archiviati» o dalla ricerca, serve solo a leggerlo e non si salva: in archivio è sempre chiuso.
5. Da chiuso, «Rispondi» apre l'argomento e la risposta insieme. Lo stato di lettura non cambia finché non invio.
6. Rispondo dentro l'argomento archiviato, che resta in archivio anche dopo l'invio. Un messaggio di conferma lo dice: «Inviato. L'argomento resta in archivio.»
7. Ripristinato torna chiuso e letto, con il lampeggio per trovarlo nella conversazione.
8. Risolve la nota «Ricerca e messaggi archiviati»: dalla ricerca vedo l'argomento intero al suo posto, anche se è archiviato.
9. Se l'argomento è quello che ho davanti in quel momento, si chiude solo quando lo lascio, per non farlo sparire mentre lo leggo.

### Da provare nel mockup

Il mockup ha dati fissi e non riceve email. Per provare la tabella «Eventi che arrivano da fuori» serve un piccolo pannello di simulazione sull'argomento selezionato: «Arriva una email da altri», «Arriva un mio messaggio», «Letto altrove», «Segnato da leggere altrove», «Archiviato altrove», «Tolto dall'archivio altrove».

### Dubbi, come li ha risolti la v10

Le proposte sono state applicate così come erano scritte, tranne dove indicato.

- **Argomento con più persone.** Lo stato chiuso è per argomento: se lo chiudo da Anna è chiuso anche da Marco.
- **Eliminato altrove.** Non simulato nel mockup; la proposta resta: sparisce dall'argomento senza altri cambi di stato, e se era l'ultimo sparisce l'argomento.
- **Ripristina.** Torna sempre chiuso e letto (S3), con il lampeggio.
- **«da leggere» su un argomento archiviato.** Lo ripristina e lo segna da leggere.
- **Servizi.** Non toccati nella v10: restano come nella v9.

## Com'è andata nella v10

Fatto tutto quello che c'è sopra, provato con i dati sintetici, servito in locale e aperto dal disco, anche a 390 px di larghezza.

**Dove la v10 si discosta dalle note**

- **«Aspetti una risposta».** Con la regola alla lettera (ultimo messaggio mio, 3 giorni lavorativi) le voci erano 45, quasi tutte «grazie» di mesi prima. Conta solo se il mio messaggio contiene un «?» ed è degli ultimi 30 giorni; nei dati sintetici resta solo Ugo Bertolini. La voce «Aspetti una risposta (N)» è fissata in basso, sopra «Servizi e notifiche», invece che in fondo alla lista persone, dove con 62 persone non si vedeva.
- **Riga accanto al titolo.** È sparito anche lo stato delle riunioni; nell'indice a destra restano solo i titoli.
- **«Leggi tutto».** Si taglia solo sopra circa 680 caratteri, a fine paragrafo o di frase, vicino a 420 caratteri. La riga si trascina (o ↑ ↓ sulla maniglia): ogni passo moltiplica o divide per 1,6 il punto di taglio di quella persona. Dopo tre «Leggi tutto» con la stessa persona, i suoi messaggi non si tagliano più. I due apprendimenti lo dicono con un avviso che si può annullare.
- **Destinatari tolti.** Se tolgo dai destinatari la persona di cui guardo la conversazione, il messaggio inviato compare nella conversazione di chi lo riceve.
- **Simulazione.** Il pannello sta dentro il pulsante «Dati sintetici» in alto, nella sezione «Simula su «titolo»», e agisce sull'argomento attivo. Ci sono: email da altri, mio messaggio da altro client, letto altrove, da leggere altrove, archiviato altrove, tolto dall'archivio altrove. I messaggi simulati restano salvati e «Torna allo stato iniziale» li toglie.

**Bug trovati e corretti**

- «Segnala» non si apriva e la console dava `Cannot read properties of undefined (reading 'p:…')`: il report leggeva un campo di stato (`more`) che non esiste più dalla v8.
- Le scorciatoie della v9 erano agganciate a `.app` e si perdevano col fuoco dopo ogni ridisegno: ora sono sulla finestra.
- Sul telefono la lista persone scorreva sopra la conversazione (già nella v9): ordine delle regole CSS.

**Restano aperti**

- Gli allegati di una bozza, dopo il ricaricamento, tengono il nome ma non il file.
- «Eliminato altrove» e i servizi, vedi sopra.
- Il canvas della v10 su claude.ai non è ancora pubblicato.
