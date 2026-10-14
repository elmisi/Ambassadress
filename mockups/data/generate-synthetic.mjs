// Generates synthetic.js: a made-up mailbox in the same format as tools/gmail-export.py.
// It is the only dataset the canvas and development ever use.
// Bug reproductions go in cases/*.json and are merged in here.
//
//   node mockups/data/generate-synthetic.mjs

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

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

const MIME = { PDF: 'application/pdf', JPG: 'image/jpeg', DOC: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', XLS: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', ZIP: 'application/zip' };
const bytes = (s) => {
  if (!s) return 20000;
  const m = /([\d,]+)\s*(KB|MB)/.exec(s);
  if (!m) return 20000;
  const n = parseFloat(m[1].replace(',', '.'));
  return Math.round(n * (m[2] === 'MB' ? 1024 * 1024 : 1024));
};

let seq = 0;
const messages = [];

// One conversation = one Gmail thread between ME and `who` about `topic`.
// entries: { d, h, m, me, text, att: [[name, ext, size]], cc: [person], quote: bool, fwd: bool }
function thread(who, topic, entries, opts = {}) {
  const threadId = (0x18f0000000000000n + BigInt(++seq * 7919)).toString(16);
  let prev = null;
  const refs = [];
  entries.forEach((e, i) => {
    const date = e.date || at(e.d, e.h ?? 10, e.m ?? 0);
    const from = e.from || (e.me ? ME : who);
    const to = e.to || (e.me ? [who] : [ME]);
    const subject = e.subject ?? (i === 0 ? topic : (opts.prefix || 'Re: ') + topic);
    const messageId = `<syn-${++seq}@esempio.it>`;
    let text = e.text;
    if (e.signature ?? (!e.me && who.signature)) text += '\n\n-- \n' + who.signature;
    if (prev && (e.quote ?? rnd() < 0.55)) {
      if (e.outlook) {
        text += `\n\nDa: ${prev.from.name} <${prev.from.address}>\nInviato: ${itQuoteDate(new Date(prev.date))}\nA: ${prev.to.map((p) => p.name || p.address).join('; ')}\nOggetto: ${prev.subject}\n\n${prev.textMain}`;
      } else {
        text += `\n\nIl giorno ${itQuoteDate(new Date(prev.date))} ${prev.from.name} <${prev.from.address}> ha scritto:\n` + prev.textMain.split('\n').map((l) => '> ' + l).join('\n');
      }
    }
    const labels = e.labels || (from === ME ? ['\\Sent'] : ['\\Inbox']);
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

const P = (name, address, signature) => ({ name, address, signature });
const lucia = P('Lucia Ferri', 'lucia.ferri@esempio.it');
const marco = P('Marco Bassi', 'marco.bassi@esempio.it');
const elena = P('Arch. Elena Sartori', 'elena.sartori@studiosartori.esempio.it', 'Arch. Elena Sartori\nStudio Sartori architetti\nVia [indirizzo] · 06 5550 1234');
const anna = P('Anna Ricci', 'anna.ricci@scuolacollodi.esempio.it', 'Maestra Anna Ricci\nScuola primaria Collodi, classe 3B');
const martini = P('Studio Martini', 'segreteria@studiomartini.esempio.it', 'Studio Martini · Commercialisti associati\nQuesta email e i suoi allegati sono riservati. Se l’hai ricevuta per errore avvisaci e cancellala.');
const admin = P('Amministrazione Condominio Via Ostiense 12', 'amministrazione@condominio.esempio.it');
const paolo = P('Paolo Conti', 'paolo.conti@esempio.it');
const paoloWork = P('Conti Paolo', 'p.conti@azienda.esempio.it', 'Paolo Conti | Ufficio acquisti\nAzienda Esempio S.p.A.');
const giulia = P('Giulia Rinaldi', 'giulia@cucinerinaldi.esempio.it');
const davide = P('Davide Greco', 'davide.greco@lavoro.esempio.it');
const sara = P('Sara Neri', 'sara.neri@lavoro.esempio.it');
const luca = P('Luca Fontana', 'luca.fontana@lavoro.esempio.it');
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
const LONG = 'Ti scrivo con calma perché la cosa si è un po’ complicata. Ho sentito tutti e ognuno ha una disponibilità diversa: chi può solo nel weekend, chi solo la sera, chi preferisce rimandare a dopo le feste.\n\nHo provato a mettere insieme le date, ma l’unica che va bene quasi a tutti è giovedì 23. Se per te è un problema dimmelo subito, così sento di nuovo gli altri prima di confermare. Nel frattempo ho chiesto anche un preventivo, ma non mi hanno ancora risposto: appena so qualcosa ti aggiorno.';
FIRST.forEach((first, k) => {
  const name = first + ' ' + LAST[(k * 5 + 2) % LAST.length];
  const who = P(name, name.toLowerCase().replace(/\s+/g, '.') + '@esempio.it');
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
      if (!me && i > 0 && rnd() < 0.15) text = LONG;
      entries.push({ d: Math.max(last, start - i * 2), h: 8 + Math.floor(rnd() * 14), m: Math.floor(rnd() * 60), me, text, att: i === 0 && tp[1] && rnd() < 0.75 ? [tp[1]] : undefined, unread: !me && last < 6 && i === n - 1 });
    }
    thread(who, tp[0], entries);
  }
});

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
  const html = `<!doctype html><html><head><style>body{font-family:Arial}</style></head><body><table width="600"><tr><td><h1>${title}</h1><p>Ciao, ecco la selezione di questa settimana.</p><ul><li>Primo articolo</li><li>Secondo articolo</li><li>Terzo articolo</li></ul><p><a href="https://${domain}/leggi">Leggi tutto sul sito</a></p><p style="font-size:11px">Ricevi questa email perché sei iscritto. <a href="https://${domain}/unsubscribe">Disiscriviti</a></p></td></tr></table></body></html>`;
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
  c.messages.forEach((m, i) => {
    messages.push(Object.assign({ id: `case-${c.id}-${i}`, cc: [], references: [], attachments: [], headers: [], flags: { seen: true, flagged: false }, problems: [] }, m, {
      gmail: Object.assign({ threadId: `case-${c.id}`, labels: ['\\Inbox'] }, m.gmail || {}, { labels: ((m.gmail && m.gmail.labels) || ['\\Inbox']).concat(['CASE/' + c.id]) })
    }));
  });
}

