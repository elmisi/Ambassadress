// Generates synthetic.js: a made-up mailbox in the same format as tools/gmail-export.py.
// It is the only dataset the canvas and development ever use.
// Bug reproductions go in cases/*.json and are merged in here.
//
//   node mockups/data/generate-synthetic.mjs

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as T from './testi-lunghi.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const NOW = new Date('2026-10-14T12:00:00+02:00');
const ME = { name: 'Alex Moro', address: 'alex.moro@esempio.it' };

function rng(seed) {
  let a = seed;
  return () => {
    a |= 0; a = a + 0x6D2B79F5 | 0;
    let t = Math.imul(a ^ a >>> 15, 1 | a);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}
const rnd = rng(20261014);
const pick = (arr) => arr[Math.floor(rnd() * arr.length)];

const at = (daysAgo, h, m) => {
  const d = new Date(NOW);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, m, 0, 0);
  return d;
};
const MONTHS = ['gen', 'feb', 'mar', 'apr', 'mag', 'giu', 'lug', 'ago', 'set', 'ott', 'nov', 'dic'];
const DAYS = ['dom', 'lun', 'mar', 'mer', 'gio', 'ven', 'sab'];
const itQuoteDate = (d) => `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()} alle ore ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
const rfcDate = (d) => d.toUTCString().replace('GMT', '+0000');
const fmtAddr = (p) => (p.name ? `"${p.name}" <${p.address}>` : p.address);

const MIME = { PDF: 'application/pdf', JPG: 'image/jpeg', DOC: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', XLS: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', ZIP: 'application/zip', PNG: 'image/png', GIF: 'image/gif' };
const bytes = (s) => {
  if (!s) return 20000;
  const m = /([\d,]+)\s*(KB|MB)/.exec(s);
  if (!m) return 20000;
  const n = parseFloat(m[1].replace(',', '.'));
  return Math.round(n * (m[2] === 'MB' ? 1024 * 1024 : 1024));
};

let seq = 0;
const messages = [];

// How each sender's mail program writes a reply: the header above the quote, how the quote is
// marked, and where the signature goes. Separate random stream, so the rest of the data stays put.
const rq = rng(20261027);
const pq = (arr) => arr[Math.floor(rq() * arr.length)];
const CLIENTS = ['gmail-it', 'gmail-it', 'gmail-it', 'gmail-en', 'outlook', 'outlook', 'outlook-en', 'apple', 'iphone', 'iphone', 'thunderbird'];
const clientOf = (p) => p.client || (p.client = pq(CLIENTS));
const DAYS_LONG = ['domenica', 'lunedì', 'martedì', 'mercoledì', 'giovedì', 'venerdì', 'sabato'];
const MONTHS_LONG = ['gennaio', 'febbraio', 'marzo', 'aprile', 'maggio', 'giugno', 'luglio', 'agosto', 'settembre', 'ottobre', 'novembre', 'dicembre'];
const EN_DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const EN_DAYS_LONG = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const EN_MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const EN_MONTHS_LONG = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const hm = (d) => `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
const ampm = (d) => `${(d.getHours() % 12) || 12}:${String(d.getMinutes()).padStart(2, '0')} ${d.getHours() < 12 ? 'AM' : 'PM'}`;
const nameAddr = (p) => (p.name ? `${p.name} <${p.address}>` : p.address);
const names = (list) => list.map((p) => p.name || p.address).join('; ');
// Gmail wraps a long header line, sometimes right inside the address
const wrap = (line) => (line.length > 78 && rq() < 0.5 ? line.replace(/ <([^>]+)>/, ' <\n$1>') : line);
const QUOTE_CAP = 16000;
function quoteBlock(client, prev) {
  let raw = prev.text || '';
  if (raw.length > QUOTE_CAP) raw = raw.slice(0, raw.lastIndexOf('\n', QUOTE_CAP));
  const d = new Date(prev.date);
  const who = nameAddr(prev.from);
  const gmailMark = (t) => t.split('\n').map((l) => (l.trim() ? (l.startsWith('>') ? '>' + l : '> ' + l) : '>')).join('\n');
  switch (client) {
    case 'gmail-en':
      return wrap(`On ${EN_DAYS[d.getDay()]}, ${EN_MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()} at ${ampm(d)} ${who} wrote:`) + '\n' + gmailMark(raw);
    case 'outlook':
      return '________________________________\n' + `Da: ${who}\nInviato: ${DAYS_LONG[d.getDay()]} ${d.getDate()} ${MONTHS_LONG[d.getMonth()]} ${d.getFullYear()} ${hm(d)}\nA: ${names(prev.to)}\n` + (prev.cc.length ? `Cc: ${names(prev.cc)}\n` : '') + `Oggetto: ${prev.subject}\n\n` + raw;
    case 'outlook-en':
      return '-----Original Message-----\n' + `From: ${who}\nSent: ${EN_DAYS_LONG[d.getDay()]}, ${EN_MONTHS_LONG[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()} ${ampm(d)}\nTo: ${names(prev.to)}\n` + (prev.cc.length ? `Cc: ${names(prev.cc)}\n` : '') + `Subject: ${prev.subject}\n\n` + raw;
    case 'apple':
      return `Il giorno ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}, alle ore ${hm(d)}, ${who} ha scritto:\n\n` + raw.split('\n').map((l) => (l.startsWith('>') ? '>' + l : '> ' + l)).join('\n');
    case 'iphone':
      return raw.split('\n').map((l) => (l.startsWith('>') ? '>' + l : '> ' + l)).join('\n').replace(/^/, `> Il giorno ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}, alle ore ${hm(d)}, ${who} ha scritto:\n> \n`);
    case 'thunderbird':
      // righe vuote lasciate vuote: la citazione si interrompe
      return `Il ${String(d.getDate()).padStart(2, '0')}/${String(d.getMonth() + 1).padStart(2, '0')}/${d.getFullYear()} ${hm(d)}, ${prev.from.name || prev.from.address} ha scritto:\n` + raw.split('\n').map((l) => (l.trim() ? '>' + (l.startsWith('>') ? '' : ' ') + l : '')).join('\n');
    default:
      return wrap(`Il giorno ${itQuoteDate(d)} ${who} ha scritto:`) + '\n' + gmailMark(raw);
  }
}
// Signatures as people write them: with or without "-- ", phone signatures, long company ones.
function signatureOf(p, client) {
  if (client === 'iphone') return p.signature && rq() < 0.3 ? '\n\n' + p.signature + '\n\nInviato da iPhone' : '\n\nInviato da iPhone';
  if (!p.signature) return '';
  return p.sigDash === false ? '\n\n' + p.signature : '\n\n-- \n' + p.signature;
}
// A reply written inside the quote: each answer under the paragraph it answers.
function inlineReply(prev, intro, answers, client) {
  const d = new Date(prev.date);
  const paras = prev.textMain.split(/\n\s*\n/);
  const out = [intro, '', wrap(`Il giorno ${itQuoteDate(d)} ${nameAddr(prev.from)} ha scritto:`)];
  paras.forEach((para, i) => {
    out.push(para.split('\n').map((l) => '> ' + l).join('\n'));
    if (answers[i]) out.push('', answers[i], '');
    else out.push('>');
  });
  return out.join('\n').replace(/\n>\n*$/, '');
}

// One conversation = one Gmail thread between ME and `who` about `topic`.
// entries: { d, h, m, me, text, att: [[name, ext, size]], cc: [person], quote: bool, fwd: bool }
function thread(who, topic, entries, opts = {}) {
  const threadId = (0x18f0000000000000n + BigInt(++seq * 7919)).toString(16);
  let prev = null;
  const refs = [];
  entries.forEach((e, i) => {
    const date = e.date || at(e.d, e.h ?? 10, e.m ?? 0);
    const me = opts.me || ME;
    const from = e.from || (e.me ? me : who);
    const to = e.to || (e.me ? [who] : [me]);
    const subject = e.subject ?? (i === 0 ? topic : (opts.prefix || 'Re: ') + topic);
    const messageId = `<syn-${++seq}@esempio.it>`;
    const client = e.client || (e.outlook ? 'outlook' : clientOf(from));
    let text = e.text;
    if (prev && e.inline) text = inlineReply(prev, e.text, e.inline, client);
    if (e.signature !== false && text) text += signatureOf(from, client);
    if (prev && !e.inline && (e.quote ?? rq() < 0.85)) text += '\n\n' + quoteBlock(client, prev);
    const labels = e.labels || (from === me ? ['\\Sent'] : ['\\Inbox']);
    const rec = {
      id: 'syn-' + seq,
      date: date.toISOString(),
      dateRaw: rfcDate(date),
      from: from,
      to: to,
      cc: e.cc || [],
      subject: subject,
      messageId: messageId,
      inReplyTo: prev ? prev.messageId : null,
      references: refs.slice(),
      text: e.html && !e.text ? null : text,
      html: e.html || null,
      attachments: (e.att || []).map(([name, ext, size]) => ({ filename: name, mimeType: MIME[ext] || 'application/octet-stream', size: bytes(size), inline: false })),
      headers: [['From', fmtAddr(from)], ['To', to.map(fmtAddr).join(', ')], ['Subject', subject], ['Date', rfcDate(date)]].concat(e.headers || []),
      flags: { seen: e.unread ? false : true, flagged: false },
      gmail: { threadId: threadId, labels: labels },
      problems: e.problems || []
    };
    rec.textMain = e.text;
    messages.push(rec);
    refs.push(messageId);
    prev = rec;
  });
}

