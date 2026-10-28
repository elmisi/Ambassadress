//! UTF-7 modificato di IMAP (RFC 3501, 5.1.3), usato nei nomi delle cartelle
//! e delle etichette di Gmail.

fn valore_base64(c: u8) -> Option<u32> {
    Some(match c {
        b'A'..=b'Z' => (c - b'A') as u32,
        b'a'..=b'z' => (c - b'a' + 26) as u32,
        b'0'..=b'9' => (c - b'0' + 52) as u32,
        b'+' => 62,
        b',' => 63,
        _ => return None,
    })
}

/// Decodifica; se il testo non è UTF-7 modificato valido lo restituisce com'è.
pub fn decodifica(s: &str) -> String {
    decodifica_stretta(s).unwrap_or_else(|| s.to_string())
}

fn decodifica_stretta(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'&' {
            out.push(b[i] as char);
            if !b[i].is_ascii() {
                // Già UTF-8 (server con UTF8=ACCEPT): niente da decodificare.
                return None;
            }
            i += 1;
            continue;
        }
        let fine = i + 1 + b[i + 1..].iter().position(|&c| c == b'-')?;
        if fine == i + 1 {
            out.push('&');
            i = fine + 1;
            continue;
        }
        let mut bits = 0u32;
        let mut n = 0;
        let mut unita = Vec::new();
        for &c in &b[i + 1..fine] {
            bits = (bits << 6) | valore_base64(c)?;
            n += 6;
            if n >= 16 {
                n -= 16;
                unita.push(((bits >> n) & 0xFFFF) as u16);
            }
        }
        out.push_str(&String::from_utf16(&unita).ok()?);
        i = fine + 1;
    }
    Some(out)
}

#[cfg(test)]
mod prove {
    use super::decodifica;

    #[test]
    fn nomi() {
        assert_eq!(decodifica("INBOX"), "INBOX");
        assert_eq!(decodifica("[Gmail]/Posta inviata"), "[Gmail]/Posta inviata");
        assert_eq!(decodifica("Priorit&AOA-"), "Priorità");
        assert_eq!(decodifica("A&-B"), "A&B");
        assert_eq!(decodifica("&ZeVnLIqe-"), "日本語");
        assert_eq!(decodifica("rotto&AOA"), "rotto&AOA");
    }
}