// Real files for every synthetic attachment, so the prototype can open them.
const ATT_DIR = join(HERE, 'attachments');
rmSync(ATT_DIR, { recursive: true, force: true });
mkdirSync(ATT_DIR, { recursive: true });

function pdf(title, lines) {
  const esc = (s) => s.replace(/[’‘]/g, "'").replace(/[“”«»]/g, '"').replace(/[–—]/g, '-').replace(/…/g, '...').replace(/⋅/g, '-')
    .replace(/[\\()]/g, (c) => '\\' + c).replace(/[^\x20-\x7e\xa0-\xff]/g, '?');
  const body = ['BT', '/F1 18 Tf', '56 780 Td', `(${esc(title)}) Tj`, '/F1 11 Tf', '0 -30 Td']
    .concat(...lines.map((l) => [`(${esc(l)}) Tj`, '0 -17 Td']), ['ET']).join('\n');
  const objs = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${Buffer.byteLength(body, 'latin1')} >>\nstream\n${body}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>'
  ];
  let out = '%PDF-1.4\n';
  const offsets = [];
  objs.forEach((o, i) => { offsets.push(Buffer.byteLength(out, 'latin1')); out += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const xref = Buffer.byteLength(out, 'latin1');
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n` + offsets.map((o) => String(o).padStart(10, '0') + ' 00000 n \n').join('');
  out += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
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
const toIcs = (d) => d.toISOString().replace(/[-:]/g, '').replace(/\.\d{3}/, '');
let fileNo = 0;
messages.forEach((msg) => {
  (msg.attachments || []).forEach((a) => {
    const no = ++fileNo;
    const name = a.filename || 'allegato';
    const ext = (name.split('.').pop() || '').toLowerCase();
    const topic = (msg.subject || '').replace(/^\s*((re|r|fwd|i)\s*:\s*)+/i, '').replace(/\s+-\s+(lun|mar|mer|gio|ven|sab|dom)\b.*$/i, '');
    const who = (msg.from && (msg.from.name || msg.from.address)) || '';
    const base = `${String(no).padStart(3, '0')}`;
    if (ext === 'pdf') {
      const lines = [`Documento sintetico creato per il prototipo.`, `Argomento: ${topic}`, `Da: ${who}`, `Data: ${(msg.date || '').slice(0, 10)}`, '',
        '1. Premessa e obiettivi', '2. Dettaglio delle voci', '3. Tempi e responsabilita', '4. Condizioni', '', 'Pagina 1 di 1'];
      writeFileSync(join(ATT_DIR, base + '.pdf'), pdf(name.replace(/\.pdf$/i, ''), lines));
      a.path = `attachments/${base}.pdf`;
      a.preview = [name.replace(/\.pdf$/i, '')].concat(lines).join('\n');
    } else if (['jpg', 'jpeg', 'png'].includes(ext)) {
      writeFileSync(join(ATT_DIR, base + '.svg'), svg(name, no));
      a.path = `attachments/${base}.svg`;
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
  });
});

messages.forEach((m) => { delete m.textMain; });
messages.sort((a, b) => String(a.date || a.internalDate).localeCompare(String(b.date || b.internalDate)));

const data = {
  schema: 1,
  meta: { source: 'synthetic', generatedAt: NOW.toISOString(), count: messages.length, shiftToNow: true, cases },
  account: { address: ME.address, name: ME.name, aliases: [] },
  messages
};
writeFileSync(join(HERE, 'synthetic.js'), '// Generated by generate-synthetic.mjs, do not edit.\nwindow.AMB_DATA = ' + JSON.stringify(data) + ';\n');
console.log(`synthetic.js: ${messages.length} messages, ${cases.length} cases`);
