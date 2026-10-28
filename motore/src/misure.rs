//! Larghezza e altezza di un'immagine lette dalle prime righe del file, senza
//! librerie: PNG, GIF, JPEG, WebP e BMP. Servono all'interfaccia per nascondere da
//! sole le immagini piccole e le strisce delle firme (proposta 5 delle note).
//!
//! Si legge solo l'intestazione: niente decodifica, niente memoria in più. Un file
//! rotto o di un altro formato dà `None`. L'orientamento EXIF dei JPEG non si
//! guarda: per «piccola o grande» non cambia niente.

/// Larghezza e altezza in pixel, se il formato è uno di quelli noti e le misure
/// non sono zero.
pub fn misure_immagine(d: &[u8]) -> Option<(u32, u32)> {
    let (l, a) = if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(d)?
    } else if d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a") {
        (le16(d, 6)? as u32, le16(d, 8)? as u32)
    } else if d.starts_with(&[0xff, 0xd8]) {
        jpeg(d)?
    } else if d.len() >= 12 && &d[0..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        webp(d)?
    } else if d.starts_with(b"BM") {
        bmp(d)?
    } else {
        return None;
    };
    (l > 0 && a > 0).then_some((l, a))
}

fn le16(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(i..i + 2)?.try_into().ok()?))
}

fn be16(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(i..i + 2)?.try_into().ok()?))
}

fn le32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

fn be32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

fn le24(d: &[u8], i: usize) -> Option<u32> {
    let b = d.get(i..i + 3)?;
    Some(b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16)
}

/// Il primo blocco dopo la firma è IHDR: larghezza e altezza a 32 bit.
fn png(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be32(d, 16)?, be32(d, 20)?))
}

/// I segmenti uno dopo l'altro fino al primo SOFn (C0..CF tranne C4 DHT, C8 JPG e CC DAC).
fn jpeg(d: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    loop {
        // Ogni marcatore comincia con FF, eventualmente ripetuto come riempimento.
        if *d.get(i)? != 0xff {
            return None;
        }
        while *d.get(i)? == 0xff {
            i += 1;
        }
        let m = *d.get(i)?;
        i += 1;
        match m {
            // Senza lunghezza: TEM, RST0..7, SOI.
            0x01 | 0xd0..=0xd8 => continue,
            // EOI o inizio dei dati compressi prima di un SOF: niente misure.
            0xd9 | 0xda => return None,
            0xc0..=0xcf if m != 0xc4 && m != 0xc8 && m != 0xcc => {
                // lunghezza (2), precisione (1), altezza (2), larghezza (2)
                let a = be16(d, i + 3)? as u32;
                let l = be16(d, i + 5)? as u32;
                return Some((l, a));
            }
            _ => {
                let n = be16(d, i)? as usize;
                if n < 2 {
                    return None;
                }
                i += n;
            }
        }
    }
}

fn webp(d: &[u8]) -> Option<(u32, u32)> {
    match d.get(12..16)? {
        // Con perdita: il fotogramma chiave comincia con 9D 01 2A, poi le misure a 14 bit.
        b"VP8 " => {
            if d.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            Some((
                (le16(d, 26)? & 0x3fff) as u32,
                (le16(d, 28)? & 0x3fff) as u32,
            ))
        }
        // Senza perdita: firma 2F, poi larghezza-1 e altezza-1 su 14 bit ciascuna.
        b"VP8L" => {
            if *d.get(20)? != 0x2f {
                return None;
            }
            let b = le32(d, 21)?;
            Some(((b & 0x3fff) + 1, ((b >> 14) & 0x3fff) + 1))
        }
        // Esteso: misure della tela, meno uno, a 24 bit.
        b"VP8X" => Some((le24(d, 24)? + 1, le24(d, 27)? + 1)),
        _ => None,
    }
}

