#!/usr/bin/env python3
"""Segnalazioni del prototipo, in un file SQLite.

Acceso a mano mentre si prova il prototipo, riceve le segnalazioni del pulsante «Segnala»
e le salva in segnalazioni/segnalazioni.sqlite (fuori da git). Lo stesso script serve a
elencarle, leggerle e cambiarne lo stato.

    python3 tools/segnalazioni.py                 accende il server su 127.0.0.1:8797
    python3 tools/segnalazioni.py elenco          le segnalazioni aperte (--tutte per tutte)
    python3 tools/segnalazioni.py mostra 12       una segnalazione, con il report intero
    python3 tools/segnalazioni.py stato 12 risolta --versione v13 --nota "..."

Stati: nuova, in-lavorazione, risolta, non-riproducibile, scartata.
Con i dati veri il contenuto delle email arriva già sostituito dal prototipo.
Solo libreria standard.
"""

import argparse
import json
import sqlite3
import sys
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DB = ROOT / "segnalazioni" / "segnalazioni.sqlite"
PORT = 8797
MAX_BYTES = 8 * 1024 * 1024
STATI = ["nuova", "in-lavorazione", "risolta", "non-riproducibile", "scartata"]
APERTE = ("nuova", "in-lavorazione")

SCHEMA = """
create table if not exists segnalazioni (
  id integer primary key,
  creata text not null,
  versione text,
  fonte text,
  anonima integer,
  tipo text,
  cosa text,
  atteso text,
  report text not null,
  stato text not null default 'nuova',
  risolta_in text,
  nota text,
  aggiornata text
)
"""


def adesso():
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def apri():
    DB.parent.mkdir(exist_ok=True)
    conn = sqlite3.connect(DB)
    conn.row_factory = sqlite3.Row
    conn.execute(SCHEMA)
    return conn


def salva(report):
    data = report.get("data") or {}
    with apri() as conn:
        cur = conn.execute(
            "insert into segnalazioni (creata, versione, fonte, anonima, tipo, cosa, atteso, report, aggiornata)"
            " values (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                adesso(),
                report.get("prototype"),
                data.get("source"),
                1 if data.get("redacted") else 0,
                report.get("kind"),
                report.get("what"),
                report.get("expected"),
                json.dumps(report, ensure_ascii=False),
                adesso(),
            ),
        )
        return cur.lastrowid


class Gestore(BaseHTTPRequestHandler):
    server_version = "segnalazioni"

    def cors(self):
        # il prototipo si apre da file:// (origine «null») o dal canvas
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Allow-Private-Network", "true")

    def rispondi(self, codice, corpo):
        dati = json.dumps(corpo, ensure_ascii=False).encode("utf-8")
        self.send_response(codice)
        self.cors()
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(dati)))
        self.end_headers()
        self.wfile.write(dati)

    def do_OPTIONS(self):
        self.send_response(204)
        self.cors()
        self.end_headers()

    def do_GET(self):
        if self.path.rstrip("/") != "/segnalazioni":
            return self.rispondi(404, {"errore": "non trovato"})
        with apri() as conn:
            n = conn.execute("select count(*) from segnalazioni").fetchone()[0]
        self.rispondi(200, {"ok": True, "segnalazioni": n})

    def do_POST(self):
        if self.path.rstrip("/") != "/segnalazioni":
            return self.rispondi(404, {"errore": "non trovato"})
        lunghezza = int(self.headers.get("Content-Length") or 0)
        if lunghezza <= 0 or lunghezza > MAX_BYTES:
            return self.rispondi(413, {"errore": "report vuoto o troppo grande"})
        try:
            report = json.loads(self.rfile.read(lunghezza).decode("utf-8"))
        except (ValueError, UnicodeDecodeError):
            return self.rispondi(400, {"errore": "il report non è JSON"})
        if not isinstance(report, dict) or report.get("report") != 1:
            return self.rispondi(400, {"errore": "non è un report del prototipo"})
        n = salva(report)
        print(f"Segnalazione {n} salvata ({report.get('prototype')}, {report.get('kind')})", flush=True)
        self.rispondi(201, {"id": n})

    def log_message(self, *args):
        pass


