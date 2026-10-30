//! Quello che chiede al sistema: aprire file e link con i programmi predefiniti,
//! le notifiche del desktop, le cartelle. Per ora Linux (xdg-open, notify-send).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Dove l'app tiene casella, database e allegati: `$A10S_DATI`, altrimenti
/// `$XDG_DATA_HOME/a10s` (di solito `~/.local/share/a10s`).
pub fn cartella_dati() -> PathBuf {
    if let Some(d) = std::env::var_os("A10S_DATI") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| casa().join(".local/share"));
    base.join("a10s")
}

/// Dove vanno le copie degli allegati aperti con un altro programma:
/// `$XDG_CACHE_HOME/a10s/aperti` (di solito `~/.cache/a10s/aperti`).
pub fn cartella_aperti() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| casa().join(".cache"));
    base.join("a10s").join("aperti")
}

fn casa() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// La cartella Scaricati dell'utente (xdg-user-dir), altrimenti `~/Scaricati` o `~/Downloads`.
pub fn cartella_scaricati() -> PathBuf {
    if let Ok(out) = Command::new("xdg-user-dir").arg("DOWNLOAD").stderr(Stdio::null()).output() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() && Path::new(&p).is_dir() {
            return PathBuf::from(p);
        }
    }
    let c = casa();
    for nome in ["Scaricati", "Downloads"] {
        if c.join(nome).is_dir() {
            return c.join(nome);
        }
    }
    c
}

/// Un nome di file sicuro: niente percorsi, niente caratteri di controllo.
pub fn nome_sicuro(nome: &str) -> String {
    let n: String = nome
        .chars()
        .map(|c| if c == '/' || c == '\\' || c.is_control() { '_' } else { c })
        .collect();
    let n = n.trim().trim_start_matches('.').to_string();
    if n.is_empty() { "allegato".into() } else { n.chars().take(180).collect() }
}

/// Copia `sorgente` in `cartella` col nome dato; se il nome c'è già aggiunge « (2)», « (3)»…
pub fn copia_con_nome(sorgente: &Path, cartella: &Path, nome: &str) -> std::io::Result<PathBuf> {
    fs::create_dir_all(cartella)?;
    let nome = nome_sicuro(nome);
    let (base, est) = match nome.rfind('.') {
        Some(i) if i > 0 => (&nome[..i], &nome[i..]),
        _ => (nome.as_str(), ""),
    };
    let mut dest = cartella.join(&nome);
    let mut n = 2;
    while dest.exists() {
        dest = cartella.join(format!("{base} ({n}){est}"));
        n += 1;
    }
    fs::copy(sorgente, &dest)?;
    Ok(dest)
}

/// Apre un file o un link col programma predefinito, senza aspettarlo.
pub fn apri(cosa: &str) -> std::io::Result<()> {
    Command::new("xdg-open")
        .arg(cosa)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(aspetta)
}

/// Il processo figlio si raccoglie in un thread, così non resta appeso.
fn aspetta(mut figlio: std::process::Child) {
    std::thread::spawn(move || {
        let _ = figlio.wait();
    });
}

/// Una notifica del desktop. Se notify-send manca, niente.
pub fn notifica(titolo: &str, testo: &str) {
    let _ = Command::new("notify-send")
        .args(["--app-name=a10s", "--icon=mail-unread", titolo, testo])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(aspetta);
}

/// Il nome del programma che il sistema usa per questo tipo di file («Papers» per i PDF):
/// `xdg-mime query default <tipo>` dà il file `.desktop`, il nome sta lì dentro
/// (in italiano se c'è). Se non si trova, niente.
pub fn programma_per(tipo: &str) -> Option<String> {
    let tipo = tipo.trim();
    if tipo.is_empty() || !tipo.chars().all(|c| c.is_ascii_alphanumeric() || "/.+-_".contains(c)) {
        return None;
    }
    let out = Command::new("xdg-mime").args(["query", "default", tipo]).stderr(Stdio::null()).output().ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() || id.contains('/') {
        return None;
    }
    let mut cartelle = vec![std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| casa().join(".local/share"))];
    let sistema = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    cartelle.extend(sistema.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    let testo = cartelle.iter().find_map(|d| fs::read_to_string(d.join("applications").join(&id)).ok())?;
    nome_dal_desktop(&testo)
}