const P = (name, address, signature, more) => Object.assign({ name, address, signature }, more || {});
ME.signature = 'Alex Moro';
ME.client = 'gmail-it';
const DISCLAIMER_IT = 'Le informazioni contenute in questo messaggio di posta elettronica e nei suoi allegati sono riservate e destinate esclusivamente alla persona o alla società indicata come destinatario. Qualsiasi utilizzo, diffusione o copia non autorizzati sono vietati. Se avete ricevuto questo messaggio per errore, vi preghiamo di avvisare immediatamente il mittente e di cancellarlo dal vostro sistema.';
const DISCLAIMER_EN = 'This e-mail and any attachments are confidential and intended solely for the addressee. If you have received this message in error, please notify the sender immediately and delete it. Any unauthorised use, disclosure or copying is strictly prohibited.';
const lucia = P('Lucia Ferri', 'lucia.ferri@esempio.it', '', { client: 'iphone' });
const marco = P('Marco Bassi', 'marco.bassi@esempio.it', 'Marco', { client: 'gmail-it', sigDash: false });
const elena = P('Arch. Elena Sartori', 'elena.sartori@studiosartori.esempio.it', 'Arch. Elena Sartori\nStudio Sartori architetti\nVia dei Volsci 41, 00185 Roma\ntel. 06 5550 1234 · cell. 333 555 0148\nwww.studiosartori.esempio.it', { client: 'outlook', sigDash: false });
const anna = P('Anna Ricci', 'anna.ricci@scuolacollodi.esempio.it', 'Maestra Anna Ricci\nScuola primaria Collodi, classe 3B', { client: 'apple' });
const martini = P('Studio Martini', 'segreteria@studiomartini.esempio.it', 'Segreteria\nStudio Martini · Commercialisti associati\nViale Marconi 210, 00146 Roma\nTel. 06 5550 7788 · Fax 06 5550 7789\nP.IVA 01234567890\n\n' + DISCLAIMER_IT, { client: 'outlook', sigDash: false });
const admin = P('Amministrazione Condominio Via Ostiense 12', 'amministrazione@condominio.esempio.it', 'Studio Amministrazioni Ferretti\nRag. Federica Monti\nVia Ostiense 98, 00154 Roma\nTel. 06 5550 3311 (lun-ven 9-13)\nPEC: studioferretti@pec.esempio.it\n\nAVVISO DI RISERVATEZZA: ' + DISCLAIMER_IT, { client: 'outlook', sigDash: false });
const paolo = P('Paolo Conti', 'paolo.conti@esempio.it', '', { client: 'iphone' });
const paoloWork = P('Conti Paolo', 'p.conti@azienda.esempio.it', 'Paolo Conti | Ufficio acquisti\nAzienda Esempio S.p.A.\nVia Tiburtina 1180, 00156 Roma\nT +39 06 5550 2000 | M +39 347 555 0911\n\n' + DISCLAIMER_EN, { client: 'outlook-en', sigDash: false });
const giulia = P('Giulia Rinaldi', 'giulia@cucinerinaldi.esempio.it', 'Giulia Rinaldi\nCucine Rinaldi – progettazione e arredo\nShowroom: Via Appia Nuova 512, Roma\n06 5550 6060', { client: 'gmail-it' });
const davide = P('Davide Greco', 'davide.greco@lavoro.esempio.it', 'Davide Greco\nResponsabile amministrazione e controllo\nLavoro Esempio S.r.l.\nT. 06 5550 4410 – M. 335 555 0172', { client: 'outlook', sigDash: false });
const sara = P('Sara Neri', 'sara.neri@lavoro.esempio.it', 'Sara Neri\nProject manager | Lavoro Esempio S.r.l.\n+39 06 5550 4422', { client: 'gmail-en' });
const luca = P('Luca Fontana', 'luca.fontana@lavoro.esempio.it', 'Luca', { client: 'thunderbird', sigDash: false });
const ASM = (() => { const d = at(-4, 18, 0); return `${['domenica', 'lunedì', 'martedì', 'mercoledì', 'giovedì', 'venerdì', 'sabato'][d.getDay()]} ${d.getDate()} ottobre`; })();

thread(lucia, 'Cena di fine estate', [
  { d: 48, h: 19, m: 10, text: 'Allora sabato da noi? Il vino lo porto io.' },
  { d: 48, h: 19, m: 32, me: true, text: 'Perfetto, a che ora?' },
  { d: 47, h: 8, m: 15, text: 'Alle 20. Ti allego il menù.', att: [['Menu cena.jpg', 'JPG', '1,2 MB']] }
]);
thread(admin, 'Assemblea ordinaria', [
  { d: 13, h: 16, m: 5, to: [{ name: 'Condomini Via Ostiense 12', address: 'condomini@condominio.esempio.it' }], text: `Gentili condomini, è convocata l’assemblea ordinaria per ${ASM} alle 18:00 presso la sala comune, in seconda convocazione.\n\nOrdine del giorno:\n1) approvazione del rendiconto 2025;\n2) preventivo di gestione 2026;\n3) rifacimento della facciata, scelta della ditta tra le tre proposte ricevute;\n4) regolamento per l’uso del cortile e delle biciclette;\n5) varie ed eventuali.\n\nChi non potesse partecipare può delegare un altro condomino compilando il modulo allegato. Ogni condomino può rappresentare al massimo due deleghe.`, att: [['Convocazione assemblea.pdf', 'PDF', '520 KB'], ['Modulo delega.docx', 'DOC', '45 KB']], quote: false },
  { d: 2, h: 9, m: 0, to: [{ name: 'Condomini Via Ostiense 12', address: 'condomini@condominio.esempio.it' }], text: `Promemoria: l’assemblea è ${ASM} alle 18:00. Portate le deleghe firmate.`, quote: false }
]);
thread(lucia, 'Assemblea ordinaria', [
  { d: 11, h: 21, m: 4, cc: [admin, marco], text: 'Io quel giorno sono fuori. Il modulo della delega dove lo trovo?' },
  { d: 10, h: 7, m: 50, me: true, cc: [admin, marco], text: 'È allegato alla convocazione, l’ultima pagina.' },
  { d: 0, h: 9, m: 42, cc: [admin, marco], text: 'Grazie! Allego la delega firmata, la porta Marco.', att: [['Delega Ferri firmata.pdf', 'PDF', '240 KB']], unread: true }
]);
thread(marco, 'Assemblea ordinaria', [
  { d: 11, h: 20, m: 12, cc: [lucia], text: 'Io ci sono, posso portare anche una delega.' },
  { d: 10, h: 7, m: 52, me: true, cc: [lucia], text: 'Grazie, porta quella di Lucia.' },
  { d: 0, h: 8, m: 55, cc: [lucia], text: `Ricevuta la delega di Lucia, la porto io ${ASM}.`, unread: true }
]);

