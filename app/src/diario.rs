//! Le righe del log (giro 2): ogni riga comincia con la data e l'ora locali, così dopo
//! una notte si sa quando è caduta la rete e quanto è durata l'interruzione.

use std::time::{SystemTime, UNIX_EPOCH};

/// «2026-10-30 14:03:22», nell'ora del sistema.
pub fn adesso() -> String {
    let secondi = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let t = secondi as libc::time_t;
    // SAFETY: localtime_r legge `t` e scrive soltanto in `tm`, che è nostro.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return motore::tempo::iso(secondi as i64);
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// Come `eprintln!`, con la data e l'ora davanti.
#[macro_export]
macro_rules! diario {
    ($($a:tt)*) => {
        eprintln!("{} {}", $crate::diario::adesso(), format_args!($($a)*))
    };
}
