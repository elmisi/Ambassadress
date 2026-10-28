# L'app: Ambassadress in sola lettura

Secondo passo del piano in `docs/implementazione.md`: l'interfaccia della v13 in una finestra vera, sopra il motore (`../motore`). **Legge soltanto**: niente cambia sul server. «Letto», «Archivia» e le bozze restano su questo computer finché non arriva la parte di scrittura; «Invia» lo dice e tiene la bozza.

## Come si lancia

```bash
cd app
cargo run                 # la prima volta compila Tauri: qualche minuto
cargo build --release     # target/release/ambassadress
cargo tauri build         # il pacchetto .deb (target/release/bundle/deb/)
```

Per l'icona nella barra (su Wayland serve un file `.desktop` installato): `cargo run --release -- --installa` scrive, senza sudo, `~/.local/share/applications/it.elmisi.a10s.desktop` (nome «a10s») e le icone in `~/.local/share/icons/hicolor/`; `--disinstalla` le toglie. La finestra si presenta come `ambassadress` (il nome dell'eseguibile) e `StartupWMClass` la lega al file. Col pacchetto `.deb` non serve.

Servono le librerie di WebKitGTK: `sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev`.

Al primo avvio chiede indirizzo e password (per Gmail la password per app). Prova l'accesso, salva la casella e comincia a scaricare: l'etichetta accanto al nome dice a che punto è.

## Dove tiene le cose

| cosa | dove |
| --- | --- |
| casella e password | `~/.local/share/a10s/casella.json` (cartella 0700, file 0600; la password è in chiaro nel file, il portachiavi del sistema arriva dopo) |
| database della posta | `~/.local/share/a10s/posta.sqlite` |
| allegati scaricati | `~/.local/share/a10s/allegati/`, uno per contenuto |
| allegati aperti con un altro programma | `~/.cache/a10s/aperti/` |
| scelte fatte nell'interfaccia (letto, archivio, bozze) | il `localStorage` della finestra, in `~/.local/share/it.elmisi.a10s/` |

`A10S_DATI=cartella` sposta i dati altrove (per le prove). «Togli» nel pannello delle caselle cancella casella, database e allegati da questo computer; sul server non cambia niente.

## Com'è fatta

- `src/main.rs`: la finestra, i comandi per l'interfaccia e il protocollo `a10s://localhost/allegato/<messaggio>/<parte>`, che serve gli allegati (il motore li scarica la prima volta).
- `src/motore_app.rs`: accesso di prova, sincronizzazione in background con l'ascolto della posta nuova, dati per l'interfaccia, allegati.
- `src/casella.rs`: la casella salvata. `src/sistema.rs`: xdg-open, notify-send, cartelle.
- `ui/index.html`: la v13 (`mockups/v13/project/Main.dc.html`) con le modifiche per l'app; `ui/support.js`: il runtime dei mockup, che prende i dati dal ponte; `ui/ponte.js`: il ponte verso Rust; `ui/fonti/`: il carattere, dentro l'app.

**Comandi** (da `ponte.js`): `avvio`, `dati`, `configura`, `cambia_password`, `togli_casella`, `apri_allegato`, `salva_allegato`, `aggiorna` (clic su «aggiornata alle …»), `programma_per` (il nome del programma per «Apri con …»), `salva_testo` («Scarica .json» in «Segnala»), `salva_messaggio` («Salva» nel menu «⋯»: il messaggio intero come `.eml` in Scaricati), `priorita` (gli allegati sullo schermo in Allegati), `stato_allegati` (la riga del pannello delle caselle), `apri_link`, `notifica`, `diario`.
**La finestra** (giro 1):
- mostra solo le pagine dell'app e il protocollo `a10s`: un link http, https o mailto va al programma del sistema, il resto si ferma (anche le nuove finestre);
- alla × si nasconde subito, poi si ferma il motore (meno di mezzo secondo);
- il processo di WebKit che disegna la pagina si ferma prima che WebKitGTK 2.52 cada chiudendolo (`free(): corrupted unsorted chunks`, con il rapporto di 200 MB di apport e il terminale appeso). Quello di rete, che salva lo stato dell'interfaccia, chiude da solo.
**Interfaccia**: `ui/support.js` confronta la pagina nuova con quella che c'è e cambia solo le differenze (gli elementi con un `id` si ritrovano anche se cambiano posto); mentre scrivi la risposta la bozza si aggiorna senza ridisegnare, il resto mezzo secondo dopo. `window.__dcTempi` dice quanto è costato l'ultimo disegno.
**Allegati in background**: un thread con una connessione sua scarica tutti gli allegati, prima quelli sullo schermo in Allegati, poi dal più recente indietro; si ferma con la quota di 1 GB al giorno, sulla connessione a consumo e sotto i 5 GB liberi. Il motore ne ricava le misure delle immagini e la miniatura e le pagine dei PDF (`pdftoppm`, `pdfinfo`); la miniatura si serve da `a10s://localhost/miniatura/<messaggio>/<parte>`.
**Allegati**: le richieste del protocollo `a10s` passano da una coda con due lavoratori; un errore chiude la connessione delle richieste solo se è un problema della connessione.
**Sincronizzazione**: il `Servizio` del motore (sincronizza, poi resta in IDLE e si ricollega da solo) con una finestra dei 3.000 messaggi più recenti, testi compresi; le sue notizie diventano gli eventi qui sotto. Mentre scarica, l'interfaccia ricarica i dati al massimo ogni tre secondi; l'etichetta «Scarico la posta» compare solo sopra i 50 messaggi da scaricare.
**Eventi** verso l'interfaccia: `rete` (`{stato: ok|sync|offline|auth, fatti, totale, alle}`), `aggiornato` (dati nuovi nel database: l'interfaccia ricostruisce il modello), `primo-avvio` (`{tappa: server|password|posta}`).