/// `Name[it]=` se c'è, altrimenti `Name=`, dalla sezione `[Desktop Entry]`.
fn nome_dal_desktop(testo: &str) -> Option<String> {
    let mut dentro = false;
    let (mut it, mut base) = (None, None);
    for riga in testo.lines() {
        let r = riga.trim();
        if r.starts_with('[') {
            dentro = r == "[Desktop Entry]";
            continue;
        }
        if !dentro {
            continue;
        }
        if let Some(v) = r.strip_prefix("Name[it]=") {
            it = Some(v.trim().to_string());
        } else if let Some(v) = r.strip_prefix("Name=") {
            base = Some(v.trim().to_string());
        }
    }
    it.or(base).filter(|n| !n.is_empty())
}

/// Scrive un testo in Scaricati col nome dato (« (2)» se c'è già) e dice dove.
pub fn salva_testo(nome: &str, testo: &str) -> std::io::Result<PathBuf> {
    let cartella = cartella_scaricati();
    fs::create_dir_all(&cartella)?;
    let dest = libero(&cartella, &nome_sicuro(nome));
    fs::write(&dest, testo)?;
    Ok(dest)
}

/// Il primo nome libero in `cartella`: `nome`, poi `nome (2)`, `nome (3)`…
pub fn libero(cartella: &Path, nome: &str) -> PathBuf {
    let (base, est) = match nome.rfind('.') {
        Some(i) if i > 0 => (&nome[..i], &nome[i..]),
        _ => (nome, ""),
    };
    let mut dest = cartella.join(nome);
    let mut n = 2;
    while dest.exists() {
        dest = cartella.join(format!("{base} ({n}){est}"));
        n += 1;
    }
    dest
}

/// Il percorso da mostrare: `~/…` per quello che sta nella cartella di casa.
pub fn da_mostrare(p: &Path) -> String {
    let casa = casa();
    match p.strip_prefix(&casa) {
        Ok(resto) => format!("~/{}", resto.to_string_lossy()),
        Err(_) => p.to_string_lossy().to_string(),
    }
}

/* ---------- l'icona: il file .desktop e le icone nella cartella dell'utente ---------- */

const ID_APP: &str = "it.elmisi.a10s";
const ICONE: [(&str, &[u8]); 4] = [
    ("32x32", include_bytes!("../icons/32x32.png")),
    ("128x128", include_bytes!("../icons/128x128.png")),
    ("256x256", include_bytes!("../icons/128x128@2x.png")),
    ("512x512", include_bytes!("../icons/icon.png")),
];

fn cartella_utente_dati() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| casa().join(".local/share"))
}

/// `--installa`: il file `.desktop` (nome «a10s», descrizione «Posta») e le icone L3,
/// senza sudo, così su Wayland la finestra ha la sua icona anche lanciata da `cargo run`.
/// La finestra si presenta come «ambassadress» (WM_CLASS su X11, app_id su Wayland: il nome
/// dell'eseguibile), e `StartupWMClass` la lega a questo file.
pub fn installa(eseguibile: &Path) -> std::io::Result<Vec<PathBuf>> {
    let base = cartella_utente_dati();
    let mut scritti = Vec::new();
    for (misura, png) in ICONE {
        let dir = base.join("icons/hicolor").join(misura).join("apps");
        fs::create_dir_all(&dir)?;
        let f = dir.join(format!("{ID_APP}.png"));
        fs::write(&f, png)?;
        scritti.push(f);
    }
    let dir = base.join("applications");
    fs::create_dir_all(&dir)?;
    let desktop = dir.join(format!("{ID_APP}.desktop"));
    let exec = eseguibile.to_string_lossy().replace('\\', "\\\\").replace(' ', "\\s");
    fs::write(
        &desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=a10s\nComment=Posta\nExec={exec}\nIcon={ID_APP}\nTerminal=false\nCategories=Network;Email;\nStartupWMClass=ambassadress\n"
        ),
    )?;
    scritti.push(desktop);
    Ok(scritti)
}

