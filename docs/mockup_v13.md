# Mockup v13

Note per la v13, l'ultimo giro di mockup: le schermate e le regole che il prototipo non aveva e che servono all'app vera (decisioni in `docs/implementazione.md`, «Regole decise»). Fatto in parallelo al banco di prova del motore.

- **Primo avvio.** Oggi nella v12 l'app parte già piena di posta. Nell'app vera la prima volta c'è una schermata sola: il sigillo, il nome, una riga, «Indirizzo email», «Password per app» (con il link a come si crea, per Gmail) e «Collegati». Il server si trova da solo: Gmail è noto, per gli altri si cerca nell'elenco pubblico dei provider; solo se non si trova compaiono due campi, server IMAP e server SMTP. Errori detti in chiaro, con cosa fare («La password non va: per Gmail serve una password per app, non quella dell'account»). Dopo il collegamento si entra subito nella vista normale: le email recenti ci sono già, il resto arriva dietro (vedi stato della rete). Nel mockup si vede dal pannello «Prototipo».
- **Stato della rete.** Quando va tutto bene non si vede niente. Accanto al nome compare una piccola etichetta solo quando serve: «Scarico la posta, 1.240 di 18.400» durante la prima sincronizzazione; «Offline, aggiornata alle 10:42»; «La password non funziona più», che porta al pannello delle caselle. Il rosso ceralacca resta per «da leggere». Nel mockup gli stati si simulano dal pannello «Prototipo».
- **Le tue caselle (le impostazioni).** Niente pagina delle impostazioni. Un pulsante con le tue iniziali in alto a destra apre un pannello come quello «Prototipo»: le caselle collegate (indirizzo, tipo, ultimo aggiornamento, «Cambia password», «Togli»), «Aggiungi una casella» (riapre la schermata del primo avvio), il tuo nome e la firma. Nient'altro.
- **Più caselle in una lista sola.** Nei dati sintetici una seconda casella (IMAP non Gmail) con qualche thread. Le persone sono una lista sola; nel riquadro della risposta, solo quando le caselle sono più di una, «Da: …» con l'indirizzo a cui ti hanno scritto.
- **Tema scuro.** Segue il sistema. Stessa carta, inchiostro e ceralacca, al buio: fondo inchiostro scuro, testo carta, il filo e i colori delle persone smorzati. Tutti i colori passano dalle variabili, compresi quelli scritti a mano nelle viste vecchie (Cerca, Allegati, Segnala). Nel pannello «Prototipo» si sceglie chiaro, scuro o come il sistema, per provarlo.
- **Newsletter in HTML.** Si mostrano come sono, in un riquadro isolato, con le immagini esterne bloccate: «2 immagini bloccate · Mostra immagini», che vale da lì in poi per quel mittente. Nei dati sintetici le newsletter hanno immagini esterne e un pixel che traccia.
- **Etichette come filtri della ricerca.** Le etichette di Gmail dell'utente diventano filtri in Cerca («etichetta: condominio», o la scelta tra i filtri). Lo spam non si vede da nessuna parte. Nei dati sintetici qualche etichetta.
- **Notifiche del desktop.** Solo dai messaggi delle persone (non dai servizi, non dai thread in cui sei solo in copia degli altri). Nel mockup, quando si simula una email in arrivo, compare in alto a destra una notifica come quella del sistema, con il nome breve «a10s».
- **Eliminato altrove.** Un messaggio cancellato da un altro dispositivo sparisce in silenzio; se era l'ultimo dell'argomento, l'argomento va via. Si simula dal pannello «Prototipo».

## Com'è andata nella v13

**Come è fatta.** È la v12 con le note qui sopra. Tutte le scorciatoie per provarle sono nel pannello «Dati sintetici»: Rete (in linea, prima sincronizzazione, offline, password scaduta), Tema (come il sistema, chiaro, scuro), «Mostra il primo avvio» e, nella simulazione sull'argomento attivo, «L'ultimo messaggio eliminato altrove».

- **Primo avvio.**
  - Una schermata sola, sul fondo carta: il sigillo L3, il nome, una riga, i campi «Indirizzo email» e password, e «Collegati».
  - Il server si riconosce mentre scrivi l'indirizzo: Gmail, Outlook.com, iCloud, Yahoo, Libero, TIM, Tiscali, Fastmail e Proton. Nel mockup i domini «esempio» fanno la parte di quelli trovati nell'elenco pubblico; per tutti gli altri compaiono i campi server IMAP e SMTP.
  - Per Gmail il campo si chiama «Password per app», con la spiegazione e il link per crearla. Se la password non è di 16 lettere, l'errore dice che quella con cui entri in Gmail qui non funziona.
  - Per Proton l'errore spiega che serve il Bridge.
  - Dopo «Collegati» scorrono tre passi (trovo il server, controllo la password, scarico le email recenti), poi si entra nella vista normale con l'etichetta «Scarico la posta».
- **Stato della rete.**
  - È un'etichetta accanto al nome: «Scarico la posta, 2.412 di 18.400» con una barra sottile, «Offline, aggiornata alle 10:42» con il bordo tratteggiato, oppure «La password non funziona più» nel giallo degli avvisi.
  - Un clic su offline spiega cosa si può fare; un clic su password porta al pannello delle caselle, con il campo della password già aperto. Salvata la password, l'etichetta sparisce.
  - Quando va tutto bene non si vede niente.
- **Le tue caselle.**
  - Si apre dal pulsante con le iniziali, in alto a destra.
  - Ogni casella ha indirizzo, tipo, ultimo aggiornamento, «Cambia password» e «Togli», con conferma; «Togli» manca se la casella è una sola.
  - Sotto ci sono «Aggiungi una casella», che riapre la schermata del primo avvio, poi «Il tuo nome» e «Firma».
  - Il nome dà le iniziali del pulsante.
- **Più caselle.** Nei dati c'è una seconda casella IMAP, `alex@studiomoro.esempio.it`, con due thread. Le persone restano una lista sola. Nella risposta, solo con più caselle, compare «Da: …» con l'indirizzo a cui ti hanno scritto.
- **Tema scuro.**
  - Tutti i colori passano da variabili. Quelli scritti a mano erano 148 (Cerca, Allegati, Segnala, servizi, stati), più le tinte delle persone e dei tipi di file.
  - Al buio il fondo è l'inchiostro dell'icona (#141927), il testo è carta e la ceralacca è più chiara.
  - Le tinte delle persone si invertono: il colore scuro diventa il fondo, quello chiaro il testo. Così le iniziali restano leggibili senza ricostruire niente.
  - Segue il sistema; dal pannello lo si forza chiaro o scuro, e la scelta resta.
- **Newsletter.**
  - Si vedono in un riquadro isolato: niente script, niente richieste esterne (Content-Security-Policy), il riquadro si adatta all'altezza.
  - Al posto delle immagini esterne ci sono riquadri grigi, e una riga dice «2 immagini esterne bloccate, più un pixel che avvisa il mittente quando apri l'email · Mostra immagini». La scelta vale per il mittente, con «Annulla» e «Blocca di nuovo».
  - **Il pixel che traccia resta bloccato anche con le immagini mostrate.** Le note non lo chiedevano, ma è la scelta che rispetta l'intenzione.
  - Nel mockup le immagini «mostrate» sono riquadri colorati con il testo alternativo, perché nei dati sintetici non esistono.
- **Etichette.** Le etichette dell'utente diventano filtri in Cerca: «etichetta condominio», oppure basta il nome («casa»). Nella pagina vuota di Cerca c'è la riga «Le tue etichette». Lo spam non compare in nessuna vista né nella ricerca, e non conta nei «da leggere».
- **Notifica del desktop.** Quando si simula «Arriva una email da …» compare in alto a destra una notifica come quella del sistema: icona, «a10s», «ora», mittente, argomento e prima riga. Va via da sola dopo 7 secondi.
- **Eliminato altrove.** Il messaggio più recente dell'argomento attivo sparisce. Se era l'unico, l'argomento va via. Resta anche dopo il ricaricamento; dall'avviso si annulla, e «Torna allo stato iniziale» lo rimette.

**Bug trovati e corretti**

- Il campo email `type="email"` scriveva al contrario. Il runtime ridisegna a ogni tasto e rimette il cursore con `setSelectionRange`, che i campi email non permettono. Ora è `type="text"` con `inputmode="email"`.
- La classe `.warn` già esistente dava uno sfondo giallo alla riga della casella con la password scaduta: rinominata.

**Da sapere**

- «Togli» una casella nel mockup la toglie dal pannello, ma i suoi messaggi restano visibili; lo dice l'avviso.
- La firma del pannello non entra ancora nelle risposte del mockup.
- Nei dati sintetici gli orari di oggi sono spostati più avanti dell'ora attuale. Per questo il messaggio simulato «appena arrivato» può non risultare il più recente, ed «eliminato altrove» toglie quello che risulta più recente nel modello.
- Per l'app vera restano da disegnare nel motore il portachiavi del sistema per le password e la ricerca del server, cioè l'elenco pubblico dei provider e i record DNS (RFC 6186).

**Restano aperti**

- Canvas della v10–v13 da pubblicare.
- Le regole della mappa degli stati per i servizi.
- Gli allegati delle bozze al ricaricamento: nell'app vera si risolve da sé.