// The 50-message thread.
const E = [
  [0, 'Sopralluogo', 'Buongiorno, sono Elena Sartori, mi ha dato il suo contatto Giulia Rinaldi. Possiamo fissare un sopralluogo per il bagno?'],
  [1, 'Sopralluogo', 'Buongiorno! Volentieri. Martedì pomeriggio le va bene?'],
  [0, 'Sopralluogo', 'Martedì alle 15 perfetto. Mi servirebbe la planimetria, se ce l’ha.'],
  [1, 'Sopralluogo', 'Eccola, è quella del rogito.', [['Planimetria appartamento.pdf', 'PDF', '880 KB']]],
  [0, 'Sopralluogo', 'Grazie, a martedì.'],
  [0, 'Sopralluogo', 'Sopralluogo fatto, le mando a giorni una prima proposta. Intanto ci pensi: doccia o vasca?'],
  [1, 'Progetto bagno', 'Abbiamo deciso: doccia grande, niente vasca.'],
  [0, 'Progetto bagno', 'Ottimo. Ecco la prima proposta, con doccia a filo pavimento.', [['Progetto bagno v1.pdf', 'PDF', '2,4 MB']]],
  [1, 'Progetto bagno', 'Bella! Si può spostare il lavabo sotto la finestra?'],
  [0, 'Progetto bagno', 'Si può fare, ma le spiego le conseguenze. Spostare il lavabo sotto la finestra vuol dire portare lo scarico sulla parete opposta rispetto alla colonna attuale: servono circa tre metri di tubazione con la giusta pendenza, quindi il massetto va rialzato di qualche centimetro, oppure bisogna passare nel controsoffitto del vicino, cosa che sconsiglio.\n\nIn alternativa possiamo lasciare lo scarico dov’è e spostare solo lo specchio, con quasi lo stesso effetto di luce. Le preparo entrambe le varianti così le confrontiamo con calma, anche sui costi, che nel primo caso salgono di circa il 15%.'],
  [1, 'Progetto bagno', 'Grazie della spiegazione, vediamo le due varianti.'],
  [0, 'Progetto bagno', 'Ecco la versione 2 con le due varianti, A e B.', [['Progetto bagno v2.pdf', 'PDF', '2,7 MB']]],
  [1, 'Progetto bagno', 'Preferiamo la B.'],
  [0, 'Progetto bagno', 'Bene. Le faccio sapere i costi della B.'],
  [0, 'Progetto bagno', 'Ho aggiornato la B con le misure del sopralluogo.', [['Progetto bagno v3.pdf', 'PDF', '2,8 MB']]],
  [1, 'Progetto bagno', 'Perfetto, approviamo questa.'],
  [0, 'Progetto bagno', 'Allora procedo con il computo metrico.'],
  [1, 'Progetto bagno', 'Grazie!'],
  [0, 'Computo metrico', 'Ecco il computo metrico, prima stesura.', [['Computo metrico v1.xlsx', 'XLS', '96 KB']]],
  [1, 'Computo metrico', 'La voce dei sanitari mi sembra alta, è possibile?'],
  [0, 'Computo metrico', 'Ho messo una linea di fascia alta, posso proporne una media.'],
  [1, 'Computo metrico', 'Sì, meglio la media.'],
  [0, 'Computo metrico', 'Computo aggiornato con sanitari di fascia media.', [['Computo metrico v2.xlsx', 'XLS', '98 KB']]],
  [1, 'Computo metrico', 'Molto meglio. La manodopera è compresa?'],
  [0, 'Computo metrico', 'Sì, tutto compreso tranne lo smaltimento delle macerie, che è a parte.'],
  [0, 'Computo metrico', 'Versione definitiva con lo smaltimento incluso.', [['Computo metrico v3.xlsx', 'XLS', '101 KB']]],
  [0, 'Piastrelle', 'Passiamo alle piastrelle: le mando tre proposte.', [['Piastrelle, 3 proposte.jpg', 'JPG', '4,1 MB']]],
  [1, 'Piastrelle', 'Ci piace la seconda, quella grigio chiaro.'],
  [0, 'Piastrelle', 'Ottima scelta. Formato 60x120 o 30x60?'],
  [1, 'Piastrelle', 'Cosa consiglia?'],
  [0, 'Piastrelle', 'Per un bagno di queste dimensioni il 60x120: meno fughe, più facile da pulire.'],
  [1, 'Piastrelle', 'Allora 60x120.'],
  [0, 'Piastrelle', 'Ho chiesto la disponibilità: arrivano in tre settimane.'],
  [1, 'Piastrelle', 'Va bene, non abbiamo fretta.'],
  [0, 'Piastrelle', 'Le mando le foto del campione in luce naturale.', [['Campione grigio, foto.jpg', 'JPG', '3,3 MB']]],
  [1, 'Piastrelle', 'Dal vivo è ancora meglio!'],
  [0, 'Inizio lavori', 'L’impresa può iniziare lunedì. Le va bene?'],
  [1, 'Inizio lavori', 'Sì, lasciamo le chiavi in portineria.'],
  [0, 'Inizio lavori', 'Perfetto. Il primo giorno è demolizione: ci sarà rumore dalle 8 alle 13.'],
  [1, 'Inizio lavori', 'Avviso i vicini.'],
  [0, 'Inizio lavori', 'Ecco il cronoprogramma dei lavori.', [['Cronoprogramma lavori.pdf', 'PDF', '310 KB']]],
  [1, 'Inizio lavori', 'Tre settimane in tutto, giusto?'],
  [0, 'Inizio lavori', 'Sì, salvo imprevisti.'],
  [0, 'Inizio lavori', 'Demolizione finita, tutto secondo i piani.', [['Cantiere giorno 1.jpg', 'JPG', '2,2 MB']]],
  [0, 'Imprevisti in cantiere', 'Le scrivo per un imprevisto. Togliendo il vecchio rivestimento abbiamo trovato la tubazione dell’acqua calda in ferro zincato, molto corrosa in due punti.\n\nNon perde, ma chiudere tutto senza sostituirla sarebbe un errore: tra qualche anno rischieremmo di dover rompere le piastrelle nuove. La sostituzione costa circa 650 euro più IVA e allunga i lavori di due giorni. Le allego le foto dei due punti e la variante al computo, così può decidere con calma, ma entro venerdì se possibile.', [['Tubo corroso, foto.jpg', 'JPG', '1,8 MB'], ['Variante computo.pdf', 'PDF', '140 KB']]],
  [1, 'Imprevisti in cantiere', 'Grazie per avercelo detto subito. Procediamo con la sostituzione.'],
  [0, 'Imprevisti in cantiere', 'Bene, l’idraulico viene domani mattina.'],
  [1, 'Imprevisti in cantiere', 'Ok, ci sarà mia moglie ad aprire.'],
  [0, 'Imprevisti in cantiere', 'Tubazione sostituita. Si riparte con i massetti.'],
  [0, 'Imprevisti in cantiere', 'Domani passo a controllare le pendenze della doccia. Se vuole esserci, sono lì alle 18.']
];
const byTopic = {};
E.forEach((e, i) => {
  const d = Math.round(112 - i * 111 / (E.length - 1));
  const last = i === E.length - 1;
  (byTopic[e[1]] = byTopic[e[1]] || []).push({ d: last ? 1 : d, h: last ? 18 : 8 + (i * 7) % 12, m: last ? 20 : (i * 13) % 60, me: !!e[0], text: e[2], att: e[3], unread: i >= E.length - 2 });
});
Object.keys(byTopic).forEach((t) => thread(elena, t, byTopic[t]));

thread(anna, 'Uscita al museo', [
  { d: 9, h: 13, m: 30, to: [{ name: 'Genitori 3B', address: 'genitori3b@scuolacollodi.esempio.it' }], text: 'Gentili genitori, giovedì prossimo la classe visiterà il museo di scienze naturali. In allegato la circolare e l’autorizzazione da riconsegnare firmata entro lunedì.', att: [['Circolare uscita museo.pdf', 'PDF', '180 KB'], ['Autorizzazione uscita.pdf', 'PDF', '90 KB']] },
  { d: 8, h: 21, m: 0, me: true, text: 'Ecco l’autorizzazione firmata. Grazie!', att: [['Autorizzazione uscita firmata.pdf', 'PDF', '230 KB']] }
]);
thread(anna, 'Colloqui individuali', [
  { d: 1, h: 16, m: 45, text: 'I colloqui individuali si terranno martedì e mercoledì dalle 16:30 alle 18:30. Può indicarmi una fascia oraria che preferisce?', unread: true }
]);
thread(martini, 'Dichiarazione dei redditi', [
  { d: 25, h: 16, m: 0, me: true, text: 'Vi mando la certificazione unica.', att: [['Certificazione unica 2026.pdf', 'PDF', '420 KB']] },
  { d: 20, h: 11, m: 30, text: 'Abbiamo ricevuto la documentazione. Dall’esame risulta che mancano ancora: le spese mediche del secondo semestre, la ricevuta del pagamento dell’asilo nido e il contratto di locazione registrato.\n\nUna volta ricevuti prepareremo il modello 730 e ve lo invieremo per la firma. Vi ricordiamo inoltre che il saldo IMU va versato entro il 16 dicembre: in allegato trovate il modello F24 già compilato con gli importi calcolati sulle rendite catastali in nostro possesso. Se nel frattempo ci sono state variazioni, segnalatecele.', att: [['F24 saldo IMU.pdf', 'PDF', '160 KB']] },
  { d: 19, h: 22, m: 10, me: true, text: 'Vi mando le spese mediche e la ricevuta dell’asilo. Il contratto lo cerco nel weekend.', att: [['Spese mediche 2 semestre.pdf', 'PDF', '1,1 MB'], ['Ricevuta asilo nido.pdf', 'PDF', '200 KB']] },
  { d: 2, h: 11, m: 5, text: 'Ricevuto tutto, vi aggiorno a fine mese.' }
]);
thread(martini, 'Scadenze di novembre', [
  { d: 2, h: 11, m: 7, text: 'Vi ricordiamo le scadenze di novembre, in allegato il riepilogo.', att: [['Scadenze novembre.pdf', 'PDF', '75 KB']] }
]);
thread(admin, 'Rendiconto 2025', [
  { d: 70, h: 10, m: 0, text: 'In allegato il rendiconto consuntivo 2025, che sarà portato in approvazione alla prossima assemblea.', att: [['Rendiconto 2025.xlsx', 'XLS', '210 KB']] }
]);
thread(admin, 'Lavori in facciata', [
  { d: 40, h: 9, m: 0, text: 'Trasmettiamo i tre preventivi per il rifacimento della facciata.', att: [['Preventivi facciata, 3 ditte.pdf', 'PDF', '3,4 MB']] }
]);
thread(paolo, 'Regalo per la mamma', [
  { d: 30, h: 20, m: 0, text: 'Per il compleanno della mamma pensavo a un weekend alle terme. Facciamo a metà?' },
  { d: 30, h: 21, m: 15, me: true, text: 'Ottima idea, ci sto.' }
]);
thread(paolo, 'Foto del lago', [
  { d: 8, h: 22, m: 10, text: 'Ecco le foto del weekend.', att: [['Lago, 24 foto.zip', 'ZIP', '86 MB']] },
  { d: 8, h: 22, m: 31, me: true, text: 'Bellissime, quella col pontile è da stampare.' }
]);
// Same person from a second address: a real-life mess the prototype must deal with.
thread(paoloWork, 'Pranzo di domenica', [
  { d: 3, h: 12, m: 45, cc: [{ name: 'Famiglia Conti', address: 'famiglia.conti@esempio.it' }], text: 'Scrivo dall’ufficio: porto io il dolce.' },
  { d: 2, h: 13, m: 0, me: true, cc: [{ name: 'Famiglia Conti', address: 'famiglia.conti@esempio.it' }], text: 'E io il vino. A che ora?' }
]);
thread(giulia, 'Preventivo cucina', [
  { d: 26, h: 10, m: 2, text: 'Ecco la prima proposta con la pianta.', att: [['Preventivo cucina v1.pdf', 'PDF', '640 KB'], ['Pianta cucina.jpg', 'JPG', '900 KB']] },
  { d: 25, h: 18, m: 40, me: true, text: 'Bella! Si può togliere il piano in quarzo?' },
  { d: 14, h: 11, m: 15, text: 'Fatto, nuova versione senza quarzo.', att: [['Preventivo cucina v2.pdf', 'PDF', '650 KB']] },
  { d: 5, h: 9, m: 30, me: true, text: 'Ok. Mancano solo le misure del lato finestra.' },
  { d: 3, h: 18, m: 20, text: 'Misure prese, ecco la versione finale.', att: [['Preventivo cucina definitivo.pdf', 'PDF', '660 KB']] }
]);
thread(davide, 'Report trimestrale', [
  { d: 16, h: 9, m: 10, text: 'Ti giro la bozza del report del terzo trimestre, mi dai un’occhiata ai numeri della seconda tabella?', att: [['Report Q3 v1.xlsx', 'XLS', '340 KB']] },
  { d: 15, h: 17, m: 45, me: true, text: 'Visto. Nella seconda tabella i totali di agosto non tornano: mancano le vendite online, che nel file sorgente stanno in un foglio separato. Ho corretto e aggiunto una riga di controllo in fondo, così se succede di nuovo si vede subito. Ti rimando il file.', att: [['Report Q3 v2.xlsx', 'XLS', '352 KB']] },
  { d: 15, h: 18, m: 2, text: 'Grazie, mi hai salvato la presentazione.' }
]);
// Outlook-style quoting and Italian "R:" prefixes.
thread(davide, 'Riunione di lunedì', [
  { d: 5, h: 8, m: 30, cc: [sara, luca], text: 'Spostiamo la riunione alle 11? Alle 9 ho il dentista.' },
  { d: 5, h: 8, m: 41, me: true, cc: [sara, luca], text: 'Per me va bene.', quote: true, outlook: true },
  { d: 5, h: 9, m: 2, from: sara, to: [davide, ME], cc: [luca], text: 'Anche per me, prenoto la sala grande.', quote: true, outlook: true }
], { prefix: 'R: ' });

