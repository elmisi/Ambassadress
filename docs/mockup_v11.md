# Mockup v11 · Thread interi

Note raccolte dopo la v10 in `mockups/PROSSIMO-GIRO.md` e fatte nella v11 (`mockups/v11/project/Main.dc.html`). In fondo, «Com'è andata nella v11».

- **Il thread intero in ogni conversazione.** Oggi nella v10 un messaggio scritto da altri va solo nella conversazione di chi l'ha scritto; i miei vanno a tutti i destinatari. In un thread tra me, Anna e Marco, da Anna vedo i suoi messaggi e i miei, ma non le risposte di Marco, anche se Anna era in copia: il thread si spezza a seconda della persona che guardo. La vista per persona resta, ma la persona sceglie **quali thread** vedo, non **quali messaggi**: dentro, il thread è sempre intero, con tutti i messaggi in ordine e il nome di chi scrive, uguale in ogni conversazione in cui compare.
  - **In quali conversazioni compare un thread.** In quella di ogni persona che ci ha scritto almeno un messaggio, e di ogni persona a cui ho scritto io (A: o Cc:). Chi è solo in copia dei messaggi degli altri non riceve il thread: così le email con lunghe liste in copia non riempiono la lista persone.
  - **Pallino.** Un messaggio di Marco da leggere accende il pallino su tutte le persone in cui compare il thread (regola 8 della mappa: la persona ha il pallino se un suo argomento ha da leggere). Leggendolo da una parte si spegne dappertutto, perché letto, chiuso e archiviato sono già per argomento.
  - **Cosa non cambia.** La mappa di stati ed eventi della v10 (`docs/mockup_v10.md`) resta valida: lo stato era già per argomento. «Rispondi» va già agli stessi destinatari dell'ultimo messaggio del thread. Da controllare con il thread intero: «chi ha tutto archiviato», «Aspetti una risposta», il contatore dei da leggere, la ricerca e la linea del tempo, che non devono contare due volte lo stesso messaggio.
  - Serve un caso sintetico nuovo: un thread tra me e due persone, con la risposta di uno dei due a me con l'altro in copia, ancora da leggere, e una persona solo in copia che non deve comparire nella lista.
- **Dati sintetici più simili a email vere.** Oggi sembrano quasi una chat: su 379 messaggi il testo è lungo in mediana 120 caratteri, il 90% sta sotto i 300 e il più lungo arriva a 1.028; nessuno supera i 1.500. Le citazioni ci sono in 110 messaggi, ma le citazioni di citazioni solo in 6, e le firme con `-- ` in 36. Vanno rifatti perché somiglino alla posta vera:
  - **testi molto più lunghi**, come spesso sono le email: riepiloghi, verbali, istruzioni, richieste con più punti, da 2.000 fino a 10.000 caratteri e oltre;
  - **citazioni di citazioni**, su tre, quattro, cinque livelli, con la risposta sopra e anche sotto o in mezzo alla citazione, interrotte da righe bianche, con `>` attaccato o staccato, con intestazioni di tutte le forme («Il giorno … ha scritto:», «On … wrote:», «-----Messaggio originale-----», «Da: … Inviato: … A: … Oggetto: …» di Outlook), anche spezzate su più righe, e citazioni senza `>` sotto un'intestazione di Outlook;
  - **firme di tutte le forme e dimensioni**: solo il nome, nome e telefono, firme aziendali di dieci righe con indirizzo, P.IVA e avviso legale sulla riservatezza in italiano e in inglese, «Inviato da iPhone», firme con e senza `-- `, firme ripetute dentro ogni livello di citazione;
  - in un thread lungo, ogni risposta si porta dietro tutto il thread citato sotto, come succede davvero.

  I dati sintetici sono comuni a tutte le versioni: si rigenerano con `generate-synthetic.mjs` e i casi in `cases/`, con nomi e testi sempre inventati. Servono anche per provare «Leggi tutto», «Mostra testo citato», l'altezza dei messaggi e il punto qui sotto.
- **Via i riquadri intorno all'argomento attivo.** Il bordo colorato intorno all'argomento attivo (con ↑ ↓, o dopo «letto» e «Archivia») non piace, e con le email lunghe esce dallo schermo, quindi spesso non si vede. Al suo posto il filo colorato a sinistra dell'argomento attivo diventa più spesso, e il suo titolo si evidenzia nell'indice a destra. Il filo corre lungo tutto l'argomento, quindi si vede anche quando l'argomento è più alto della finestra. Da provare con i dati sintetici nuovi del punto qui sopra, con più messaggi lunghi di fila nello stesso argomento.

