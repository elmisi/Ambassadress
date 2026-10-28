#!/usr/bin/env node
// Trasforma la casella sintetica del prototipo (mockups/data/synthetic.js e i file
// in mockups/data/attachments/) in messaggi RFC 822 veri, uno per file .eml, con
// accanto un file .meta che il finto server IMAP del motore legge: UID, X-GM-MSGID,
// X-GM-THRID, etichette, flag, INTERNALDATE e le impronte attese di testi e allegati
// (le usano le prove automatiche per controllare che il motore li ricostruisca uguali).
//
//   node tools/sintetici-in-eml.mjs                      # scrive in motore/prove/sintetici/
//   node tools/sintetici-in-eml.mjs --uscita CARTELLA
//   node tools/sintetici-in-eml.mjs --senza-spostamento  # date originali, non spostate a oggi
//
// Legge solo i dati sintetici: nessun percorso verso i dati veri.
//
// Le date si spostano come fa il prototipo (meta.shiftToNow): di un numero intero di
// giorni, così l'ultimo messaggio cade oggi. Il MIME imita i programmi veri: testo e
// HTML in multipart/alternative, allegati in multipart/mixed (base64), immagini
// incorporate in multipart/related con Content-ID, set di caratteri e codifiche
// diversi secondo il programma di chi scrive (UTF-8 o windows-1252; 7bit, 8bit,
// quoted-printable, base64), parole codificate RFC 2047 e nomi di file RFC 2231.