## Provarla senza la posta vera

- **Nel browser**, con i dati sintetici: `python3 -m http.server -d . 8799` dalla cartella del progetto, poi `http://127.0.0.1:8799/app/ui/index.html`. `ponte.js` fa da motore finto: `?primo` mostra il primo avvio (una password con «sbagliata» viene rifiutata), `?rete=offline|auth|sync` uno stato della rete.
- **Nell'app, col finto server IMAP** del motore, che serve i dati sintetici come email vere (vedi `motore/README.md`):
  ```bash
  node tools/sintetici-in-eml.mjs                                   # una volta, dalla cartella del progetto
  cd motore && cargo run --example finto_server -- --porta 1143 [--arrivi 25] [--cadute 25 --pausa 12] [--copie 8]
  cd app && A10S_SERVER=127.0.0.1:1143 A10S_DATI=/tmp/prova cargo run
  ```
  Al primo avvio: `alex.moro@esempio.it`, password `finta-password`. `A10S_SERVER` vale solo per le prove: collega in chiaro a quell'indirizzo, qualunque server si scriva, e solo se è 127.0.0.1 o localhost.
- **Chiusura nelle prove**: `A10S_PROVA_CHIUDI=secondi` chiude la finestra come la ×, per misurare la chiusura.
- **Prove automatiche nella finestra vera**: `A10S_PROVA_JS=script.js` esegue lo script nella pagina dopo tre secondi; lo script scrive i risultati con `A10S.invoca('diario', {riga})`, che finiscono sul terminale (`[interfaccia] …`), come gli errori della pagina e le violazioni della CSP. Con Xvfb (`Xvfb :99`, poi `DISPLAY=:99 GDK_BACKEND=x11`) e `ffmpeg -f x11grab -i :99 -frames:v 1 foto.png` si fotografa la finestra.
- **Nelle prove, mai il desktop vero**: «Apri», «Scarica» e le notifiche chiamano `xdg-open`, `xdg-user-dir` e `notify-send`. Mettere in testa al `PATH` tre script finti che scrivono in un registro, e `XDG_DATA_HOME`/`XDG_CACHE_HOME` in una cartella di prova: altrimenti un allegato sintetico finisce nella cartella Scaricati vera e la notifica sul desktop.

## Com'è andata la prima prova (28 ottobre 2026)

Tutto col finto server e i dati sintetici, nella finestra vera (WebKitGTK su Xvfb):

- primo avvio: password sbagliata rifiutata con il messaggio per Gmail, poi quella giusta; 400 messaggi in un secondo; il nome «Alex Moro» preso dai messaggi inviati;
- 3.240 messaggi sul server: ne tiene 3.000 (la finestra), in circa quattro secondi;
- rete che cade ogni 25 secondi: «Offline, aggiornata alle …», poi torna da sola;
- password rifiutata: «La password non funziona più», il pannello delle caselle si apre sul campo; una password sbagliata lo dice, quella giusta fa ripartire la posta;
- allegati dal protocollo `a10s`: PDF, immagini e calendari con il tipo giusto in pochi millisecondi; **i PDF si vedono dentro l'app** (WebKitGTK ha un visore incorporato), il testo si legge nel visore, «Apri» passa la copia a `xdg-open`, «Scarica» la mette in Scaricati;
- newsletter: immagini esterne bloccate, con «Mostra immagini» caricate davvero; il pixel di tracciamento tolto sempre;
- notifiche solo per i messaggi nuovi delle persone, non letti, arrivati nell'ultima ora (non per servizi e newsletter, non durante la prima sincronizzazione);
- nessun errore nella pagina, nessuna violazione della CSP; `cargo clippy -- -D warnings` pulito.

## Cosa non c'è ancora

- La scrittura: «letto», «archivia», «chiuso» (su Gmail andrà in un'etichetta) restano locali; l'invio non c'è.
- Una casella sola («Aggiungi una casella» è nascosto).
- Il portachiavi del sistema per la password.
- Lo storico oltre i 3.000 messaggi più recenti (le intestazioni di tutta la casella e i testi a strati, come deciso).
- Mai provata sulla casella vera: il primo giro su Gmail lo fai tu (vedi sotto, «Il primo avvio sulla tua casella»).

## Il primo avvio sulla tua casella

`cd app && cargo run --release`, poi indirizzo Gmail e password per app (la stessa di `~/.secrets/gmail-app-password.txt` va bene: l'app non legge quel file, va scritta nel campo). La prima volta scarica i 3.000 messaggi più recenti con i testi: dalle misure del motore, qualche minuto. Il server non cambia: la guardia del motore ferma ogni comando che scrive.
- L'interfaccia ridisegna tutto a ogni clic, come il runtime dei mockup: con 2.000 messaggi sintetici un ridisegno costa circa 120 ms, con 10.000 circa 250 ms (Chromium). Il disegno solo delle righe sullo schermo è il passo dopo.
