//! Date e misure scritte a mano, per non aggiungere dipendenze.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn adesso() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Giorni dal 1970-01-01 per una data del calendario gregoriano.
fn giorni_da_civile(anno: i64, mese: u32, giorno: u32) -> i64 {
    let a = if mese <= 2 { anno - 1 } else { anno };
    let era = a.div_euclid(400);
    let ya = a - era * 400;
    let m = mese as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + giorno as i64 - 1;
    let doe = ya * 365 + ya / 4 - ya / 100 + doy;
    era * 146097 + doe - 719468
}

fn civile_da_giorni(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// INTERNALDATE di IMAP: `17-Jul-1996 02:44:25 -0700` (il giorno può avere uno spazio davanti).
pub fn da_internaldate(s: &str) -> Option<i64> {
    let s = s.trim();
    let (data, resto) = s.split_once(' ')?;
    let mut d = data.split('-');
    let giorno: u32 = d.next()?.trim().parse().ok()?;
    let mese = match d.next()?.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    let anno: i64 = d.next()?.parse().ok()?;
    let (ora, zona) = resto.trim().split_once(' ')?;
    let mut o = ora.split(':');
    let h: i64 = o.next()?.parse().ok()?;
    let m: i64 = o.next()?.parse().ok()?;
    let sec: i64 = o.next()?.parse().ok()?;
    let segno = match zona.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let zh: i64 = zona.get(1..3)?.parse().ok()?;
    let zm: i64 = zona.get(3..5)?.parse().ok()?;
    let locale = giorni_da_civile(anno, mese, giorno) * 86400 + h * 3600 + m * 60 + sec;
    Some(locale - segno * (zh * 3600 + zm * 60))
}

/// `2026-10-28 14:03 UTC`
pub fn data_utc(ts: i64) -> String {
    let (a, m, g) = civile_da_giorni(ts.div_euclid(86400));
    let s = ts.rem_euclid(86400);
    format!(
        "{a:04}-{m:02}-{g:02} {:02}:{:02} UTC",
        s / 3600,
        (s % 3600) / 60
    )
}

/// `14:03:21` (UTC), per le righe di `ascolta`.
pub fn ora_utc() -> String {
    let s = adesso().rem_euclid(86400);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

pub fn durata(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.2} s", d.as_secs_f64())
    } else {
        let s = d.as_secs();
        format!("{} min {:02} s", s / 60, s % 60)
    }
}

pub fn byte(n: u64) -> String {
    const K: f64 = 1024.0;
    let f = n as f64;
    if f < K {
        format!("{n} B")
    } else if f < K * K {
        format!("{:.1} KB", f / K)
    } else if f < K * K * K {
        format!("{:.1} MB", f / K / K)
    } else {
        format!("{:.2} GB", f / K / K / K)
    }
}

/// Numero con il punto delle migliaia: `48.213`.
pub fn numero(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn internaldate() {
        assert_eq!(
            da_internaldate("17-Jul-1996 02:44:25 -0700"),
            Some(837596665)
        );
        assert_eq!(
            da_internaldate(" 1-Jan-2026 00:00:00 +0000"),
            Some(1767225600)
        );
        assert_eq!(da_internaldate("rotta"), None);
    }

    #[test]
    fn date_e_numeri() {
        assert_eq!(data_utc(1767225600), "2026-01-01 00:00 UTC");
        assert_eq!(numero(48213), "48.213");
        assert_eq!(numero(999), "999");
        assert_eq!(numero(1000000), "1.000.000");
        assert_eq!(byte(2048), "2.0 KB");
    }
}
