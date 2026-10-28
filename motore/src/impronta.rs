//! SHA-256 (FIPS 180-4) e MD5 (RFC 1321), scritti qui per non aggiungere dipendenze.
//!
//! Serve all'archivio degli allegati, che li salva per contenuto: lo stesso file
//! in più email occupa spazio una volta sola. Un'impronta crittografica invece di
//! FNV-1a perché il contenuto arriva da chiunque ci scriva: con un'impronta debole
//! chi manda un allegato costruito apposta potrebbe farlo coincidere con quello di
//! un altro messaggio e sostituirne il contenuto nell'archivio. Con SHA-256 non si
//! può, e la dimensione nella chiave non serve.
//!
//! MD5 serve solo al nome dei file `.eml` di «Salva» (i primi 6 caratteri, come nel
//! prototipo): lì non deve resistere a nessuno, basta che sia lo stesso dell'interfaccia.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INIZIALE: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

fn blocco(h: &mut [u32; 8], b: &[u8]) {
    let mut w = [0u32; 64];
    for (i, parola) in b.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([parola[0], parola[1], parola[2], parola[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut bb, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & bb) ^ (a & c) ^ (bb & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = bb;
        bb = a;
        a = t1.wrapping_add(t2);
    }
    for (x, y) in h.iter_mut().zip([a, bb, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(y);
    }
}

/// SHA-256 dei dati.
pub fn sha256(dati: &[u8]) -> [u8; 32] {
    let mut h = INIZIALE;
    let mut pezzi = dati.chunks_exact(64);
    for b in &mut pezzi {
        blocco(&mut h, b);
    }
    let resto = pezzi.remainder();
    let mut coda = [0u8; 128];
    coda[..resto.len()].copy_from_slice(resto);
    coda[resto.len()] = 0x80;
    let lunghezza = if resto.len() < 56 { 64 } else { 128 };
    coda[lunghezza - 8..lunghezza].copy_from_slice(&((dati.len() as u64) * 8).to_be_bytes());
    for b in coda[..lunghezza].chunks_exact(64) {
        blocco(&mut h, b);
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// SHA-256 in esadecimale minuscolo: il nome del file nell'archivio.
pub fn sha256_esadecimale(dati: &[u8]) -> String {
    sha256(dati).iter().map(|b| format!("{b:02x}")).collect()
}

/// MD5 dei dati (RFC 1321). Solo per i nomi dei file, mai per la sicurezza.
pub fn md5(dati: &[u8]) -> [u8; 16] {
    const S: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    // K[i] = parte intera di |sin(i + 1)| · 2^32
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut blocco = |b: &[u8]| {
        let mut m = [0u32; 16];
        for (i, p) in b.chunks_exact(4).enumerate() {
            m[i] = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
        }
        let [mut a, mut bb, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((bb & c) | (!bb & d), i),
                1 => ((d & bb) | (!d & c), (5 * i + 1) % 16),
                2 => (bb ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (bb | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = bb;
            bb = bb.wrapping_add(f.rotate_left(S[(i / 16) * 4 + i % 4]));
        }
        for (x, y) in h.iter_mut().zip([a, bb, c, d]) {
            *x = x.wrapping_add(y);
        }
    };
    let mut pezzi = dati.chunks_exact(64);
    for b in &mut pezzi {
        blocco(b);
    }
    let resto = pezzi.remainder();
    let mut coda = [0u8; 128];
    coda[..resto.len()].copy_from_slice(resto);
    coda[resto.len()] = 0x80;
    let lunghezza = if resto.len() < 56 { 64 } else { 128 };
    coda[lunghezza - 8..lunghezza]
        .copy_from_slice(&((dati.len() as u64).wrapping_mul(8)).to_le_bytes());
    for b in coda[..lunghezza].chunks_exact(64) {
        blocco(b);
    }
    let mut out = [0u8; 16];
    for (i, x) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
    }
    out
}

/// MD5 in esadecimale minuscolo.
pub fn md5_esadecimale(dati: &[u8]) -> String {
    md5(dati).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod prove {
    use super::sha256_esadecimale as h;

    #[test]
    fn vettori_noti() {
        assert_eq!(
            h(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            h(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            h(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            h(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // 55, 56 e 64 byte: i casi limite del riempimento.
        assert_eq!(
            h(&[b'x'; 55]),
            "d5e285683cd4efc02d021a5c62014694958901005d6f71e89e0989fac77e4072"
        );
        assert_eq!(
            h(&[b'x'; 56]),
            "04c26261370ee7541549d16dee320c723e3fd14671e66a099afe0a377c16888e"
        );
        assert_eq!(
            h(&[b'x'; 64]),
            "7ce100971f64e7001e8fe5a51973ecdfe1ced42befe7ee8d5fd6219506b5393c"
        );
    }

    #[test]
    fn md5_della_rfc_1321() {
        use super::md5_esadecimale as m;
        // A.5 Test suite
        assert_eq!(m(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(m(b"a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(m(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(m(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        assert_eq!(
            m(b"abcdefghijklmnopqrstuvwxyz"),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
        assert_eq!(
            m(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"),
            "d174ab98d277d9f5a5611c2c9f419d9f"
        );
        assert_eq!(
            m("1234567890".repeat(8).as_bytes()),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
        // Testo UTF-8 (confrontato con md5sum) e i casi limite del riempimento.
        assert_eq!(
            m("Perché sì, è così".as_bytes()),
            "bf5d849ac1964fadd89bd89b9773419a"
        );
        assert_eq!(
            m("日本語のテキスト".as_bytes()),
            "6161712f6b31d73e38679943c0bbea92"
        );
        assert_eq!(
            m(b"case-inoltro-0@esempio.it"),
            "4f6f3f5d854271b2dcbf75f8054cf147"
        );
        assert_eq!(m(&[b'x'; 55]), "04364420e25c512fd958a70738aa8f72");
        assert_eq!(m(&[b'x'; 56]), "668a72d5ba17f08e62dabcafad6db14b");
        assert_eq!(m(&[b'x'; 63]), "7dc2ca208106a2f703567bdff99d8981");
        assert_eq!(m(&[b'x'; 64]), "c1bb4f81d892b2d57947682aeb252456");
    }
}