import { readFileSync, writeFileSync, mkdirSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { join, dirname, resolve, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import zlib from 'node:zlib';

const QUI = dirname(fileURLToPath(import.meta.url));
const RADICE = resolve(QUI, '..');
const DATI = join(RADICE, 'mockups', 'data');

const argomenti = process.argv.slice(2);
let uscita = join(RADICE, 'motore', 'prove', 'sintetici');
let sposta = true;
for (let i = 0; i < argomenti.length; i++) {
  if (argomenti[i] === '--uscita') uscita = resolve(argomenti[++i] || '');
  else if (argomenti[i] === '--senza-spostamento') sposta = false;
  else {
    console.error(`opzione sconosciuta: ${argomenti[i]}`);
    process.exit(2);
  }
}

// ---------- lettura dei dati sintetici ----------

const contesto = { window: {} };
vm.runInNewContext(readFileSync(join(DATI, 'synthetic.js'), 'utf8'), contesto);
const dati = contesto.window.AMB_DATA;
if (!dati || !Array.isArray(dati.messages)) {
  console.error('synthetic.js non definisce window.AMB_DATA');
  process.exit(1);
}

// ---------- piccoli strumenti ----------

const CRLF = '\r\n';
const pad = (n, l = 2) => String(n).padStart(l, '0');
const ascii = (s) => /^[\x00-\x7f]*$/.test(s);

function fnv64(buf) {
  let h = 0xcbf29ce484222325n;
  for (const b of buf) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return h;
}
const hex = (n, l = 16) => n.toString(16).padStart(l, '0');

const CRC = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// ---------- date ----------

const GIORNI = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const MESI = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
function dataRfc(t) {
  const d = new Date(t);
  const off = -d.getTimezoneOffset();
  const a = Math.abs(off);
  return `${GIORNI[d.getDay()]}, ${pad(d.getDate())} ${MESI[d.getMonth()]} ${d.getFullYear()} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())} ${off >= 0 ? '+' : '-'}${pad(Math.floor(a / 60))}${pad(a % 60)}`;
}
const inizioGiorno = (t) => { const d = new Date(t); d.setHours(0, 0, 0, 0); return d.getTime(); };

let spostamento = 0;
if (sposta && dati.meta && dati.meta.shiftToNow) {
  let max = 0;
  for (const m of dati.messages) {
    const t = Date.parse(m.date || m.internalDate);
    if (t > max) max = t;
  }
  if (max) spostamento = inizioGiorno(Date.now()) - inizioGiorno(max);
}

// ---------- set di caratteri e codifiche ----------

const CP1252_ALTI = '€\u0081‚ƒ„…†‡ˆ‰Š‹Œ\u008dŽ\u008f\u0090‘’“”•–—˜™š›œ\u009džŸ';
function inCp1252(s) {
  const out = [];
  for (const c of s) {
    const u = c.codePointAt(0);
    if (u < 0x80 || (u >= 0xa0 && u <= 0xff)) out.push(u);
    else {
      const i = CP1252_ALTI.indexOf(c);
      if (i < 0) return null;
      out.push(0x80 + i);
    }
  }
  return Buffer.from(out);
}

// Quoted-printable (RFC 2045): righe al massimo di 76 caratteri, a capo veri in CRLF,
// spazi in fondo alla riga codificati.
function qp(buf) {
  const righe = [];
  let inizio = 0;
  const pezzi = [];
  for (let i = 0; i <= buf.length; i++) {
    if (i === buf.length || buf[i] === 0x0a) {
      let fine = i;
      if (fine > inizio && buf[fine - 1] === 0x0d) fine--;
      pezzi.push(buf.subarray(inizio, fine));
      inizio = i + 1;
    }
  }
  for (const riga of pezzi) {
    let cur = '';
    for (let i = 0; i < riga.length; i++) {
      const b = riga[i];
      const ultimo = i === riga.length - 1;
      let t;
      if ((b === 0x20 || b === 0x09) && !ultimo) t = String.fromCharCode(b);
      else if (b >= 33 && b <= 126 && b !== 61) t = String.fromCharCode(b);
      else t = '=' + b.toString(16).toUpperCase().padStart(2, '0');
      if (cur.length + t.length > 75) {
        righe.push(cur + '=');
        cur = '';
      }
      cur += t;
    }
    righe.push(cur);
  }
  return righe.join(CRLF);
}

const base64 = (buf) => (buf.toString('base64').match(/.{1,76}/g) || ['']).join(CRLF);

// Parola codificata RFC 2047, spezzata in pezzi di 75 caratteri al massimo.
function paroleCodificate(s, stile) {
  const parole = [];
  if (stile === 'B') {
    let cur = [];
    let byte = 0;
    for (const c of s) {
      const b = Buffer.byteLength(c);
      if (byte + b > 45 && cur.length) {
        parole.push(`=?UTF-8?B?${Buffer.from(cur.join('')).toString('base64')}?=`);
        cur = [];
        byte = 0;
      }
      cur.push(c);
      byte += b;
    }
    if (cur.length) parole.push(`=?UTF-8?B?${Buffer.from(cur.join('')).toString('base64')}?=`);
    return parole;
  }
  let cur = '';
  for (const c of s) {
    let t;
    if (/[A-Za-z0-9!*+\-/]/.test(c)) t = c;
    else if (c === ' ') t = '_';
    else t = [...Buffer.from(c)].map((b) => '=' + b.toString(16).toUpperCase().padStart(2, '0')).join('');
    if (cur.length + t.length > 63) {
      parole.push(`=?UTF-8?Q?${cur}?=`);
      cur = '';
    }
    cur += t;
  }
  if (cur) parole.push(`=?UTF-8?Q?${cur}?=`);
  return parole;
}

// ---------- intestazioni ----------

// Piega una intestazione fatta di pezzi: va a capo prima di un pezzo che farebbe superare 76 caratteri.
function piega(nome, pezzi, separatore = ' ') {
  let riga = `${nome}:`;
  const righe = [];
  pezzi.forEach((p, i) => {
    const sep = i === 0 ? ' ' : separatore;
    if (riga.length + sep.length + p.length > 76 && i > 0) {
      righe.push(riga + sep.trimEnd());
      riga = ' ' + p;
    } else riga += sep + p;
  });
  righe.push(riga);
  return righe.join(CRLF);
}

function frase(nome, stile) {
  if (!ascii(nome)) return paroleCodificate(nome, stile).join(' ');
  if (/[()<>[\]:;@\\,."]/.test(nome)) return `"${nome.replace(/["\\]/g, '\\$&')}"`;
  return nome;
}

function indirizzo(p, stile) {
  if (!p || !p.address) return '';
  if (!p.name || p.name.toLowerCase() === p.address.toLowerCase()) return p.address;
  return `${frase(p.name, stile)} <${p.address}>`;
}

function intestazioneIndirizzi(nome, lista, stile) {
  const v = (lista || []).map((p) => indirizzo(p, stile)).filter(Boolean);
  if (!v.length) return null;
  return piega(nome, v.map((x, i) => (i < v.length - 1 ? x + ',' : x)));
}

function intestazioneTesto(nome, valore, stile) {
  if (ascii(valore)) return piega(nome, valore.split(' ').filter((x, i, a) => x || i < a.length - 1));
  return piega(nome, paroleCodificate(valore, stile));
}

function citato(v) {
  return `"${v.replace(/["\\]/g, '\\$&')}"`;
}

// Parametro con un nome di file: ASCII tra virgolette, altrimenti RFC 2231 (spezzato se lungo).
function parametroNome(chiave, nome) {
  if (ascii(nome)) return [`${chiave}=${citato(nome)}`];
  const pct = [...Buffer.from(nome)]
    .map((b) => (/[A-Za-z0-9!#$&+\-.^_`|~]/.test(String.fromCharCode(b)) ? String.fromCharCode(b) : '%' + b.toString(16).toUpperCase().padStart(2, '0')))
    .join('');
  if (pct.length <= 50) return [`${chiave}*=UTF-8''${pct}`];
  const pezzi = [];
  let resto = pct;
  let i = 0;
  while (resto.length) {
    let n = Math.min(40, resto.length);
    // non spezzare una sequenza %XX
    while (n < resto.length && resto[n - 1] === '%') n--;
    while (n < resto.length && n >= 2 && resto[n - 2] === '%') n -= 2;
    pezzi.push(`${chiave}*${i}*=${i === 0 ? "UTF-8''" : ''}${resto.slice(0, n)}`);
    resto = resto.slice(n);
    i++;
  }
  return pezzi;
}

function contentType(tipo, parametri) {
  return piega('Content-Type', [tipo + (parametri.length ? ';' : '')].concat(parametri.map((p, i) => (i < parametri.length - 1 ? p + ';' : p))));
}

// ---------- programmi di chi scrive ----------

function programma(m) {
  const c = (m.from && m.from.client) || '';
  if (c.startsWith('outlook')) return 'outlook';
  if (c.startsWith('gmail')) return 'gmail';
  if (c) return c; // apple, iphone, thunderbird
  return 'servizio';
}

function confine(prog, seme) {
  const h = hex(fnv64(Buffer.from(seme)));
  switch (prog) {
    case 'gmail': return '000000000000' + h.slice(4);
    case 'outlook': return `_000_${h.toUpperCase()}AMSPR02MB${h.slice(0, 4).toUpperCase()}eurp_`;
    case 'apple':
    case 'iphone': return `Apple-Mail=_${h.slice(0, 8).toUpperCase()}-${h.slice(8, 12).toUpperCase()}-${h.slice(12).toUpperCase()}`;
    case 'thunderbird': return '------------' + h + h.slice(0, 8);
    default: return '=_a10s_' + h;
  }
}

function intestazioniProgramma(prog) {
  switch (prog) {
    case 'outlook': return [['X-Mailer', 'Microsoft Outlook 16.0'], ['X-MS-Has-Attach', '']];
    case 'apple': return [['X-Mailer', 'Apple Mail (2.3826.400.131.1.6)']];
    case 'iphone': return [['X-Mailer', 'iPhone Mail (22A3354)']];
    case 'thunderbird': return [['User-Agent', 'Mozilla Thunderbird']];
    default: return [];
  }
}

// ---------- parti di testo ----------

// Sceglie set di caratteri e codifica come farebbe il programma di chi scrive.
function parteTesto(sottotipo, testo, prog, html) {
  const crlf = testo.replace(/\r?\n/g, CRLF);
  let charset = 'UTF-8';
  let byte = Buffer.from(crlf, 'utf8');
  if (prog === 'outlook') {
    const w = inCp1252(crlf);
    if (w) {
      charset = 'windows-1252';
      byte = w;
    }
  }
  const righeCorte = crlf.split(CRLF).every((r) => r.length <= 76);
  const righeLecite = crlf.split(CRLF).every((r) => Buffer.byteLength(r) <= 998);
  let cte;
  let corpo;
  if (ascii(crlf) && righeCorte) {
    cte = '7bit';
    corpo = byte.toString('latin1');
  } else if (prog === 'thunderbird' && righeLecite) {
    cte = '8bit';
    corpo = byte.toString('latin1');
  } else if (prog === 'servizio' && html) {
    cte = 'base64';
    corpo = base64(byte);
  } else {
    cte = 'quoted-printable';
    corpo = qp(byte);
  }
  if (!corpo.endsWith(CRLF)) corpo += CRLF;
  if (charset === 'UTF-8' && cte === '7bit' && prog === 'outlook') charset = 'us-ascii';
  return {
    intestazioni: [contentType(`text/${sottotipo}`, [`charset=${citato(charset)}`]), `Content-Transfer-Encoding: ${cte}`],
    corpo: Buffer.from(corpo, 'latin1'),
  };
}

// ---------- contenuti degli allegati ----------

function zip(voci) {
  const locali = [];
  const centrali = [];
  let posizione = 0;
  for (const { nome, dati: d } of voci) {
    const n = Buffer.from(nome, 'utf8');
    const crc = crc32(d);
    const l = Buffer.alloc(30);
    l.writeUInt32LE(0x04034b50, 0); l.writeUInt16LE(20, 4); l.writeUInt16LE(0x0800, 6); l.writeUInt16LE(0, 8);
    l.writeUInt16LE(0, 10); l.writeUInt16LE(0x5a21, 12); l.writeUInt32LE(crc, 14);
    l.writeUInt32LE(d.length, 18); l.writeUInt32LE(d.length, 22); l.writeUInt16LE(n.length, 26); l.writeUInt16LE(0, 28);
    const c = Buffer.alloc(46);
    c.writeUInt32LE(0x02014b50, 0); c.writeUInt16LE(20, 4); c.writeUInt16LE(20, 6); c.writeUInt16LE(0x0800, 8);
    c.writeUInt16LE(0, 10); c.writeUInt16LE(0, 12); c.writeUInt16LE(0x5a21, 14); c.writeUInt32LE(crc, 16);
    c.writeUInt32LE(d.length, 20); c.writeUInt32LE(d.length, 24); c.writeUInt16LE(n.length, 28);
    c.writeUInt32LE(posizione, 42);
    locali.push(l, n, d);
    centrali.push(c, n);
    posizione += 30 + n.length + d.length;
  }
  const dimCentrale = centrali.reduce((s, b) => s + b.length, 0);
  const fine = Buffer.alloc(22);
  fine.writeUInt32LE(0x06054b50, 0); fine.writeUInt16LE(voci.length, 8); fine.writeUInt16LE(voci.length, 10);
  fine.writeUInt32LE(dimCentrale, 12); fine.writeUInt32LE(posizione, 16);
  return Buffer.concat([...locali, ...centrali, fine]);
}

const xml = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const TIPI_OFFICE = (extra) => `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>${extra}</Types>`;
const RELS = (tipo, bersaglio) => `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="${tipo}" Target="${bersaglio}"/></Relationships>`;
const DOC_UFFICIALE = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument';

function docx(testo) {
  const paragrafi = testo.split(/\r?\n/).map((r) => `<w:p><w:r><w:t xml:space="preserve">${xml(r)}</w:t></w:r></w:p>`).join('');
  return zip([
    { nome: '[Content_Types].xml', dati: Buffer.from(TIPI_OFFICE('<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>')) },
    { nome: '_rels/.rels', dati: Buffer.from(RELS(DOC_UFFICIALE, 'word/document.xml')) },
    { nome: 'word/document.xml', dati: Buffer.from(`<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${paragrafi}</w:body></w:document>`) },
  ]);
}

function xlsx(csv) {
  const colonna = (i) => String.fromCharCode(65 + i);
  const righe = csv.trim().split(/\r?\n/).map((r, i) => `<row r="${i + 1}">${r.split(';').map((v, j) =>
    `<c r="${colonna(j)}${i + 1}" t="inlineStr"><is><t>${xml(v)}</t></is></c>`).join('')}</row>`).join('');
  return zip([
    { nome: '[Content_Types].xml', dati: Buffer.from(TIPI_OFFICE('<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>')) },
    { nome: '_rels/.rels', dati: Buffer.from(RELS(DOC_UFFICIALE, 'xl/workbook.xml')) },
    { nome: 'xl/workbook.xml', dati: Buffer.from(`<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Foglio1" sheetId="1" r:id="rId1"/></sheets></workbook>`) },
    { nome: 'xl/_rels/workbook.xml.rels', dati: Buffer.from(RELS('http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet', 'worksheets/sheet1.xml')) },
    { nome: 'xl/worksheets/sheet1.xml', dati: Buffer.from(`<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>${righe}</sheetData></worksheet>`) },
  ]);
}

function pdf(titolo, righe) {
  const esc = (s) => s.replace(/[\\()]/g, (c) => '\\' + c).replace(/[^\x20-\x7e]/g, '?');
  const testo = ['BT', '/F1 16 Tf', '56 780 Td', `(${esc(titolo)}) Tj`, '/F1 11 Tf', '0 -28 Td']
    .concat(...righe.map((r) => [`(${esc(r)}) Tj`, '0 -16 Td']), ['ET']).join('\n');
  const oggetti = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${testo.length} >>\nstream\n${testo}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  let out = '%PDF-1.4\n';
  const pos = [];
  oggetti.forEach((o, i) => { pos.push(out.length); out += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const xref = out.length;
  out += `xref\n0 ${oggetti.length + 1}\n0000000000 65535 f \n` + pos.map((p) => `${pad(p, 10)} 00000 n \n`).join('');
  out += `trailer\n<< /Size ${oggetti.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}

// Un PNG vero da una delle piccole SVG sintetiche: rettangoli, cerchi e poligoni (M/L/Z)
// riempiti; le scritte si lasciano stare. Così l'app ha immagini raster da mostrare.
function colore(c) {
  if (!c) return null;
  let m = /^#([0-9a-f]{3})$/i.exec(c);
  if (m) return [...m[1]].map((x) => parseInt(x + x, 16));
  m = /^#([0-9a-f]{6})$/i.exec(c);
  if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16));
  m = /^hsl\(\s*([\d.]+)\s*,\s*([\d.]+)%\s*,\s*([\d.]+)%\s*\)$/i.exec(c);
  if (m) {
    const h = +m[1] / 360, s = +m[2] / 100, l = +m[3] / 100;
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s, p = 2 * l - q;
    const f = (t) => { t = (t + 1) % 1; return t < 1 / 6 ? p + (q - p) * 6 * t : t < 0.5 ? q : t < 2 / 3 ? p + (q - p) * (2 / 3 - t) * 6 : p; };
    return [f(h + 1 / 3), f(h), f(h - 1 / 3)].map((v) => Math.round(v * 255));
  }
  return null;
}

function pngDaSvg(svg) {
  const attr = (s, n) => { const m = new RegExp(`\\b${n}="([^"]*)"`).exec(s); return m ? m[1] : null; };
  const W = +(attr(svg, 'width') || 400), H = +(attr(svg, 'height') || 300);
  const scala = Math.min(1, 600 / Math.max(W, H));
  const w = Math.max(1, Math.round(W * scala)), h = Math.max(1, Math.round(H * scala));
  const forme = [];
  for (const m of svg.matchAll(/<(rect|circle|path)\b([^>]*)\/?>/g)) {
    const [, tipo, a] = m;
    const c = colore(attr(a, 'fill'));
    if (!c) continue;
    if (tipo === 'rect') forme.push({ tipo, c, x: +(attr(a, 'x') || 0), y: +(attr(a, 'y') || 0), w: +attr(a, 'width'), h: +attr(a, 'height') });
    else if (tipo === 'circle') forme.push({ tipo, c, x: +attr(a, 'cx'), y: +attr(a, 'cy'), r: +attr(a, 'r') });
    else {
      const punti = [...(attr(a, 'd') || '').matchAll(/(-?[\d.]+)[ ,]+(-?[\d.]+)/g)].map((p) => [+p[1], +p[2]]);
      if (punti.length > 2) forme.push({ tipo, c, punti });
    }
  }
  const dentro = (f, x, y) => {
    if (f.tipo === 'rect') return x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h;
    if (f.tipo === 'circle') return (x - f.x) ** 2 + (y - f.y) ** 2 <= f.r ** 2;
    let d = false;
    for (let i = 0, j = f.punti.length - 1; i < f.punti.length; j = i++) {
      const [xi, yi] = f.punti[i], [xj, yj] = f.punti[j];
      if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) d = !d;
    }
    return d;
  };
  const grezzo = Buffer.alloc((w * 3 + 1) * h);
  for (let py = 0; py < h; py++) {
    grezzo[py * (w * 3 + 1)] = 0;
    for (let px = 0; px < w; px++) {
      const x = (px + 0.5) / scala, y = (py + 0.5) / scala;
      let c = [255, 255, 255];
      for (const f of forme) if (dentro(f, x, y)) c = f.c;
      grezzo.set(c, py * (w * 3 + 1) + 1 + px * 3);
    }
  }
  const pezzo = (tipo, d) => {
    const l = Buffer.alloc(4); l.writeUInt32BE(d.length);
    const td = Buffer.concat([Buffer.from(tipo, 'latin1'), d]);
    const c = Buffer.alloc(4); c.writeUInt32BE(crc32(td));
    return Buffer.concat([l, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), pezzo('IHDR', ihdr),
    pezzo('IDAT', zlib.deflateSync(grezzo)), pezzo('IEND', Buffer.alloc(0))]);
}

// Il contenuto di un allegato: dal file del prototipo quando c'è, nel formato vero
// che il tipo dichiara (docx e xlsx veri dai .txt e .csv, PNG dalle SVG); altrimenti
// inventato a partire dall'anteprima.
function contenutoAllegato(a) {
  const tipo = (a.mimeType || 'application/octet-stream').toLowerCase();
  const file = a.path ? join(DATI, a.path) : null;
  const grezzo = file && existsSync(file) ? readFileSync(file) : null;
  const anteprima = a.preview || `${a.filename}\n\nContenuto sintetico creato per le prove del motore.`;
  const est = file ? extname(file).toLowerCase() : '';
  if (tipo.startsWith('image/')) {
    if (grezzo && est === '.svg') return pngDaSvg(grezzo.toString('utf8'));
    if (grezzo) return grezzo;
    return pngDaSvg('<svg width="320" height="200"><rect width="320" height="200" fill="hsl(200,40%,80%)"/></svg>');
  }
  if (tipo.includes('wordprocessingml')) return docx(grezzo ? grezzo.toString('utf8') : anteprima);
  if (tipo.includes('spreadsheetml')) return xlsx(grezzo ? grezzo.toString('utf8') : 'Voce;Valore\nEsempio;1');
  if (tipo === 'application/zip') {
    if (grezzo) return zip([{ nome: a.path.split('/').pop(), dati: grezzo }]);
    return zip([{ nome: 'LEGGIMI.txt', dati: Buffer.from(anteprima + '\n') }]);
  }
  if (tipo === 'application/pdf' && !grezzo) return pdf(a.filename, anteprima.split('\n').slice(0, 30));
  if (grezzo) return grezzo;
  return Buffer.from(anteprima + '\n', 'utf8');
}

function calendarioInventato(m) {
  const t = new Date(Date.parse(m.date || m.internalDate) + spostamento);
  const ics = (d) => d.toISOString().replace(/[-:]/g, '').replace(/\.\d+/, '');
  return ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//Ambassadress//sintetico//IT', 'METHOD:REQUEST', 'BEGIN:VEVENT',
    `UID:${m.id}@esempio.it`, `DTSTART:${ics(t)}`, `DTEND:${ics(new Date(t.getTime() + 3600e3))}`,
    `SUMMARY:${(m.subject || '').replace(/[\r\n]/g, ' ')}`, 'END:VEVENT', 'END:VCALENDAR', ''].join(CRLF);
}

// ---------- albero MIME ----------

// foglia: { intestazioni: [righe], corpo: Buffer, attesa? }; multipart: { sotto, confine, figli }
function serializza(nodo, cima = false) {
  if (!nodo.figli) return Buffer.concat([Buffer.from(nodo.intestazioni.join(CRLF) + CRLF + CRLF, 'latin1'), nodo.corpo]);
  const pezzi = [];
  if (!cima) pezzi.push(Buffer.from(contentType(`multipart/${nodo.sotto}`, [`boundary=${citato(nodo.confine)}`]) + CRLF + CRLF, 'latin1'));
  else pezzi.push(Buffer.from('This is a multi-part message in MIME format.' + CRLF + CRLF, 'latin1'));
  for (const f of nodo.figli) {
    pezzi.push(Buffer.from(`--${nodo.confine}${CRLF}`, 'latin1'), serializza(f), Buffer.from(CRLF, 'latin1'));
  }
  pezzi.push(Buffer.from(`--${nodo.confine}--${CRLF}`, 'latin1'));
  return Buffer.concat(pezzi);
}

function numera(nodo, prefisso, out) {
  if (!nodo.figli) {
    if (nodo.attesa) out.push({ ...nodo.attesa, parte: prefisso || '1' });
    return;
  }
  nodo.figli.forEach((f, i) => numera(f, prefisso ? `${prefisso}.${i + 1}` : String(i + 1), out));
}

function messaggio(m, indice) {
  const prog = programma(m);
  const stile = prog === 'outlook' ? 'B' : 'Q';
  const seme = (n) => `${m.id}|${n}`;

  const testo = typeof m.text === 'string' ? m.text : '';
  const html = typeof m.html === 'string' && m.html ? m.html : null;
  const alternative = [];
  if (testo || !html) {
    const p = parteTesto('plain', testo, prog, html);
    alternative.push({ ...p, attesa: { tipo: 'testo' } });
  }
  if (html) alternative.push({ ...parteTesto('html', html, prog, true), attesa: { tipo: 'html' } });

  const allegati = (m.attachments || []).map((a) => ({ a, dati: contenutoAllegato(a) }));
  const calendario = (m.otherParts || []).includes('text/calendar');
  if (calendario) {
    const ics = allegati.find((x) => /calendar|ics/i.test(x.a.mimeType || ''));
    const contenuto = ics ? ics.dati.toString('utf8').replace(/\r?\n/g, CRLF) : calendarioInventato(m);
    const cte = ascii(contenuto) ? '7bit' : 'quoted-printable';
    alternative.push({
      intestazioni: [contentType('text/calendar', ['charset="UTF-8"', 'method=REQUEST']), `Content-Transfer-Encoding: ${cte}`],
      corpo: Buffer.from(cte === '7bit' ? contenuto : qp(Buffer.from(contenuto, 'utf8')) + CRLF, 'latin1'),
    });
  }
  let corpo = alternative.length === 1 ? alternative[0] : { sotto: 'alternative', confine: confine(prog, seme('alt')), figli: alternative };

  const parteAllegato = ({ a, dati: d }, incorporato) => {
    const nome = a.filename || 'senza-nome';
    const tipo = (a.mimeType || 'application/octet-stream').toLowerCase();
    // Come Gmail: nel Content-Type il nome con le parole codificate RFC 2047, nella
    // disposizione con RFC 2231.
    const nomeTipo = ascii(nome) ? citato(nome) : citato(paroleCodificate(nome, 'Q').join(' '));
    const intestazioni = [contentType(tipo, [`name=${nomeTipo}`])];
    intestazioni.push(piega('Content-Disposition', [(incorporato ? 'inline' : 'attachment') + ';'].concat(
      parametroNome('filename', nome).map((p, i, v) => (i < v.length - 1 ? p + ';' : p)))));
    intestazioni.push('Content-Transfer-Encoding: base64');
    if (incorporato) intestazioni.push(`Content-ID: <${a.contentId}>`);
    if (prog === 'outlook' && !incorporato) intestazioni.push(`Content-Description: ${ascii(nome) ? nome : paroleCodificate(nome, 'B').join(' ')}`);
    return {
      intestazioni,
      corpo: Buffer.from(base64(d) + CRLF, 'latin1'),
      attesa: { tipo: incorporato ? 'incorporato' : 'allegato', byte: d.length, impronta: hex(fnv64(d)), mime: tipo, nome },
    };
  };

  const incorporati = allegati.filter((x) => x.a.inline && x.a.contentId && /^image\//i.test(x.a.mimeType || ''));
  const normali = allegati.filter((x) => !incorporati.includes(x));
  if (incorporati.length) {
    // Come Gmail: alternative[testo, related[html, immagini]]
    const imm = incorporati.map((x) => parteAllegato(x, true));
    if (corpo.figli && html) {
      const i = corpo.figli.findIndex((f) => f.attesa && f.attesa.tipo === 'html');
      corpo.figli[i] = { sotto: 'related', confine: confine(prog, seme('rel')), figli: [corpo.figli[i], ...imm] };
    } else corpo = { sotto: 'related', confine: confine(prog, seme('rel')), figli: [corpo, ...imm] };
  }
  if (normali.length) {
    corpo = { sotto: 'mixed', confine: confine(prog, seme('mix')), figli: [corpo, ...normali.map((x) => parteAllegato(x, false))] };
  }

  // Intestazioni del messaggio
  const ts = m.date ? Date.parse(m.date) + spostamento : null;
  const interna = Date.parse(m.date || m.internalDate) + spostamento;
  const righe = [];
  if (ts != null && !isNaN(ts)) righe.push(`Date: ${dataRfc(ts)}`);
  else if (m.dateRaw) righe.push(`Date: ${m.dateRaw}`);
  righe.push(intestazioneIndirizzi('From', [m.from], stile));
  if (m.subject != null) righe.push(intestazioneTesto('Subject', m.subject, stile));
  for (const [nome, lista] of [['To', m.to], ['Cc', m.cc], ['Bcc', m.bcc]]) {
    const r = intestazioneIndirizzi(nome, lista, stile);
    if (r) righe.push(r);
  }
  if (m.messageId) righe.push(`Message-ID: ${m.messageId}`);
  if (m.inReplyTo) righe.push(`In-Reply-To: ${m.inReplyTo}`);
  if (m.references && m.references.length) righe.push(piega('References', m.references));
  const strutturali = /^(from|to|cc|bcc|subject|date|message-id|in-reply-to|references|content-type|content-transfer-encoding|mime-version)$/i;
  for (const [nome, valore] of m.headers || []) {
    if (strutturali.test(nome)) continue;
    if (/^(sender|reply-to)$/i.test(nome)) righe.push(piega(nome, [valore]));
    else righe.push(intestazioneTesto(nome, valore, stile));
  }
  for (const [nome, valore] of intestazioniProgramma(prog)) {
    if (nome === 'X-MS-Has-Attach') righe.push(`X-MS-Has-Attach:${normali.length ? ' yes' : ''}`);
    else righe.push(`${nome}: ${valore}`);
  }
  righe.push('MIME-Version: 1.0');
  let byte;
  if (corpo.figli) {
    righe.push(contentType(`multipart/${corpo.sotto}`, [`boundary=${citato(corpo.confine)}`]));
    byte = Buffer.concat([Buffer.from(righe.join(CRLF) + CRLF + CRLF, 'latin1'), serializza(corpo, true)]);
  } else {
    byte = Buffer.concat([Buffer.from(righe.concat(corpo.intestazioni).join(CRLF) + CRLF + CRLF, 'latin1'), corpo.corpo]);
  }

  const parti = [];
  numera(corpo, '', parti);

  // Identità di Gmail: X-GM-THRID dal threadId (esadecimale come quelli veri, oppure
  // un'impronta), X-GM-MSGID crescente con l'UID.
  const tid = (m.gmail && m.gmail.threadId) || m.id;
  const thrid = /^[0-9a-f]{16}$/i.test(tid) ? BigInt('0x' + tid) : fnv64(Buffer.from(tid)) & 0x7fffffffffffffffn;
  const uid = indice + 1;
  const msgid = 0x1900000000000000n + BigInt(uid) * 0x100000n + (fnv64(Buffer.from(m.id)) & 0xfffffn);

  const meta = [`id: ${m.id}`, `uid: ${uid}`, `x-gm-msgid: ${msgid}`, `x-gm-thrid: ${thrid}`, `internaldate: ${Math.floor(interna / 1000)}`];
  if (m.flags && m.flags.seen) meta.push('flag: \\Seen');
  if (m.flags && m.flags.flagged) meta.push('flag: \\Flagged');
  for (const e of (m.gmail && m.gmail.labels) || []) meta.push(`etichetta: ${e}`);
  const normalizza = (s) => Buffer.from(s.replace(/\r\n/g, '\n').replace(/\s+$/, ''), 'utf8');
  if (testo.trim()) { const b = normalizza(testo); meta.push(`testo: ${b.length} ${hex(fnv64(b))}`); }
  if (html) { const b = normalizza(html); meta.push(`html: ${b.length} ${hex(fnv64(b))}`); }
  for (const p of parti.filter((x) => x.tipo === 'allegato' || x.tipo === 'incorporato')) {
    meta.push(`${p.tipo}: ${p.parte} ${p.byte} ${p.impronta} ${p.mime} ${p.nome}`);
  }
  return { byte, meta: meta.join('\n') + '\n' };
}

// ---------- scrittura ----------

mkdirSync(uscita, { recursive: true });
for (const f of readdirSync(uscita)) {
  if (f.endsWith('.eml') || f.endsWith('.meta')) rmSync(join(uscita, f));
}
const ordinati = dati.messages.slice().sort((a, b) =>
  (Date.parse(a.date || a.internalDate) || 0) - (Date.parse(b.date || b.internalDate) || 0));
let byteTotali = 0;
let allegati = 0;
ordinati.forEach((m, i) => {
  const { byte, meta } = messaggio(m, i);
  const nome = pad(i + 1, 4);
  writeFileSync(join(uscita, `${nome}.eml`), byte);
  writeFileSync(join(uscita, `${nome}.meta`), meta);
  byteTotali += byte.length;
  allegati += (meta.match(/^allegato: /gm) || []).length;
});
const a = dati.account || {};
writeFileSync(join(uscita, 'account.meta'),
  [`indirizzo: ${a.address || ''}`, `nome: ${a.name || ''}`].concat((a.aliases || []).map((x) => `alias: ${x}`)).join('\n') + '\n');
const giorni = Math.round(spostamento / 86400e3);
console.log(`${ordinati.length} messaggi in ${uscita} (${(byteTotali / 1024).toFixed(0)} KB, ${allegati} allegati); ` +
  `date spostate di ${giorni} giorni`);