## Com'è andata nella v11

Fatto tutto quello che c'è sopra, provato con i dati sintetici nuovi, servito in locale e aperto dal disco, anche a 390 px di larghezza, senza errori in console.

**Come è fatto**

- **Thread intero.** Il modello prima raccoglie i messaggi, poi raggruppa i thread, poi dà ogni messaggio del thread a tutti i membri: chi ci ha scritto e chi ha ricevuto un mio messaggio, in A: o in Cc: (nella v10 solo A:, e Cc: solo se A: era vuoto). Le notifiche dei servizi restano solo in «Servizi e notifiche».
- **Messaggi degli altri.** Nella conversazione di Anna, un messaggio di Marco ha le iniziali di Marco nei suoi colori e il nome; il nome porta alla conversazione di Marco, sullo stesso messaggio. Nella lista persone l'anteprima dice chi ha scritto l'ultimo messaggio quando non è la persona («Roberto Galli: …»).
- **Argomento attivo.** Il filo passa da 2 a 6 px per tutta l'altezza dell'argomento; nell'indice a destra il titolo ha lo sfondo e la striscia di colore più larga.
- **Dati sintetici.** 396 messaggi (erano 379): testo lungo in mediana 213 caratteri (era 120), il 10% sopra i 1.400, il più lungo 9.300 (verbale di assemblea, passaggio di consegne, relazione tecnica, itinerario). Ogni risposta cita tutto il thread sotto, nel formato del programma di posta di chi scrive: Gmail in italiano e in inglese (con l'intestazione spezzata su due righe), Outlook in italiano con la riga di trattini bassi e in inglese con «-----Original Message-----», Apple Mail, iPhone con «Inviato da iPhone», Thunderbird con le righe vuote che interrompono la citazione. Firme di ogni forma: solo il nome, nome e cellulare, firme aziendali fino a dieci righe con P.IVA e avvisi di riservatezza in italiano e in inglese, con e senza `-- `. I testi lunghi stanno in `mockups/data/testi-lunghi.mjs`.
- **Casi nuovi** (nel generatore, non in `cases/`): «Infiltrazioni dal terrazzo» tra me, la geometra Chiara Benedetti e l'impresa di Roberto Galli, con Federica Monti solo in copia (non compare nella lista), una risposta scritta dentro la citazione e l'ultimo messaggio di Galli da leggere; «Passaggio di consegne progetto Orione» con quattro persone e citazioni fino a quattro livelli.

**Oltre le note**

- **Riconoscere citazioni e firme.** Con i dati nuovi servivano regole in più, ora nel prototipo: la riga di trattini bassi di Outlook, «Inviato da iPhone» e simili, gli avvisi di riservatezza, e la firma aziendale senza `-- ` (l'ultimo blocco di righe brevi con telefono, indirizzo, sito o partita IVA, anche sopra l'avviso di riservatezza). Su tutti i 396 messaggi nessun testo principale contiene più intestazioni, `>` o «Inviato da iPhone».
- **Risposta dentro la citazione.** Prima i `>` restavano a vista e «Leggi tutto» tagliava prima delle risposte. Ora le righe citate sono in piccolo, rientrate con una riga a sinistra e senza `>`, l'intestazione «… ha scritto:» in grigio, e le risposte in chiaro; niente taglio.
- **Autori giusti.** Nella ricerca, negli allegati e nell'archivio un messaggio di Marco trovato nella conversazione di Anna risultava di Anna: ora è di chi l'ha scritto. «Aspetti una risposta» conta una volta un thread con più persone; nella ricerca «scambio con Anna» vale solo per i messaggi di Anna e i miei.

**Da sapere**

- Gli allegati sintetici sono stati rinumerati dalla rigenerazione (`attachments/042…100`): i percorsi in `synthetic.js` sono aggiornati e valgono per tutte le versioni.
- Le date nelle intestazioni delle citazioni sono quelle del generatore, prima dello spostamento a oggi: possono non coincidere con la data mostrata sul messaggio (succedeva già prima).
- Una mia risposta a tutti in una email con 26 destinatari porta il thread nelle conversazioni dei primi 8 (come nella v10, che li metteva già nella lista).

**Restano aperti**

- Quelli della v10: canvas da pubblicare (anche della v11), «eliminato altrove», servizi, allegati delle bozze al ricaricamento, nome breve dell'app.