// A forward.
thread(P('Francesca Galli', 'francesca.galli@esempio.it'), 'Biglietti concerto', [
  { d: 12, h: 18, m: 0, subject: 'Fwd: Biglietti concerto', text: 'Ti giro i biglietti, il tuo è il secondo.\n\n---------- Forwarded message ---------\nDa: Biglietteria Teatro <noreply@teatro.esempio.it>\nDate: ven 2 ott 2026\nSubject: Biglietti concerto\nTo: <francesca.galli@esempio.it>\n\nGrazie per l’acquisto. In allegato i tuoi biglietti per sabato 24 ottobre, ore 21.', att: [['Biglietti concerto.pdf', 'PDF', '210 KB']], quote: false }
]);

// A mass email from a parents' list with many recipients.
const many = Array.from({ length: 26 }, (_, i) => ({ name: '', address: `genitore${i + 1}@esempio.it` }));
thread(P('Associazione Genitori Collodi', 'associazione.genitori@esempio.it'), 'Festa di fine anno: chi porta cosa', [
  { d: 6, h: 20, m: 30, to: many.concat([ME]), text: 'Ciao a tutti, per la festa abbiamo bisogno di: 6 torte salate, 4 dolci, bibite, piatti e bicchieri. Rispondete a tutti con cosa portate, così non ci sovrapponiamo.' },
  { d: 6, h: 21, m: 2, from: { name: '', address: 'genitore7@esempio.it' }, to: many.concat([ME]), text: 'Io porto una torta salata.', quote: true },
  { d: 6, h: 21, m: 40, me: true, to: many, text: 'Noi i bicchieri e due bottiglie di aranciata.', quote: true }
]);

// Sender without a display name, empty subject, attachment only.
thread(P('', 'm.rossi1987@esempio.it'), '', [
  { d: 15, h: 7, m: 12, subject: '', text: '', att: [['IMG_20261001_0712.jpg', 'JPG', '3,9 MB']] }
]);
// Very long subject.
thread(P('Ufficio Tributi Comune', 'tributi@comune.esempio.it'), 'Comunicazione relativa alla verifica della posizione contributiva TARI per gli anni 2023, 2024 e 2025 e richiesta di integrazione documentale entro trenta giorni', [
  { d: 21, h: 11, m: 0, text: 'Con la presente si comunica che dalla verifica della posizione risultano da integrare i documenti indicati nell’allegato. Si prega di provvedere entro trenta giorni dalla ricezione.', att: [['Richiesta integrazione.pdf', 'PDF', '300 KB']] }
]);
// Names with non-Latin script and emoji.
thread(P('Kenji 田中', 'kenji.tanaka@esempio.jp'), 'Visita a Roma', [
  { d: 35, h: 6, m: 30, text: 'Ciao! Arrivo a Roma il 3 novembre, ci vediamo per un caffè? 😊' },
  { d: 35, h: 9, m: 0, me: true, text: 'Certo! Ti scrivo il posto più avanti.' }
]);

// Generated people, for volume.
const FIRST = ['Sara', 'Luca', 'Chiara', 'Andrea', 'Francesca', 'Matteo', 'Valentina', 'Simone', 'Martina', 'Alessio', 'Federica', 'Giorgio', 'Elisa', 'Stefano', 'Silvia', 'Roberto', 'Laura', 'Nicola', 'Ilaria', 'Fabio', 'Monica', 'Tommaso', 'Serena', 'Daniele', 'Beatrice', 'Riccardo', 'Camilla', 'Emanuele'];
const LAST = ['Moretti', 'Barbieri', 'Lombardi', 'Mariani', 'Rizzo', 'Colombo', 'Santoro', 'Marino', 'Gentile', 'Caruso', 'Ferrara', 'Testa', 'Vitale', 'Serra', 'Pellegrini', 'Leone', 'Longo', 'Martinelli', 'Bianchi', 'Costa', 'Giordano', 'Ferraro', 'Benedetti', 'Grasso', 'Palumbo', 'Orlando', 'Monti', 'Fabbri'];
const POOL = [
  ['Calcetto del giovedì'], ['Regalo per Sara', ['Idee regalo.pdf', 'PDF', '120 KB']], ['Club del libro', ['Libri di novembre.docx', 'DOC', '30 KB']],
  ['Vacanze in Puglia', ['Masseria, foto.jpg', 'JPG', '2,9 MB']], ['Trasloco', ['Inventario scatole.xlsx', 'XLS', '44 KB']], ['Revisione auto', ['Ricevuta revisione.pdf', 'PDF', '150 KB']],
  ['Visita di controllo', ['Referto.pdf', 'PDF', '380 KB']], ['Fattura 2026/118', ['Fattura 2026-118.pdf', 'PDF', '95 KB']], ['Cena di classe'],
  ['Trapano in prestito'], ['Iscrizione in palestra', ['Modulo iscrizione palestra.pdf', 'PDF', '130 KB']],
  ['Foto del matrimonio', ['Matrimonio, 120 foto.zip', 'ZIP', '310 MB']], ['Riunione genitori', ['Verbale riunione genitori.docx', 'DOC', '60 KB']], ['Assicurazione casa', ['Polizza casa 2026.pdf', 'PDF', '1,3 MB']],
  ['Casa al mare ad agosto', ['Contratto affitto estivo.pdf', 'PDF', '450 KB']], ['Corso di inglese'], ['Torneo di padel', ['Tabellone torneo.xlsx', 'XLS', '25 KB']],
  ['Bici da sistemare'], ['Compleanno di Leo', ['Invito compleanno Leo.jpg', 'JPG', '700 KB']], ['Spese del gruppo vacanze', ['Spese condivise.xlsx', 'XLS', '33 KB']], ['Curriculum da rivedere', ['CV aggiornato.docx', 'DOC', '80 KB']]
];
const THEM = ['Ciao! Per {t} ti va bene giovedì sera?', 'Ti giro le ultime novità su {t}.', 'Scusa il ritardo, ho visto solo ora il messaggio.', 'Allora confermiamo? Fammi sapere entro domani.', 'Ho sentito anche gli altri: per loro va bene.', 'Ti mando i dettagli, dimmi se torna tutto.', 'Ottimo, grazie mille!', 'Mi era sfuggito, ci penso io.', 'Ti ricordi a che ora avevamo detto?', 'Possiamo spostare di una settimana?'];
const MINE = ['Ok per me.', 'Perfetto, grazie!', 'Facciamo la settimana prossima?', 'Ricevuto, ci guardo stasera.', 'Ci sono, segnato in agenda.', 'Mi sembra tutto giusto.', 'Ti richiamo dopo pranzo.', 'Va benissimo, a presto.'];
FIRST.forEach((first, k) => {
  const name = first + ' ' + LAST[(k * 5 + 2) % LAST.length];
  const r = rq();
  const sig = r < 0.3 ? '' : r < 0.5 ? first : r < 0.65 ? name : r < 0.8 ? name + '\ncell. 3' + (40 + k) + ' 555 0' + (100 + k * 7) : r < 0.9 ? 'Un caro saluto,\n' + first : name + '\n' + pq(['Studio legale associato', 'Farmacia San Marco', 'Agenzia viaggi Mediterranea', 'Officina meccanica Prati']) + '\nVia ' + pq(['Cavour', 'Nomentana', 'Prenestina', 'Cassia']) + ' ' + (10 + k * 3) + ', Roma\n\n' + (rq() < 0.5 ? DISCLAIMER_IT : DISCLAIMER_EN);
  const who = P(name, name.toLowerCase().replace(/\s+/g, '.') + '@esempio.it', sig, { sigDash: rq() < 0.6 });
  const last = 3 + Math.floor(rnd() * 160);
  const nTopics = 1 + Math.floor(rnd() * 3);
  const used = {};
  for (let t = 0; t < nTopics; t++) {
    let tp = pick(POOL);
    while (used[tp[0]]) tp = pick(POOL);
    used[tp[0]] = true;
    const lower = tp[0].charAt(0).toLowerCase() + tp[0].slice(1);
    const n = 1 + Math.floor(rnd() * 5);
    const start = last + (nTopics - 1 - t) * 9 + n * 2;
    const entries = [];
    for (let i = 0; i < n; i++) {
      const me = i > 0 && rnd() < 0.5;
      let text = i === 0 ? THEM[Math.floor(rnd() * 2)].replace('{t}', lower) : (me ? pick(MINE) : pick(THEM).replace('{t}', lower));
      if (!me && i > 0 && rnd() < 0.15) text = pq(T.LUNGHE_POOL);
      entries.push({ d: Math.max(last, start - i * 2), h: 8 + Math.floor(rnd() * 14), m: Math.floor(rnd() * 60), me, text, att: i === 0 && tp[1] && rnd() < 0.75 ? [tp[1]] : undefined, unread: !me && last < 6 && i === n - 1 });
    }
    thread(who, tp[0], entries);
  }
});

