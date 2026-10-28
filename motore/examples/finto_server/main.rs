//! Il finto server IMAP con la casella sintetica, da lanciare a mano per provare
//! il motore e l'app senza Gmail:
//!
//! ```text
//! node tools/sintetici-in-eml.mjs
//! cargo run --example finto_server -- --cartella prove/sintetici --porta 1143
//! ```
//!
//! In chiaro, solo su 127.0.0.1. Utente e password sono finti e si stampano all'avvio.

mod finto;

use std::path::PathBuf;
use std::time::Duration;

use finto::{Casella, Opzioni, Server};

const AIUTO: &str = "\
finto_server: un server IMAP finto con la casella sintetica (solo 127.0.0.1, in chiaro)

Uso:
  cargo run --example finto_server -- [opzioni]

Opzioni:
  --cartella DIR      file .eml e .meta (predefinita motore/prove/sintetici)
  --porta N           porta su 127.0.0.1 (predefinita 1143)
  --utente U          utente accettato (predefinito: l'indirizzo dell'account sintetico)
  --password P        password accettata (predefinita finta-password)
  --arrivi SECONDI    fa arrivare un messaggio nuovo ogni SECONDI (notificato in IDLE)
  --riserva N         con --arrivi: tiene da parte gli N messaggi più recenti e li fa
                      arrivare per primi (predefinito 5); poi ricopia quelli vecchi
  --cadute SECONDI    ogni SECONDI fa cadere tutte le connessioni
  --pausa SECONDI     dopo una caduta rifiuta le connessioni per SECONDI (predefinito 5)
  --copie N           moltiplica la casella per N (per misure su migliaia di messaggi)
  --lento MS          aspetta MS millisecondi prima di rispondere a ogni comando
  --verboso           stampa ogni comando ricevuto (la password di LOGIN no)
";

fn numero(v: Option<String>, nome: &str) -> Result<u64, String> {
    v.ok_or_else(|| format!("{nome} vuole un valore"))?
        .parse()
        .map_err(|_| format!("{nome} vuole un numero"))
}

fn main() {
    if let Err(e) = avvia() {
        eprintln!("Errore: {e}\n\n{AIUTO}");
        std::process::exit(2);
    }
}

fn avvia() -> Result<(), String> {
    let mut cartella = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prove/sintetici");
    let mut porta = 1143u16;
    let mut opz = Opzioni {
        registra: false,
        ..Opzioni::default()
    };
    let mut utente: Option<String> = None;
    let mut riserva = 5usize;
    let mut copie = 1usize;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--cartella" => {
                cartella = it
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--cartella vuole un valore")?
            }
            "--porta" => porta = numero(it.next(), "--porta")? as u16,
            "--utente" => utente = Some(it.next().ok_or("--utente vuole un valore")?),
            "--password" => opz.password = it.next().ok_or("--password vuole un valore")?,
            "--arrivi" => {
                opz.arrivi = Some(Duration::from_secs(numero(it.next(), "--arrivi")?.max(1)))
            }
            "--riserva" => riserva = numero(it.next(), "--riserva")? as usize,
            "--cadute" => {
                opz.cadute = Some(Duration::from_secs(numero(it.next(), "--cadute")?.max(1)))
            }
            "--pausa" => opz.pausa = Duration::from_secs(numero(it.next(), "--pausa")?),
            "--copie" => copie = numero(it.next(), "--copie")?.max(1) as usize,
            "--lento" => opz.ritardo = Duration::from_millis(numero(it.next(), "--lento")?),
            "--verboso" => opz.verboso = true,
            "--aiuto" | "--help" | "-h" => {
                print!("{AIUTO}");
                return Ok(());
            }
            altro => return Err(format!("opzione sconosciuta: {altro}")),
        }
    }
    let (mut casella, account) = Casella::carica(&cartella)?;
    if copie > 1 {
        casella.moltiplica(copie);
    }
    if opz.arrivi.is_some() {
        casella.tieni_da_parte(riserva);
    }
    opz.utente = utente.unwrap_or_else(|| {
        if account.indirizzo.is_empty() {
            finto::UTENTE_PREDEFINITO.to_string()
        } else {
            account.indirizzo.clone()
        }
    });
    let (n, esclusi, da_parte) = (
        casella.messaggi.len(),
        casella.esclusi.len(),
        casella.riserva.len(),
    );
    let server = Server::avvia(casella, opz.clone(), &format!("127.0.0.1:{porta}"))
        .map_err(|e| format!("non riesco ad ascoltare su 127.0.0.1:{porta}: {e}"))?;

    println!(
        "Finto server IMAP in ascolto su 127.0.0.1:{} (in chiaro)",
        server.porta
    );
    println!("  utente    {}", opz.utente);
    println!("  password  {}", opz.password);
    if !account.nome.is_empty() {
        println!(
            "  account   {} · alias: {}",
            account.nome,
            account.alias.join(", ")
        );
    }
    println!(
        "  cartella  «{}»: {n} messaggi ({esclusi} fra spam e cestino restano fuori, come su Gmail)",
        finto::CARTELLA_TUTTI
    );
    if let Some(a) = opz.arrivi {
        println!(
            "  arrivi    uno ogni {} s ({da_parte} tenuti da parte, poi copie)",
            a.as_secs()
        );
    }
    if !opz.ritardo.is_zero() {
        println!(
            "  lento     {} ms prima di ogni risposta",
            opz.ritardo.as_millis()
        );
    }
    if let Some(c) = opz.cadute {
        println!(
            "  cadute    ogni {} s, poi {} s di connessioni rifiutate",
            c.as_secs(),
            opz.pausa.as_secs()
        );
    }
    println!(
        "Per il motore: --server 127.0.0.1:{} --in-chiaro --credenziali FILE (due righe: utente e password)",
        server.porta
    );
    println!("Ctrl+C per fermarlo.");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
