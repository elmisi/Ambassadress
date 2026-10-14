# Ambrassadress
Software per la gestione della posta elettronica come nessuno lo ha mai fatto

## Motivazione
Sono decenni che nessuno innova la posta elettronica da un punto di vista UX e UI, ed ora che abbiamo AI possiamo innovarlo anche nelle funzionalità. Voglio vedere qualcosa di nuovo che nessuno ha mai fatto, ragionare completamente da zero l'utilizzo della posta elettronica.

## Retrospettiva
Negli anni ho usato molti programmi di posta elettronica:

- Eudora
- Thunderbird
- Opera Mail
- The Bat!
- Gmail
- Proton Mail

in qualche modo tutti avevano qualcosa di buono, ma l'ultimo che si è imposto che sembrava una grande innovazione è fermo da anni, gestisce i thread in modo pessimo e confuso, moltiplica le bozze in modo convulso e non si trova mai niente: GMail
Grandi promesse di innovare le email e siamo ancora fermi al palo, stesse idee di 30 anni fa, voglio un client nuovo

## Cosa mi aspetto
Ecco la lista, non per priorità:
- vedere i thread email gestiti bene
- gestione degli allegati magistrale
- trovare sempre quello che cerco (es. email con la convocazione all'assemblea del 12 ottobre con allegata la delega)
- veloce come il fulmine
- web possibilmente, ma se le performance sono un problema anche applicazione standalone va bene, ma leggera (e modulare)
- gestione IMAP perfetta
- importare rubrica, email da altri client, integrata (ma tutto fatto con moduli che vengono caricati solo quando servono e poi spariscono
- una UX/UI sbalorditiva ma semplice, veloce intuitiva (ci lavorerò su questo appena vedo le proposte capisco in che direzione 
- che usi l'AI non è un requisito, ma se per impelemetare funzionalità innovative serve, allora deve essere un modello possibilmente che gira in locale, piccolo ma funzionale

## Direzione
Come arriviamo al nuovo mail client del millennio ? ovviamente per tentativi, prima in ampiezza provando diverse soluzioni, interfacce e funzioni e poi validando di volta in volta la strada migliore selezionando il nostro client ideale.
Come idee di base le cose che mi piacciono sono:
- zero (o quasi) dipendenze
- modularità (ogn funzione usata poco o raramente è un modulo esterno caricato e scaricato all'occorrenza)
- validazione (le cose si provano per un po', si raccolgono feedback, si passa alla funzione successiva)
- poche configurazioni: il software deve fare le cose con una filosofia che funziona, i settings servono solo per dettgli di gusto personale ma il software ideale per me ha zero settings di configurazione, parte ed è già utilizzabile

## Design
Useremo claude design per sviluppare il mockup navigabile dell'interfaccia e lavorare di iterazioni con design fino ad avere un sistema perfettamente funzionate, e di conseguenza aver chiarito tutti gli aspetti cruciali dell'usabilità (cioè cosa intendo per "magistrale" o "UX/UI sbalorditiva) cosa fondamentale è che il design tenda allo sforzo definito in "Direzione" che raggiunga quello definito il "Cosa mi aspetto" ma ancora di più che strada facendo si chiariranno sempre di più i requisiti o ne nasceranno di nuovi