// Long emails and threads with more than two people, as real mail is: whole thread quoted under
// every reply, different mail programs, long company signatures, a reply written inside the quote.
const chiara = P('Chiara Benedetti', 'c.benedetti@studiotecnico.esempio.it', 'Geom. Chiara Benedetti\nStudio tecnico Benedetti\nVia Laurentina 77, 00142 Roma\ncell. 338 555 0420', { client: 'gmail-it' });
const galli = P('Roberto Galli', 'r.galli@gallicostruzioni.esempio.it', 'Roberto Galli\nUfficio tecnico\n\nGALLI COSTRUZIONI S.r.l.\nRistrutturazioni · Impermeabilizzazioni · Facciate\nVia della Magliana 640 – 00148 Roma\nTel. 06 5550 9090 – Fax 06 5550 9091\nwww.gallicostruzioni.esempio.it\nP.IVA e C.F. 09876543210\n\n' + DISCLAIMER_IT + '\n\n' + DISCLAIMER_EN, { client: 'outlook', sigDash: false });
// only ever in copy of other people's messages: must not become a person in the list
const monti = { name: 'Federica Monti', address: 'federica.monti@studioferretti.esempio.it' };
thread(chiara, 'Infiltrazioni dal terrazzo', [
  { d: 9, h: 18, m: 40, cc: [galli, monti], text: T.RELAZIONE_TERRAZZO, att: [['Foto terrazzo.zip', 'ZIP', '24 MB'], ['Pianta misure umidità.pdf', 'PDF', '410 KB']] },
  { d: 7, h: 11, m: 20, from: galli, to: [ME, chiara], cc: [monti], subject: 'R: Infiltrazioni dal terrazzo', text: T.PREVENTIVO_GALLI, att: [['Preventivo 2026-214 terrazzo.pdf', 'PDF', '280 KB']], quote: true },
  { d: 6, h: 21, m: 5, me: true, to: [chiara, galli], text: T.DOMANDE_TERRAZZO, quote: true },
  { d: 5, h: 8, m: 50, cc: [galli, monti], text: 'Ciao Alex, ti rispondo qui sotto punto per punto.', inline: { 2: T.RISPOSTE_TERRAZZO[0], 3: T.RISPOSTE_TERRAZZO[2], 4: T.RISPOSTE_TERRAZZO[3] } },
  { d: 1, h: 17, m: 35, from: galli, to: [ME], cc: [chiara, monti], client: 'iphone', quote: true, unread: true, text: 'Buonasera a tutti,\n\nrispondo dal telefono perché sono in cantiere. Sul solaio: se troviamo i ferri arrugginiti, per una superficie come la vostra si parla di 1.500-2.500 euro in più, ma come dice la Geometra è raro.\n\nPer giovedì confermo: arriviamo alle 8:30, due persone, mezza giornata. Mi lasci libero il passaggio verso la porta del terrazzo, grazie.\n\nR. Galli' }
]);
thread(sara, 'Passaggio di consegne progetto Orione', [
  { d: 20, h: 18, m: 5, cc: [davide, luca], text: T.PASSAGGIO_CONSEGNE },
  { d: 19, h: 9, m: 12, me: true, to: [sara], cc: [davide, luca], text: T.DOMANDE_CONSEGNE, quote: true },
  { d: 19, h: 11, m: 47, from: luca, to: [ME, sara], cc: [davide], text: T.RISPOSTA_LUCA, quote: true },
  { d: 18, h: 8, m: 30, from: davide, to: [ME, sara, luca], client: 'outlook-en', text: T.RISPOSTA_DAVIDE, quote: true },
  { d: 18, h: 10, m: 15, to: [ME], cc: [davide, luca], text: T.RISPOSTA_SARA, quote: true },
  { d: 17, h: 19, m: 2, me: true, to: [sara, davide, luca], text: 'Perfetto, allora giovedì facciamo così. Davide, mi mandi una bozza della proposta sulle varianti entro mercoledì? Grazie a tutti, davvero.', quote: true }
]);
thread(admin, 'Verbale dell’assemblea straordinaria del 18 settembre', [
  { d: 22, h: 15, m: 30, to: [{ name: 'Condomini Via Ostiense 12', address: 'condomini@condominio.esempio.it' }], text: T.VERBALE_ASSEMBLEA, att: [['Verbale assemblea 18-09-2026 firmato.pdf', 'PDF', '1,6 MB'], ['Foglio presenze.pdf', 'PDF', '320 KB'], ['Regolamento biciclette approvato.pdf', 'PDF', '140 KB']], quote: false }
]);
thread(lucia, 'Portogallo, il programma', [
  { d: 40, h: 22, m: 15, client: 'apple', text: T.ITINERARIO, att: [['Spese viaggio Portogallo.xlsx', 'XLS', '28 KB']] },
  { d: 39, h: 8, m: 40, me: true, text: 'Ciao Lucia, letto tutto: è perfetto, grazie per esserti sobbarcata l’organizzazione.\n\nCoimbra per me va benissimo, mi incuriosisce il fado diverso. La valigia grande sì, aggiungila pure: al ritorno porto a casa almeno due bottiglie di porto.\n\nPer i soldi ti faccio un bonifico stasera. Il foglio l’ho visto, c’è solo una cosa: l’auto mi sembra contata due volte, nella riga 4 e nella 9. Ricontrolli?', quote: true },
  { d: 39, h: 9, m: 2, text: 'Hai ragione, l’auto era doppia! Corretto, viene 590 a testa 😊 Valigia aggiunta.', quote: true }
]);

// Signatures made of images, as Outlook sends them: the company logo and the social icons arrive
// as small attachments (image001.png…), the same in every message. A real small photo in the same
// message must stay visible. In the text Outlook leaves a [cid:…] placeholder where the logo was.
const bassi = P('Paolo Bassi', 'assistenza@termobassi.esempio.it', '[cid:image001.png@01DB2C4E.7A1F3B20]\nPaolo Bassi\nAssistenza tecnica\nTERMOIDRAULICA BASSI S.n.c.\nVia dei Castani 212 – 00172 Roma\nTel. 06 5554 1180 | Cell. 347 555 0912\nwww.termobassi.esempio.it\n[cid:image002.png@01DB2C4E.7A1F3B20] [cid:image003.png@01DB2C4E.7A1F3B20]', { client: 'outlook', sigDash: false });
const sigImgs = [['image001.png', 'PNG', '14 KB'], ['image002.png', 'PNG', '1 KB'], ['image003.png', 'PNG', '1 KB']];
thread(bassi, 'Manutenzione caldaia, preventivo sostituzione', [
  { d: 13, h: 9, m: 10, text: 'Buongiorno Sig. Moro,\n\ncome anticipato al telefono le invio il preventivo per la sostituzione della caldaia. Quella attuale ha 19 anni e alla prova fumi di ieri il rendimento era al limite: si può ancora usare per questo inverno, ma una riparazione importante non conviene più.\n\nLe propongo una caldaia a condensazione da 25 kW, con installazione, smaltimento della vecchia e pratica per la detrazione. Il prezzo è nel preventivo allegato, valido 30 giorni.\n\nResto a disposizione per qualsiasi chiarimento.\n\nCordiali saluti', att: [['Preventivo 2026-0871 caldaia.pdf', 'PDF', '190 KB']].concat(sigImgs) },
  { d: 12, h: 20, m: 30, me: true, text: 'Buonasera Paolo,\n\ngrazie del preventivo. Prima di decidere mi servirebbe sapere se la canna fumaria va adeguata, e quanto tempo resteremmo senza riscaldamento.\n\nMi può mandare anche il modello esatto della caldaia attuale? Lo chiede l’amministratore per la pratica.\n\nGrazie, Alex Moro', quote: true },
  { d: 11, h: 8, m: 45, text: 'Buongiorno,\n\nla canna fumaria va intubata, è già compresa nel preventivo alla voce 4. Il lavoro si fa in una giornata: la mattina stacchiamo la vecchia, la sera avete già il riscaldamento.\n\nLe allego la foto della targhetta della caldaia attuale, si legge bene modello e matricola.\n\nCordiali saluti', att: [['targhetta caldaia.jpg', 'JPG', '31 KB']].concat(sigImgs), quote: true },
  { d: 2, h: 10, m: 5, unread: true, text: 'Buongiorno Sig. Moro,\n\nle scrivo per sapere se ha deciso: da metà novembre abbiamo poche giornate libere, se vuole fare il lavoro prima del freddo conviene fissare la data entro la prossima settimana.\n\nCordiali saluti', att: sigImgs, quote: true }
]);
// A signature with a gif logo and a jpeg banner, sent once: small and with a signature name.
const conti = P('Studio Conti', 'segreteria@studioconti.esempio.it', 'Segreteria\nStudio Conti – Commercialisti associati\nViale Marconi 98, 00146 Roma\ntel. 06 5553 7720', { client: 'apple' });
thread(conti, 'Documenti per il 730', [
  { d: 16, h: 11, m: 30, text: 'Gentile Alex,\n\nper preparare il 730 ci servono entro fine mese le spese mediche, le ricevute dei lavori in casa e la certificazione unica. Può mandarle anche come foto, purché leggibili.\n\nLe ricordiamo che lo studio resta chiuso dal 30 ottobre al 2 novembre.\n\nUn cordiale saluto', att: [['logo_studioconti.gif', 'GIF', '6 KB'], ['banner-firma.jpg', 'JPG', '22 KB']] }
]);