def server(porta):
    apri().close()
    httpd = ThreadingHTTPServer(("127.0.0.1", porta), Gestore)
    print(f"Segnalazioni su http://127.0.0.1:{porta}, file {DB.relative_to(ROOT)}. Ctrl+C per fermare.", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print()


def riga(r):
    cosa = " ".join((r["cosa"] or "").split())
    if len(cosa) > 70:
        cosa = cosa[:69] + "…"
    stato = r["stato"] + (f" in {r['risolta_in']}" if r["risolta_in"] else "")
    return f"{r['id']:>4}  {r['creata'][:16].replace('T', ' ')}  {r['versione'] or '?':<4} {r['fonte'] or '?':<9} {r['tipo'] or '':<15} {stato:<20} {cosa}"


def elenco(tutte):
    with apri() as conn:
        q = "select * from segnalazioni" + ("" if tutte else " where stato in (?, ?)") + " order by id"
        righe = conn.execute(q, () if tutte else APERTE).fetchall()
    if not righe:
        print("Nessuna segnalazione" + ("." if tutte else " aperta."))
        return
    for r in righe:
        print(riga(r))


def mostra(n, solo_json):
    with apri() as conn:
        r = conn.execute("select * from segnalazioni where id = ?", (n,)).fetchone()
    if not r:
        sys.exit(f"Non c'è la segnalazione {n}.")
    report = json.loads(r["report"])
    if solo_json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return
    print(riga(r))
    for k, etichetta in (("cosa", "Cosa è successo"), ("atteso", "Cosa si aspettava"), ("nota", "Nota")):
        if r[k]:
            print(f"\n{etichetta}:\n{r[k]}")
    print(f"\nDati: {r['fonte']}" + (", contenuto sostituito" if r["anonima"] else ", in chiaro") + f". Aggiornata {r['aggiornata']}.")
    print("\nReport:")
    print(json.dumps(report, ensure_ascii=False, indent=2))


def cambia(n, stato, versione, nota):
    if stato == "risolta" and not versione:
        sys.exit("Per «risolta» serve --versione (per esempio v13).")
    with apri() as conn:
        cur = conn.execute(
            "update segnalazioni set stato = ?, risolta_in = ?, nota = coalesce(?, nota), aggiornata = ? where id = ?",
            (stato, versione if stato == "risolta" else None, nota, adesso(), n),
        )
        if not cur.rowcount:
            sys.exit(f"Non c'è la segnalazione {n}.")
    print(f"Segnalazione {n}: {stato}" + (f" in {versione}" if stato == "risolta" else "") + ".")


def main():
    ap = argparse.ArgumentParser(description="Segnalazioni del prototipo Ambassadress.")
    sub = ap.add_subparsers(dest="cmd")
    s = sub.add_parser("server", help="accende il server (è anche il default)")
    s.add_argument("--porta", type=int, default=PORT)
    e = sub.add_parser("elenco", help="elenca le segnalazioni aperte")
    e.add_argument("--tutte", action="store_true", help="anche quelle chiuse")
    m = sub.add_parser("mostra", help="mostra una segnalazione")
    m.add_argument("id", type=int)
    m.add_argument("--json", action="store_true", help="solo il report, in JSON")
    c = sub.add_parser("stato", help="cambia lo stato di una segnalazione")
    c.add_argument("id", type=int)
    c.add_argument("stato", choices=STATI)
    c.add_argument("--versione", help="la versione che la risolve, per «risolta»")
    c.add_argument("--nota")
    a = ap.parse_args()
    if a.cmd == "elenco":
        elenco(a.tutte)
    elif a.cmd == "mostra":
        mostra(a.id, a.json)
    elif a.cmd == "stato":
        cambia(a.id, a.stato, a.versione, a.nota)
    else:
        server(getattr(a, "porta", PORT))


if __name__ == "__main__":
    main()
