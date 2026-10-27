# Mockups

Una cartella per versione. Ogni versione si apre in locale e ha un canvas Claude Design (privato) per commenti e modifiche. Le note di ogni giro e le decisioni prese sono in `../docs/mockup_vN.md` (dalla v10) e in `../CLAUDE.md`.

| Versione | Locale | Canvas |
| --- | --- | --- |
| v1 · Prime direzioni | `v1/project/Main.dc.html` | https://claude.ai/artifact/TqxUb7dzquT1a3tSXTTtMK |
| v2 · Persone | `v2/project/Main.dc.html` | https://claude.ai/artifact/LNLHz8LatH5VPwWHke8wi8 |
| v3 · Persone, dati da JSON, segnalazioni | `v3/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/VpxpyxPBdUB7J5W7gByoxH |
| v4 · Riunioni, servizi, destinatari, allegati | `v4/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/BdLjVYbbVfqBcUFk1xAAnu |
| v5 · Da leggere, archivio, link | `v5/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/EcmNsbQ4JVhSMobTKRNziT |
| v6 · Più ordine | `v6/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/1PoEkJVN5a2bV82o7igrhX |
| v7 · Il filo | `v7/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/74czDhLRudmykHNgMKKrb9 |
| v8 · Per argomento | `v8/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/YAWa692USuocPWrJ32r6FX |
| v9 · Argomenti che si chiudono | `v9/project/Main.dc.html` (`?data=real` per i dati veri) | https://claude.ai/artifact/MQ5mJ5gZXq5hJs85gHfHmC |
| v10 · Stati chiari | `v10/project/Main.dc.html` (`?data=real` per i dati veri) | ancora da pubblicare |
| v11 · Thread interi | `v11/project/Main.dc.html` (`?data=real` per i dati veri) | ancora da pubblicare |
| v12 · Segnalazioni e tagli | `v12/project/Main.dc.html` (`?data=real` per i dati veri) | ancora da pubblicare |

L’icona ha un canvas a parte: `icona/project/Main.dc.html`, https://claude.ai/artifact/YKJjSseVQJ6eXcsouBtUa1. La scelta è H2 con il contorno L3; i file sono in `icona/`: `icona.svg` (fondo inchiostro), `icona-chiara.svg` (fondo carta), `icona-senza-fondo.svg` e `sigillo.svg` (solo il sigillo, per favicon e barra). Dalla v12 la favicon e il logo accanto al nome sono L3, cioè `icona-senza-fondo.svg` ritagliata intorno alla fascia, nel mockup e in `index.html`.

Dalla v5 lo stato «letto», l'archivio e il cestino restano nel `localStorage` del browser (dalla v10 anche argomenti chiusi, bozze, preferenze su «Leggi tutto», messaggi inviati e simulati), separati per dati sintetici e veri. Dalla v6 la chiave è comune alle versioni (`ambassadress:<fonte>`), e la v6 riprende lo stato salvato dalla v5. Dalla v7 si azzera dal pulsante «Dati sintetici» / «Dati veri» in alto, con «Annulla» (dalla v8 «Torna allo stato iniziale», che riporta ai flag di lettura di Gmail); nelle versioni precedenti dalla console del browser con `localStorage.removeItem('ambassadress:synthetic')` (o `ambassadress:real`).

## Segnalazioni

Dalla v12 «Segnala» salva la segnalazione in `../segnalazioni/segnalazioni.sqlite` (fuori da git) se è acceso il server locale:

```sh
python3 tools/segnalazioni.py            # dalla radice del repository, porta 8797
python3 tools/segnalazioni.py elenco     # le segnalazioni aperte
```

Se il server è spento restano «Copia il report» e «Scarica .json». Come ci si lavora: `../CLAUDE.md`.

## Aprire in locale

Aprire `index.html` nel browser, oppure:

```sh
python3 -m http.server -d mockups 8000   # poi http://localhost:8000
```

## Come è fatto

- `vN/project/canvas.json` e `vN/project/*.dc.html` sono i file del canvas: si pubblicano sul canvas con root `vN/`.
- `support.js` è un runtime minimo che fa girare i `.dc.html` fuori dal canvas. In ogni `vN/project/` c'è un symlink `support.js -> ../../support.js`. Il canvas usa il suo runtime: `support.js` non va pubblicato.
- Dalla v3 i dati arrivano da `data/` (symlink `vN/project/data -> ../../data`): vedi `data/README.md`. Sul canvas si pubblica solo `data/synthetic.js`.