// A second mailbox (the studio, plain IMAP, not Gmail): the people list is one, the reply leaves
// from the address the message was sent to.
const ME2 = { name: 'Alex Moro', address: 'alex@studiomoro.esempio.it', signature: 'Alex Moro\nStudio Moro – progettazione d’interni\nwww.studiomoro.esempio.it', client: 'thunderbird' };
const ferrante = P('Giulia Ferrante', 'giulia.ferrante@esempio.it', 'Giulia', { client: 'iphone' });
thread(ferrante, 'Bagno di via Merulana, sopralluogo', [
  { d: 8, h: 19, m: 20, text: 'Buonasera Alex,\n\nci ha dato il suo contatto la signora Benedetti, che è rimasta molto contenta della cucina. Vorremmo rifare il bagno dell’appartamento di via Merulana: è piccolo, 4 metri quadri, con una finestra sola, e vorremmo una doccia al posto della vasca.\n\nAvrebbe tempo per un sopralluogo la settimana prossima? Siamo liberi martedì e giovedì dopo le 17.\n\nGrazie' },
  { d: 8, h: 21, m: 5, me: true, text: 'Buonasera Giulia,\n\ngrazie, mi fa piacere. Giovedì alle 17:30 per me va bene: mi manda l’indirizzo preciso e il piano?\n\nSe ha una piantina, anche una foto fatta col telefono, me la porti: mi aiuta a preparare qualche idea già dalla prima volta.', quote: true },
  { d: 3, h: 9, m: 15, unread: true, text: 'Buongiorno, confermo giovedì alle 17:30. Via Merulana 141, scala B, terzo piano, citofono Ferrante. Le allego la piantina che avevamo dal rogito.\n\nA giovedì', att: [['Piantina appartamento.pdf', 'PDF', '640 KB']], quote: true }
], { me: ME2 });
thread(P('Fornace Sartori', 'ordini@fornacesartori.esempio.it', 'Ufficio ordini\nFornace Sartori S.r.l.\nTel. 0444 555 210', { client: 'outlook', sigDash: false }), 'Campionatura cotto, ordine 3381', [
  { d: 15, h: 10, m: 40, text: 'Buongiorno,\n\nle confermiamo la spedizione della campionatura richiesta: cinque formati di cotto naturale e due trattati. Il corriere passa domani in mattinata.\n\nCordiali saluti', att: [['Bolla 3381.pdf', 'PDF', '95 KB']] }
], { me: ME2 });

// Automated senders.
const unsub = (domain) => [['List-Unsubscribe', `<https://${domain}/unsubscribe>`], ['List-Id', `<news.${domain}>`]];
const SVC = [
  [60, 'Banca Esempio', 'noreply@banca.esempio.it', 'Estratto conto di agosto', 'Estratto conto di agosto disponibile nella tua area riservata.', ['Estratto conto agosto.pdf', 'PDF', '210 KB'], 'CATEGORY_UPDATES'],
  [45, 'Comune di Roma', 'noreply@comune.esempio.it', 'Avviso di pagamento TARI 2026', 'Avviso di pagamento TARI 2026, prima rata.', ['Avviso TARI 2026.pdf', 'PDF', '330 KB'], 'CATEGORY_UPDATES'],
  [33, 'Energia Esempio', 'bollette@energia.esempio.it', 'La tua bolletta di agosto', 'Bolletta di agosto disponibile.', ['Bolletta agosto.pdf', 'PDF', '180 KB'], 'CATEGORY_UPDATES'],
  [30, 'Banca Esempio', 'noreply@banca.esempio.it', 'Estratto conto di settembre', 'Estratto conto di settembre disponibile nella tua area riservata.', ['Estratto conto settembre.pdf', 'PDF', '215 KB'], 'CATEGORY_UPDATES'],
  [18, 'Treni Esempio', 'biglietti@treni.esempio.it', 'Il tuo biglietto Roma–Milano', 'Biglietto Roma–Milano, andata e ritorno.', ['Biglietto Roma-Milano.pdf', 'PDF', '120 KB'], 'CATEGORY_UPDATES'],
  [12, 'Corriere Esempio', 'tracking@corriere.esempio.it', 'Spedizione 1Z999', 'Il tuo pacco è stato spedito.', null, 'CATEGORY_UPDATES'],
  [11, 'Corriere Esempio', 'tracking@corriere.esempio.it', 'Spedizione 1Z999', 'Il tuo pacco è stato consegnato.', null, 'CATEGORY_UPDATES'],
  [4, 'Energia Esempio', 'bollette@energia.esempio.it', 'La tua bolletta di settembre', 'Bolletta di settembre disponibile, scade a fine mese.', ['Bolletta settembre.pdf', 'PDF', '182 KB'], 'CATEGORY_UPDATES'],
  [1, 'Banca Esempio', 'noreply@banca.esempio.it', 'Nuovo accesso all’app', 'Accesso all’app da un nuovo dispositivo. Se non sei stato tu, contattaci.', null, 'CATEGORY_UPDATES'],
  [0, 'Corriere Esempio', 'tracking@corriere.esempio.it', 'Consegna prevista oggi', 'Il pacco arriva oggi tra le 14 e le 18.', null, 'CATEGORY_UPDATES']
];
SVC.forEach(([d, name, address, subject, text, att, cat], i) => {
  thread(P(name, address), subject, [{ d, h: 7 + (i % 10), m: (i * 17) % 60, text, att: att ? [att] : undefined, labels: ['\\Inbox', cat], quote: false, unread: d < 2 }]);
});
const NEWS = [
  [21, 'Libreria Esempio', 'news@libreria.esempio.it', 'Le novità di ottobre', 'Le novità di ottobre e gli incontri con gli autori'],
  [14, 'Quotidiano Esempio', 'newsletter@quotidiano.esempio.it', 'La settimana in dieci notizie', 'La settimana in dieci notizie'],
  [9, 'Palestra Esempio', 'info@palestra.esempio.it', 'Nuovi corsi del mattino', 'Nuovi corsi del mattino: pilates, yoga e funzionale'],
  [7, 'Libreria Esempio', 'news@libreria.esempio.it', 'Tre libri per l’autunno', 'Tre libri per l’autunno, scelti dai librai'],
  [3, 'Quotidiano Esempio', 'newsletter@quotidiano.esempio.it', 'La settimana in dieci notizie', 'La settimana in dieci notizie']
];
NEWS.forEach(([d, name, address, subject, title], i) => {
  const domain = address.split('@')[1];
  const html = `<!doctype html><html><head><style>body{font-family:Arial}</style></head><body><table width="600"><tr><td><img src="https://img.${domain}/testata-${i}.jpg" width="600" height="160" alt="${name}"><h1>${title}</h1><p>Ciao, ecco la selezione di questa settimana.</p><p><img src="https://img.${domain}/scelta-${i}.jpg" width="280" height="180" alt="La scelta della settimana"></p><ul><li>Primo articolo</li><li>Secondo articolo</li><li>Terzo articolo</li></ul><p><a href="https://${domain}/leggi">Leggi tutto sul sito</a></p><img src="https://t.${domain}/o.gif?u=8f3a${i}" width="1" height="1" alt=""><p style="font-size:11px">Ricevi questa email perché sei iscritto. <a href="https://${domain}/unsubscribe">Disiscriviti</a></p></td></tr></table></body></html>`;
  thread(P(name, address), subject, [{ d, h: 6, m: 30 + i, html, text: null, headers: unsub(domain), labels: ['\\Inbox', 'CATEGORY_PROMOTIONS'], quote: false }]);
});

// A record the export could not date.
messages.push({
  id: 'syn-bad-date', date: null, dateRaw: 'Mon, 32 Foo 2026 99:00', internalDate: at(27, 10, 0).toISOString(),
  from: { name: 'Vecchio Gestionale', address: 'gestionale@esempio.it' }, to: [ME], cc: [], subject: 'Promemoria automatico',
  messageId: '<syn-bad-date@esempio.it>', inReplyTo: null, references: [], text: 'Questo messaggio ha una data che non si legge.', html: null,
  attachments: [], headers: [['Date', 'Mon, 32 Foo 2026 99:00']], flags: { seen: true, flagged: false },
  gmail: { threadId: 'bad-date', labels: ['\\Inbox'] }, problems: ['date: non leggibile']
});

// Bug reproductions: cases/*.json (files starting with "_" are templates and are skipped).
const cases = [];
for (const f of readdirSync(join(HERE, 'cases')).filter((f) => f.endsWith('.json') && !f.startsWith('_')).sort()) {
  const c = JSON.parse(readFileSync(join(HERE, 'cases', f), 'utf8'));
  cases.push({ id: c.id, title: c.title, count: c.messages.length });
  // "quote": true (v15): the reply quotes the previous message of the same thread, in the format
  // of the sender's program (from.client), as the generated threads do.
  const lastOf = {};
  c.messages.forEach((m, i) => {
    const rec = Object.assign({ id: `case-${c.id}-${i}`, cc: [], references: [], attachments: [], headers: [], flags: { seen: true, flagged: false }, problems: [] }, m, {
      gmail: Object.assign({ threadId: `case-${c.id}`, labels: ['\\Inbox'] }, m.gmail || {}, { labels: ((m.gmail && m.gmail.labels) || ['\\Inbox']).concat(['CASE/' + c.id]) })
    });
    const prev = lastOf[rec.gmail.threadId];
    if (rec.quote && prev) rec.text = (rec.text || '') + '\n\n' + quoteBlock((rec.from && rec.from.client) || 'gmail-it', prev);
    delete rec.quote;
    lastOf[rec.gmail.threadId] = rec;
    messages.push(rec);
  });
}