/// `--disinstalla`: toglie quello che `--installa` ha scritto.
pub fn disinstalla() -> std::io::Result<Vec<PathBuf>> {
    let base = cartella_utente_dati();
    let mut tolti = Vec::new();
    let mut file: Vec<PathBuf> = ICONE.iter().map(|(m, _)| base.join("icons/hicolor").join(m).join("apps").join(format!("{ID_APP}.png"))).collect();
    file.push(base.join("applications").join(format!("{ID_APP}.desktop")));
    for f in file {
        if f.exists() {
            fs::remove_file(&f)?;
            tolti.push(f);
        }
    }
    Ok(tolti)
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn nome_in_italiano_se_c_e() {
        let t = "[Desktop Entry]\nName=Papers\nName[it]=Documenti\nExec=papers %U\n[Desktop Action new]\nName=Nuova finestra\n";
        assert_eq!(nome_dal_desktop(t).as_deref(), Some("Documenti"));
        let t = "[Desktop Entry]\nName=Papers\n";
        assert_eq!(nome_dal_desktop(t).as_deref(), Some("Papers"));
        assert_eq!(nome_dal_desktop("[Altro]\nName=X\n"), None);
    }
}

/// Il lato più lungo delle miniature delle immagini, in pixel: basta per i quadratini di Allegati
/// anche su uno schermo ad alta densità.
const LATO_MINIATURA: i32 = 480;

/// La miniatura di un'immagine, in JPEG, con gdk-pixbuf (giro 2): ridotta mentre si legge, girata
/// come dice la foto (EXIF), le parti trasparenti su bianco. Scritta in un file temporaneo e poi
/// rinominata, così una miniatura a metà non c'è mai. Falso se l'immagine non si legge.
#[cfg(target_os = "linux")]
pub fn miniatura_immagine(file: &Path, dest: &Path) -> bool {
    use gdk_pixbuf::{Colorspace, InterpType, Pixbuf};
    let Ok(p) = Pixbuf::from_file_at_scale(file, LATO_MINIATURA, LATO_MINIATURA, true) else {
        return false;
    };
    let p = p.apply_embedded_orientation().unwrap_or(p);
    let p = if p.has_alpha() {
        let (w, h) = (p.width(), p.height());
        let Some(fondo) = Pixbuf::new(Colorspace::Rgb, false, 8, w, h) else { return false };
        fondo.fill(0xffff_ffff);
        p.composite(&fondo, 0, 0, w, h, 0.0, 0.0, 1.0, 1.0, InterpType::Bilinear, 255);
        fondo
    } else {
        p
    };
    let nome = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // il punto davanti: `pulisci_temporanei` del motore toglie quelli rimasti da un'interruzione
    let tmp = dest.with_file_name(format!(".{nome}.tmp"));
    if p.savev(&tmp, "jpeg", &[("quality", "82")]).is_err() {
        let _ = fs::remove_file(&tmp);
        return false;
    }
    fs::rename(&tmp, dest).is_ok()
}

#[cfg(not(target_os = "linux"))]
pub fn miniatura_immagine(_file: &Path, _dest: &Path) -> bool {
    false
}

#[cfg(all(test, target_os = "linux"))]
mod prove_miniature {
    use super::*;
    use gdk_pixbuf::{Colorspace, Pixbuf};

    /// Una foto grande (PNG con trasparenza, 3000×2000) diventa un JPEG di al massimo 480 px.
    #[test]
    fn miniatura_di_unimmagine_grande() {
        let dir = std::env::temp_dir().join(format!("a10s-miniatura-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let grande = Pixbuf::new(Colorspace::Rgb, true, 8, 3000, 2000).unwrap();
        grande.fill(0x3366_9980);
        let file = dir.join("foto");
        grande.savev(&file, "png", &[]).unwrap();
        let dest = dir.join("foto.miniatura.jpg");
        assert!(miniatura_immagine(&file, &dest));
        let m = Pixbuf::from_file(&dest).unwrap();
        assert_eq!((m.width(), m.height()), (480, 320));
        assert!(!m.has_alpha());
        let byte = fs::metadata(&dest).unwrap().len();
        assert!(byte < 60_000, "{byte} byte");
        assert!(fs::read(&dest).unwrap().starts_with(&[0xff, 0xd8]), "un JPEG");
        // un file che non è un'immagine: niente miniatura, niente file a metà
        fs::write(dir.join("rotto"), b"non sono un'immagine").unwrap();
        assert!(!miniatura_immagine(&dir.join("rotto"), &dir.join("rotto.miniatura.jpg")));
        assert!(!dir.join("rotto.miniatura.jpg").exists() && !dir.join(".rotto.miniatura.jpg.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