fn bmp(d: &[u8]) -> Option<(u32, u32)> {
    match le32(d, 14)? {
        // BITMAPCOREHEADER (OS/2): misure a 16 bit.
        12 => Some((le16(d, 18)? as u32, le16(d, 20)? as u32)),
        // BITMAPINFOHEADER e successivi: a 32 bit con segno; altezza negativa = dall'alto.
        n if n >= 40 => {
            let l = le32(d, 18)? as i32;
            let a = le32(d, 22)? as i32;
            Some((l.unsigned_abs(), a.unsigned_abs()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod prove {
    use super::misure_immagine as m;

    fn png(l: u32, a: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend(l.to_be_bytes());
        v.extend(a.to_be_bytes());
        v.extend([8, 6, 0, 0, 0]);
        v
    }

    #[test]
    fn png_e_gif() {
        assert_eq!(m(&png(300, 120)), Some((300, 120)));
        assert_eq!(m(&png(0, 120)), None);
        assert_eq!(m(&png(300, 120)[..20]), None, "intestazione tagliata");
        let mut gif = b"GIF89a".to_vec();
        gif.extend(640u16.to_le_bytes());
        gif.extend(48u16.to_le_bytes());
        gif.extend([0xf7, 0, 0]);
        assert_eq!(m(&gif), Some((640, 48)));
        gif[4] = b'7';
        assert_eq!(m(&gif), Some((640, 48)));
    }

    #[test]
    fn jpeg() {
        // SOI, APP0 JFIF, DQT, DHT (C4: da saltare), riempimento FF, SOF2 progressivo.
        let mut j = vec![0xff, 0xd8];
        j.extend([0xff, 0xe0, 0, 16]);
        j.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        j.extend([0xff, 0xdb, 0, 4, 0, 0]);
        j.extend([0xff, 0xc4, 0, 5, 1, 2, 3]);
        j.extend([0xff, 0xff, 0xc2, 0, 17, 8]);
        j.extend(360u16.to_be_bytes()); // altezza
        j.extend(255u16.to_be_bytes()); // larghezza
        j.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        assert_eq!(m(&j), Some((255, 360)));
        // Senza SOF prima dei dati: niente misure.
        let senza = [0xff, 0xd8, 0xff, 0xda, 0, 2];
        assert_eq!(m(&senza), None);
        assert_eq!(m(&[0xff, 0xd8, 0xff, 0xe1, 0xff]), None);
        // DAC (CC) e JPG (C8) non sono SOF.
        let mut dac = vec![0xff, 0xd8, 0xff, 0xcc, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8];
        dac.extend(10u16.to_be_bytes());
        dac.extend(20u16.to_be_bytes());
        dac.extend([1, 1, 0x11, 0]);
        assert_eq!(m(&dac), Some((20, 10)));
    }

    fn riff(blocco: &[u8], dati: &[u8]) -> Vec<u8> {
        let mut v = b"RIFF\0\0\0\0WEBP".to_vec();
        v.extend(blocco);
        v.extend((dati.len() as u32).to_le_bytes());
        v.extend(dati);
        v
    }

    #[test]
    fn webp() {
        // VP8: tag del fotogramma (3), 9D 01 2A, larghezza e altezza con 2 bit di scala.
        let mut vp8 = vec![0x30, 0x01, 0x00, 0x9d, 0x01, 0x2a];
        vp8.extend((800u16 | 0x4000).to_le_bytes());
        vp8.extend(600u16.to_le_bytes());
        assert_eq!(m(&riff(b"VP8 ", &vp8)), Some((800, 600)));
        // VP8L: 2F, poi (l-1) | (a-1) << 14.
        let bits: u32 = (99) | (49 << 14);
        let mut vp8l = vec![0x2f];
        vp8l.extend(bits.to_le_bytes());
        assert_eq!(m(&riff(b"VP8L", &vp8l)), Some((100, 50)));
        // VP8X: flag (4), poi tela-1 a 24 bit.
        let mut vp8x = vec![0x10, 0, 0, 0];
        vp8x.extend(&1919u32.to_le_bytes()[..3]);
        vp8x.extend(&1079u32.to_le_bytes()[..3]);
        assert_eq!(m(&riff(b"VP8X", &vp8x)), Some((1920, 1080)));
        assert_eq!(m(&riff(b"ALTR", &vp8x)), None);
    }

    #[test]
    fn bmp() {
        let mut b = b"BM".to_vec();
        b.extend([0u8; 12]);
        b.extend(40u32.to_le_bytes());
        b.extend(320i32.to_le_bytes());
        b.extend((-90i32).to_le_bytes());
        assert_eq!(m(&b), Some((320, 90)));
        let mut core = b"BM".to_vec();
        core.extend([0u8; 12]);
        core.extend(12u32.to_le_bytes());
        core.extend(64u16.to_le_bytes());
        core.extend(32u16.to_le_bytes());
        assert_eq!(m(&core), Some((64, 32)));
    }

    #[test]
    fn altri_formati() {
        assert_eq!(m(b"%PDF-1.4"), None);
        assert_eq!(m(b""), None);
        assert_eq!(m(b"<svg width='10' height='10'/>"), None);
    }
}