// The user's own Gmail labels: in the app they become search filters.
const LABELS = [[/assemblea|condominio|facciata|rendiconto|verbale/i, 'Condominio'], [/terrazzo|caldaia|cucina/i, 'Casa'], [/orione|report trimestrale|riunione di lunedì/i, 'Lavoro'], [/portogallo/i, 'Viaggi'], [/redditi|730|scadenze di novembre/i, 'Tasse']];
messages.forEach((m) => {
  const hit = LABELS.find(([re]) => re.test(m.subject || ''));
  if (hit && m.gmail) m.gmail.labels = m.gmail.labels.concat([hit[1]]);
});
// Spam: it is in the mailbox, it must not appear anywhere.
messages.push({
  id: 'syn-spam-1', date: at(2, 4, 12).toISOString(), dateRaw: rfcDate(at(2, 4, 12)), from: { name: 'Premi Fedeltà', address: 'premi@vincita-sicura.esempio.com' }, to: [ME], cc: [],
  subject: 'Hai vinto un buono da 500 euro!', messageId: '<spam-1@vincita-sicura.esempio.com>', inReplyTo: null, references: [],
  text: 'Complimenti! Sei stato selezionato. Clicca qui per ritirare il tuo premio entro 24 ore.', html: null, attachments: [], headers: [],
  flags: { seen: false, flagged: false }, gmail: { threadId: 'spam-1', labels: ['\\Spam'] }, problems: []
});

// Real files for every synthetic attachment, so the prototype can open them.
const ATT_DIR = join(HERE, 'attachments');
rmSync(ATT_DIR, { recursive: true, force: true });
mkdirSync(ATT_DIR, { recursive: true });

// A PDF with real pages: page 1 looks like a document (coloured band, title, paragraphs, a table),
// the others carry sections and the page number. The thumbnail is drawn from page 1 (v14).
function pdf(title, lines, pages, hue) {
  const esc = (s) => s.replace(/[’‘]/g, "'").replace(/[“”«»]/g, '"').replace(/[–—]/g, '-').replace(/…/g, '...').replace(/⋅/g, '-').replace(/€/g, 'EUR')
    .replace(/[\\()]/g, (c) => '\\' + c).replace(/[^\x20-\x7e\xa0-\xff]/g, '?');
  const rgb = (h, s, l) => {
    const k = (n) => (n + h / 30) % 12, a = s * Math.min(l, 1 - l);
    const f = (n) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
    return [f(0), f(8), f(4)].map((v) => v.toFixed(3)).join(' ');
  };
  const bars = (y, n, seed) => {
    const out = ['0.82 g'];
    for (let i = 0; i < n; i++) {
      const w = i % 5 === 4 ? 180 + ((seed * 31 + i * 17) % 160) : 470 - ((seed * 13 + i * 7) % 40);
      out.push(`56 ${y - i * 15} ${w} 7 re f`);
    }
    return out;
  };
  const page = (k) => {
    const ops = [];
    if (k === 1) {
      ops.push(`${rgb(hue, 0.45, 0.42)} rg`, '0 772 595 70 re f');
      ops.push('BT', '1 1 1 rg', '/F1 20 Tf', '56 800 Td', `(${esc(title)}) Tj`, 'ET');
      ops.push('BT', '0.15 g', '/F1 11 Tf', '56 740 Td');
      lines.filter((l) => !/^Pagina /.test(l)).forEach((l) => ops.push(`(${esc(l)}) Tj`, '0 -16 Td'));
      ops.push('ET');
      ops.push(...bars(530, 9, k + title.length));
      ops.push('0.6 G', '0.8 w', '56 250 483 120 re S', '56 330 m 539 330 l S', '56 290 m 539 290 l S', '300 250 m 300 370 l S');
      ops.push(`${rgb(hue, 0.35, 0.85)} rg`, '56.4 330.4 482.2 39.2 re f');
      ops.push(...bars(200, 5, k * 3));
    } else {
      ops.push('BT', '0.15 g', '/F1 15 Tf', '56 780 Td', `(${k}. Sezione ${k}) Tj`, 'ET');
      ops.push(...bars(740, 22, k + title.length));
      ops.push(...bars(380, 14, k * 7));
    }
    ops.push('BT', '0.45 g', '/F1 9 Tf', '480 40 Td', `(Pagina ${k} di ${pages}) Tj`, 'ET');
    return ops.join('\n');
  };
  const objs = ['<< /Type /Catalog /Pages 2 0 R >>', null, '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>'];
  const kids = [];
  for (let k = 1; k <= pages; k++) {
    const body = page(k);
    const pageNo = objs.length + 1;
    kids.push(`${pageNo} 0 R`);
    objs.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents ${pageNo + 1} 0 R /Resources << /Font << /F1 3 0 R >> >> >>`);
    objs.push(`<< /Length ${Buffer.byteLength(body, 'latin1')} >>\nstream\n${body}\nendstream`);
  }
  objs[1] = `<< /Type /Pages /Kids [${kids.join(' ')}] /Count ${pages} >>`;
  let out = '%PDF-1.4\n';
  const offsets = [];
  objs.forEach((o, i) => { offsets.push(Buffer.byteLength(out, 'latin1')); out += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const xref = Buffer.byteLength(out, 'latin1');
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n` + offsets.map((o) => String(o).padStart(10, '0') + ' 00000 n \n').join('');
  out += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}
// Pages from the declared size: about one page every 95 KB, from 1 to 24.
const pagesFor = (size) => Math.max(1, Math.min(24, Math.round((size || 0) / 95000)));
// The thumbnail of page 1, as the engine will make it with pdftoppm; without poppler-utils, none.
let hasPdftoppm = true;
function pdfThumb(pdfPath, outBase) {
  if (!hasPdftoppm) return false;
  try {
    execFileSync('pdftoppm', ['-f', '1', '-l', '1', '-singlefile', '-scale-to', '360', '-jpeg', '-jpegopt', 'quality=72', pdfPath, outBase], { stdio: 'ignore' });
    return existsSync(outBase + '.jpg');
  } catch (e) {
    hasPdftoppm = false;
    console.warn('pdftoppm non c’è: i PDF restano senza miniatura');
    return false;
  }
}
const xmlEsc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
function svg(name, i) {
  const hues = [[214, 60], [28, 70], [140, 40], [262, 45], [190, 55], [45, 75]];
  const [h, s] = hues[i % hues.length];
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800" viewBox="0 0 1200 800">
<rect width="1200" height="800" fill="hsl(${h},${s}%,82%)"/>
<circle cx="900" cy="250" r="120" fill="hsl(${h},${s}%,92%)"/>
<path d="M0 640 L300 380 L520 560 L760 300 L1200 700 L1200 800 L0 800 Z" fill="hsl(${h},${s}%,45%)"/>
<path d="M0 700 L260 520 L560 720 L900 520 L1200 760 L1200 800 L0 800 Z" fill="hsl(${h},${s}%,30%)"/>
<text x="48" y="80" font-family="sans-serif" font-size="40" fill="hsl(${h},${s}%,20%)">${xmlEsc(name)}</text>
<text x="48" y="128" font-family="sans-serif" font-size="24" fill="hsl(${h},${s}%,25%)">immagine sintetica</text>
</svg>\n`;
}
// An image with the declared pixel size: a logo strip, a small icon or a screenshot-like picture.
function sizedSvg(name, from, w, h, i) {
  const who = (from && (from.name || from.address)) || '?';
  // v15: a QR code and a barcode strip, drawn with plain rects so the PNG for the engine looks the same
  if (/qr/i.test(name)) {
    const n = 25, u = w / (n + 4), cells = [];
    let s = name.length * 7 + i;
    const finder = (x, y) => (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7);
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
      s = (s * 1103515245 + 12345) & 0x7fffffff;
      if (!finder(x, y) && (s >> 16) % 2) cells.push(`<rect x="${((x + 2) * u).toFixed(1)}" y="${((y + 2) * u).toFixed(1)}" width="${u.toFixed(1)}" height="${u.toFixed(1)}" fill="#111111"/>`);
    }
    const eye = (x, y) => [[0, 7, '#111111'], [1, 5, '#ffffff'], [2, 3, '#111111']].map(([o, k, f]) => `<rect x="${((x + 2 + o) * u).toFixed(1)}" y="${((y + 2 + o) * u).toFixed(1)}" width="${(k * u).toFixed(1)}" height="${(k * u).toFixed(1)}" fill="${f}"/>`).join('');
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect x="0" y="0" width="${w}" height="${h}" fill="#ffffff"/>${eye(0, 0)}${eye(n - 7, 0)}${eye(0, n - 7)}${cells.join('')}</svg>\n`;
  }
  if (h < 120 && w >= 3 * h) {
    const bars = [];
    let x = Math.round(w * 0.06), s = name.length * 13 + i;
    while (x < w * 0.94) {
      s = (s * 1103515245 + 12345) & 0x7fffffff;
      const bw = 1 + ((s >> 16) % 4);
      bars.push(`<rect x="${x}" y="${Math.round(h * 0.12)}" width="${bw}" height="${Math.round(h * 0.62)}" fill="#1a1a1a"/>`);
      x += bw + 1 + ((s >> 20) % 3);
    }
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect x="0" y="0" width="${w}" height="${h}" fill="#f4f1ea"/>${bars.join('')}<text x="${Math.round(w * 0.06)}" y="${Math.round(h * 0.92)}" font-family="monospace" font-size="${Math.max(8, Math.round(h * 0.14))}" fill="#333">${xmlEsc(name)}</text></svg>\n`;
  }
  if ((w >= 300 && h >= 120) || /^(image|schermata|screenshot)/i.test(name)) {
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><rect width="${w}" height="${h}" fill="#F3F3F1"/><rect width="${w}" height="${Math.round(h * 0.12)}" fill="#3B5B7A"/><rect x="${Math.round(w * 0.05)}" y="${Math.round(h * 0.22)}" width="${Math.round(w * 0.9)}" height="${Math.round(h * 0.6)}" fill="#fff" stroke="#C9C9C4"/><text x="${Math.round(w * 0.08)}" y="${Math.round(h * 0.34)}" font-family="sans-serif" font-size="${Math.max(12, Math.round(h * 0.06))}" fill="#333">${xmlEsc(name)}</text><text x="${Math.round(w * 0.08)}" y="${Math.round(h * 0.46)}" font-family="sans-serif" font-size="${Math.max(10, Math.round(h * 0.045))}" fill="#666">immagine sintetica, ${w}×${h}</text></svg>\n`;
  }
  if (w <= 120 && h <= 120) {
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 96 96"><circle cx="48" cy="48" r="44" fill="#F2C94C"/><circle cx="34" cy="40" r="6" fill="#5B4300"/><circle cx="62" cy="40" r="6" fill="#5B4300"/><path d="M28 58 Q48 78 68 58" stroke="#5B4300" stroke-width="6" fill="none" stroke-linecap="round"/></svg>\n`;
  }
  const ini = who.split(/\s+/).map((x) => x[0]).join('').slice(0, 2).toUpperCase();
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 200 80"><rect width="200" height="80" fill="#fff"/><path d="M8 66 L40 14 L72 66 Z" fill="#C2410C"/><text x="84" y="46" font-family="sans-serif" font-size="22" font-weight="700" fill="#333">EDIL</text><text x="84" y="66" font-family="sans-serif" font-size="13" fill="#666">${xmlEsc(ini)} srl</text></svg>\n`;
}
// Small signature images: a logo with the sender's initials, or a round social icon.
function smallSvg(from, size, i) {
  const who = (from && (from.name || from.address)) || '?';
  if (size < 4 * 1024) {
    const fill = i % 2 ? '#2A5DB0' : '#2C7DA0';
    return `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><circle cx="16" cy="16" r="16" fill="${fill}"/><text x="16" y="21" text-anchor="middle" font-family="sans-serif" font-size="14" font-weight="700" fill="#fff">${i % 2 ? 'in' : 'f'}</text></svg>\n`;
  }
  const ini = who.split(/\s+/).map((w) => w[0]).join('').slice(0, 2).toUpperCase();
  return `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="64" viewBox="0 0 240 64"><rect width="240" height="64" fill="#fff"/><rect x="4" y="8" width="48" height="48" rx="8" fill="#B5462E"/><text x="28" y="40" text-anchor="middle" font-family="sans-serif" font-size="20" font-weight="700" fill="#fff">${xmlEsc(ini)}</text><text x="64" y="40" font-family="sans-serif" font-size="18" font-weight="700" fill="#333">${xmlEsc(who.toUpperCase()).slice(0, 18)}</text></svg>\n`;
}
const toIcs = (d) => d.toISOString().replace(/[-:]/g, '').replace(/\.\d{3}/, '');
let fileNo = 0;
const fileByKey = {};
messages.forEach((msg) => {
  (msg.attachments || []).forEach((a) => {
    const no = ++fileNo;
    const name = a.filename || 'allegato';
    const ext = (name.split('.').pop() || '').toLowerCase();
    const topic = (msg.subject || '').replace(/^\s*((re|r|fwd|i)\s*:\s*)+/i, '').replace(/\s+-\s+(lun|mar|mer|gio|ven|sab|dom)\b.*$/i, '');
    const who = (msg.from && (msg.from.name || msg.from.address)) || '';
    const base = `${String(no).padStart(3, '0')}`;
    // v15: "fileKey" marks the same file sent more than once, maybe under another name: same bytes
    const same = a.fileKey && fileByKey[a.fileKey];
    if (same) {
      for (const [from, to] of same.files) writeFileSync(join(ATT_DIR, base + to), readFileSync(join(ATT_DIR, from)));
      Object.assign(a, same.fields(base));
    } else if (ext === 'pdf') {
      const pages = pagesFor(a.size);
      // v15: "text" is what the document says, for the search inside attachments; otherwise the usual sections
      const body = a.text ? a.text.split('\n') : ['1. Premessa e obiettivi', '2. Dettaglio delle voci', '3. Tempi e responsabilita', '4. Condizioni'];
      const lines = [`Documento sintetico creato per il prototipo.`, `Argomento: ${topic}`, `Da: ${who}`, `Data: ${(msg.date || '').slice(0, 10)}`, ''].concat(body, ['', `Pagina 1 di ${pages}`]);
      writeFileSync(join(ATT_DIR, base + '.pdf'), pdf(name.replace(/\.pdf$/i, ''), lines, pages, (no * 47) % 360));
      a.path = `attachments/${base}.pdf`;
      a.pages = pages;
      if (pdfThumb(join(ATT_DIR, base + '.pdf'), join(ATT_DIR, base))) a.thumb = `attachments/${base}.jpg`;
      a.preview = [name.replace(/\.pdf$/i, '')].concat(lines).join('\n');
    } else if (['jpg', 'jpeg', 'png', 'gif'].includes(ext)) {
      // the pixel size, as the engine reads it from the first bytes of the file (v14); noDims: not downloaded yet
      const small = a.size < 40 * 1024 && !/targhetta/i.test(name);
      const w = a.width || (small ? (a.size < 4 * 1024 ? 32 : 240) : 1200);
      const h = a.height || (small ? (a.size < 4 * 1024 ? 32 : 64) : 800);
      writeFileSync(join(ATT_DIR, base + '.svg'), a.width && !(/\.jpe?g$/i.test(name) && w >= 300 && h >= 120) ? sizedSvg(name, msg.from, w, h, no) : small && !a.width ? smallSvg(msg.from, a.size, no) : svg(name, no));
      a.path = `attachments/${base}.svg`;
      if (a.noDims) delete a.noDims;
      else { a.width = w; a.height = h; }
    } else if (['xlsx', 'xls', 'csv'].includes(ext)) {
      const rows = ['Voce;Quantità;Importo'].concat(['Materiali', 'Manodopera', 'Trasporto', 'Smaltimento', 'Imprevisti', 'Totale'].map((v, i) => `${v};${i < 5 ? 1 + (no + i) % 7 : ''};${(120 + ((no * 37 + i * 91) % 900)).toFixed(2).replace('.', ',')}`));
      const csv = rows.join('\n') + '\n';
      writeFileSync(join(ATT_DIR, base + '.csv'), csv);
      a.path = `attachments/${base}.csv`;
      a.preview = csv;
    } else if (['docx', 'doc', 'txt'].includes(ext)) {
      const txt = `${name}\n\nTesto sintetico creato per il prototipo.\nArgomento: ${topic}\n\nPrimo paragrafo con le informazioni principali.\nSecondo paragrafo con i dettagli e le scadenze.\n`;
      writeFileSync(join(ATT_DIR, base + '.txt'), txt);
      a.path = `attachments/${base}.txt`;
      a.preview = txt;
    } else if (ext === 'ics') {
      const start = new Date(msg.date || NOW);
      const end = new Date(start.getTime() + 3600000);
      const ics = ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//Ambassadress//sintetico//IT', 'METHOD:REQUEST', 'BEGIN:VEVENT',
        `UID:${base}@esempio.it`, `DTSTART:${toIcs(start)}`, `DTEND:${toIcs(end)}`, `SUMMARY:${topic.replace(/^(Invito aggiornato|Invito)\s*:\s*/i, '')}`,
        `ORGANIZER;CN=${who}:mailto:${msg.from && msg.from.address}`, 'END:VEVENT', 'END:VCALENDAR'].join('\r\n') + '\r\n';
      writeFileSync(join(ATT_DIR, base + '.ics'), ics);
      a.path = `attachments/${base}.ics`;
      a.preview = ics;
    } else {
      a.preview = `Archivio «${name}»: nei dati sintetici il contenuto non c'è.`;
    }
    if (a.fileKey && !same) {
      const files = [a.path, a.thumb].filter(Boolean).map((p) => [p.replace('attachments/', ''), p.slice(p.lastIndexOf('.'))]);
      const keep = { preview: a.preview, pages: a.pages, width: a.width, height: a.height, text: a.text, code: a.code };
      fileByKey[a.fileKey] = { files, fields: (b) => Object.fromEntries(Object.entries(Object.assign({}, keep, { path: a.path && `attachments/${b}${a.path.slice(a.path.lastIndexOf('.'))}`, thumb: a.thumb && `attachments/${b}.jpg` })).filter(([, v]) => v !== undefined)) };
    }
    delete a.fileKey;
  });
});

messages.forEach((m) => { delete m.textMain; });
messages.sort((a, b) => String(a.date || a.internalDate).localeCompare(String(b.date || b.internalDate)));

const data = {
  schema: 1,
  meta: { source: 'synthetic', generatedAt: NOW.toISOString(), count: messages.length, shiftToNow: true, cases },
  account: { address: ME.address, name: ME.name, aliases: [ME2.address] },
  accounts: [{ address: ME.address, kind: 'gmail', server: 'imap.gmail.com' }, { address: ME2.address, kind: 'imap', server: 'mail.studiomoro.esempio.it' }],
  messages
};
writeFileSync(join(HERE, 'synthetic.js'), '// Generated by generate-synthetic.mjs, do not edit.\nwindow.AMB_DATA = ' + JSON.stringify(data) + ';\n');
console.log(`synthetic.js: ${messages.length} messages, ${cases.length} cases`);
