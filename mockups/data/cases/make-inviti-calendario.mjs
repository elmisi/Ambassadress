// Builds inviti-calendario.json: one colleague whose 39 messages split into 26 "topics",
// most of them calendar invitations, reschedules and meeting-recording notices.
// Same shape as a real mailbox reported on 2026-10-14; names and projects are made up.
//
//   node mockups/data/cases/make-inviti-calendario.mjs

import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ME = { name: 'Alex Moro', address: 'alex.moro@esempio.it' };
const LB = { name: 'Lorenzo Bellini', address: 'lorenzo@bellini.esempio.it' };
const IC = { name: 'Ilaria Conte', address: 'ilaria.conte@faro.esempio.it' };
const PS = { name: 'Pietro Sala', address: 'pietro.sala@faro.esempio.it' };
const GV = { name: 'Giada Valle', address: 'giada.valle@faro.esempio.it' };

const iso = (s) => new Date(s + ':00+02:00').toISOString();
const DAYS = ['domenica', 'lunedì', 'martedì', 'mercoledì', 'giovedì', 'venerdì', 'sabato'];
const MONTHS = ['gennaio', 'febbraio', 'marzo', 'aprile', 'maggio', 'giugno', 'luglio', 'agosto', 'settembre', 'ottobre', 'novembre', 'dicembre'];
const longDate = (day) => { const d = new Date(day + 'T12:00:00+02:00'); return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`; };
const shortDate = (day) => { const d = new Date(day + 'T12:00:00+02:00'); return `${DAYS[d.getDay()].slice(0, 3)} ${d.getDate()} ${MONTHS[d.getMonth()].slice(0, 3)} ${d.getFullYear()}`; };
const cut = (s, n) => (s.length > n ? s.slice(0, n) + '...' : s);

let n = 0;
const messages = [];
let thread = 0;
const push = (m) => {
  n++;
  messages.push(Object.assign({
    id: 'case-inviti-' + n,
    to: [ME], cc: [], references: [], attachments: [], otherParts: [],
    messageId: `<case-inviti-${n}@esempio.it>`,
    flags: { seen: true, flagged: false },
    gmail: { threadId: 'case-inviti-t' + (m.thread || ++thread), labels: m.from === ME ? ['\\Sent'] : ['\\Inbox'] },
    problems: []
  }, m));
  delete messages[messages.length - 1].thread;
};

const guests = [LB, ME, IC, PS];
const calendarHeaders = [['Sender', 'Google Calendar <calendar-notification@google.com>'], ['Reply-To', `${LB.name} <${LB.address}>`], ['Content-Type', 'multipart/mixed; boundary="0000000000001"']];
const ics = { filename: 'invite.ics', mimeType: 'application/ics', size: 2900, inline: false };

function invite(sent, title, day, from, to, { updated = false, before = null, tentative = false } = {}) {
  const t = (tentative ? 'Tentative ' : '') + title;
  const when = `${shortDate(day)} ${from} - ${to} (CEST)`;
  const subject = `${updated ? 'Invito aggiornato' : 'Invito'}: ${updated ? cut(t, 55) : t} - ${when} (${ME.address})`;
  const lines = [
    updated ? 'Questo evento è stato modificato.' : `${LB.name} ti ha invitato a un evento.`,
    '',
    t,
    `${longDate(day)} ⋅ ${from} – ${to}`,
    'Ora dell’Europa centrale - Roma'
  ];
  if (before) lines.push(`(modificato: prima era ${longDate(before[0])} ⋅ ${before[1]} – ${before[2]})`);
  lines.push('', 'Partecipa con Google Meet', 'meet.google.com/abc-defg-hij', '', 'Invitati');
  guests.forEach((g, i) => lines.push(g.address === LB.address ? `${g.name} - organizzatore` : (i === 1 ? g.address : g.name)));
  lines.push('', `Visualizza tutti i dettagli dell’evento: https://calendar.google.com/calendar/event?action=VIEW&eid=${Buffer.from(t).toString('base64').slice(0, 24)}`,
    '', `Rispondi per ${ME.address}`, 'Sì   No   Forse   Altre opzioni', '', 'Invito da Google Calendar',
    'Hai ricevuto questa email perché sei un invitato all’evento. Per non ricevere più aggiornamenti su questo evento, rifiuta l’invito.');
  push({
    date: iso(sent), from: LB, to: [ME, IC, PS], subject, text: lines.join('\n'),
    html: '<div itemscope itemtype="http://schema.org/Event"><table><tr><td><h2>' + t + '</h2><p>' + longDate(day) + '</p><a href="https://meet.google.com/abc-defg-hij">Partecipa con Google Meet</a></td></tr></table></div>',
    attachments: [ics], otherParts: ['text/calendar'], headers: calendarHeaders.concat([['Subject', subject]])
  });
}

function recording(sent, title, day) {
  const subject = `Record della riunione: "${title}" ${shortDate(day).split(' ').slice(1).join(' ')}`;
  push({
    date: iso(sent), from: LB, to: [ME, IC], subject,
    text: `La registrazione della riunione «${title}» del ${longDate(day)} è pronta.\n\nLa trovi nella cartella Registrazioni di Meet su Drive dell’organizzatore.\nhttps://drive.google.com/file/d/1AbCdEfGhIjKlMnOp/view\n\nTrascrizione e note automatiche sono nella stessa cartella.`,
    headers: [['Sender', 'Google Meet <meet-recordings-noreply@google.com>'], ['Subject', subject]]
  });
}

function mail(sent, from, to, subject, text, extra = {}) {
  push(Object.assign({ date: iso(sent), from, to, subject, text, headers: [['Subject', subject]] }, extra));
}

// In chronological order.
mail('2026-09-08T18:20', LB, [ME], 'Lanterna', 'Ciao Alex, prima di giovedì ti mando due righe su Lanterna: ci serve qualcuno che metta ordine nei processi operativi prima di scegliere gli strumenti. Ne parliamo con calma.');
invite('2026-09-08T18:31', 'Lanterna', '2026-09-10', '09:15', '10:00');
recording('2026-09-10T10:12', 'Lanterna', '2026-09-10');
invite('2026-09-11T09:02', 'Lanterna (placeholder)', '2026-09-17', '11:00', '12:00');
invite('2026-09-12T16:40', 'Brief Lanterna con Alex', '2026-09-15', '09:00', '09:30');
recording('2026-09-15T09:48', 'Brief Lanterna con Alex', '2026-09-15');

const contract = ++thread;
mail('2026-09-16T11:05', LB, [ME], 'Lanterna, contratto Pineta Forniture', 'Ti giro la bozza del contratto con Pineta Forniture. Guardala con calma, soprattutto l’articolo 4 sulle tempistiche.', { thread: contract, attachments: [{ filename: 'Contratto Lanterna - Pineta Forniture v1.pdf', mimeType: 'application/pdf', size: 312000, inline: false }] });
mail('2026-09-17T09:30', ME, [LB], 'Re: Lanterna, contratto Pineta Forniture', 'Letta. Sull’articolo 4 proporrei 60 giorni invece di 30, il resto mi torna.\n\nIl giorno mer 16 set 2026 alle ore 11:05 Lorenzo Bellini <lorenzo@bellini.esempio.it> ha scritto:\n> Ti giro la bozza del contratto con Pineta Forniture.', { thread: contract });
mail('2026-09-21T15:12', LB, [ME], 'Re: Lanterna, contratto Pineta Forniture', 'Accettato, ecco la versione aggiornata.', { thread: contract, attachments: [{ filename: 'Contratto Lanterna - Pineta Forniture v2.pdf', mimeType: 'application/pdf', size: 318000, inline: false }] });
mail('2026-09-23T10:01', ME, [LB], 'Re: Lanterna, contratto Pineta Forniture', 'Firmato, te lo rimando.', { thread: contract, attachments: [{ filename: 'Contratto Lanterna - Pineta Forniture firmato.pdf', mimeType: 'application/pdf', size: 402000, inline: false }] });

invite('2026-09-21T17:45', 'Orto Comune | Lanterna', '2026-09-24', '11:30', '12:30');
invite('2026-09-23T08:15', 'Aggiornamento operativo', '2026-09-25', '11:00', '11:30');

const dash = ++thread;
mail('2026-09-24T14:00', LB, [ME, PS], 'Analisi cruscotti', 'Pietro ha preparato i primi cruscotti sui tempi delle consegne. Riesci a darci un’occhiata prima di venerdì?', { thread: dash });
mail('2026-09-25T08:40', ME, [LB, PS], 'Re: Analisi cruscotti', 'Visti. Il tempo medio è falsato dalle consegne in sospeso: andrebbero escluse o mostrate a parte.', { thread: dash });

invite('2026-09-26T12:10', 'Lanterna | Giro delle consegne: dall’ordine al saldo', '2026-10-02', '11:00', '12:00');

const slack = ++thread;
mail('2026-09-26T16:00', LB, [ME, IC], 'LANTERNA / Onboard Slack', 'Vi ho invitati nello spazio Slack di Lanterna. Canali principali: #operations, #magazzino, #contratti.', { thread: slack });
mail('2026-09-27T10:20', LB, [ME], 'Re: LANTERNA / Onboard Slack', 'Ilaria mi dice che il canale #magazzino non lo vede, ci guardo.', { thread: slack, cc: [IC] });
mail('2026-09-28T09:05', LB, [IC, ME], 'Re: LANTERNA / Onboard Slack', 'Sistemato, era privato.', { thread: slack });
mail('2026-09-29T19:30', ME, [LB, IC], 'Re: LANTERNA / Onboard Slack', 'Tutto ok anche per me.', { thread: slack });

invite('2026-09-28T11:30', 'Lanterna | Furgoni, turni dei corrieri e fogli condivisi', '2026-10-01', '11:00', '12:00');
invite('2026-09-29T13:00', 'Lanterna | Giro delle consegne: dall’ordine alla fattura', '2026-10-02', '11:00', '12:00', { updated: true });
invite('2026-09-30T09:00', 'Approfondimenti sul magazzino', '2026-10-02', '16:30', '17:30');

const ops = ++thread;
const opsText = [
  [LB, 'Operations', 'Apro questo filo per le cose operative della settimana, così non si perdono tra le riunioni.\n\n1) chi segue le consegne in sospeso\n2) foglio unico per i turni\n3) chiusura mensile'],
  [ME, 'Re: Operations', 'Sulle consegne in sospeso posso fare io una prima ricognizione entro lunedì.'],
  [LB, 'Re: Operations', 'Per i turni Ilaria ha un foglio che usano da due anni, lo condivide su Slack.'],
  [LB, 'Re: Operations', 'Perfetto. Alex, la ricognizione mettila nel documento condiviso.'],
  [ME, 'Re: Operations', 'Fatto: 42 consegne in sospeso, 30 per indirizzi incompleti, 12 in attesa del furgone.'],
  [ME, 'Re: Operations', 'Le 12 in attesa del furgone secondo Pietro si chiudono entro fine mese, se arriva il secondo furgone.'],
  [LB, 'Re: Operations', 'Ottimo, ne parliamo giovedì all’allineamento. Intanto mettete tutto in #operations.']
];
const opsDates = ['2026-09-30T18:00', '2026-10-01T08:30', '2026-10-01T12:15', '2026-10-02T09:00', '2026-10-05T17:45', '2026-10-06T10:10', '2026-10-12T19:05'];
opsText.forEach(([from, subject, text], i) => {
  const to = from === ME ? [LB, IC, PS] : [ME, IC, PS];
  const prev = i ? opsText[i - 1] : null;
  const quoted = prev ? `\n\nIl giorno ${shortDate(opsDates[i - 1].slice(0, 10))} alle ore ${opsDates[i - 1].slice(11)} ${prev[0].name} <${prev[0].address}> ha scritto:\n` + prev[2].split('\n').map((l) => '> ' + l).join('\n') : '';
  mail(opsDates[i], from, to.slice(0, 1), subject, text + quoted, { thread: ops, cc: to.slice(1) });
});

mail('2026-10-01T10:30', LB, [ME], 'Riassutno call Cometa Dati', 'Riassunto veloce della call di stamattina con Cometa Dati: possono integrare la lettura automatica delle bolle, preventivo entro due settimane, serve un caso pilota su 50 consegne.');
mail('2026-10-01T16:20', LB, [ME, GV], 'Lanterna, Magazzino: prima mappa', 'Vi mando la prima mappa dell’Magazzino: chi fa cosa, quali strumenti usano, dove si blocca il lavoro.', { attachments: [{ filename: 'Mappa Magazzino.pdf', mimeType: 'application/pdf', size: 1240000, inline: false }] });
invite('2026-10-01T17:00', 'Lanterna | Punto della settimana sulle priorità operations', '2026-10-02', '17:30', '18:00');
mail('2026-10-02T08:00', LB, [ME], 'Due incontri di Marea a Bologna, 8 e 9 ottobre: vi aspetto', 'Ciao a tutti, l’8 e il 9 ottobre a Bologna ci sono due incontri di Marea dedicati all’innovazione nella pubblica amministrazione. Io ci sarò entrambi i giorni: se passate scrivetemi.\n\nProgramma e iscrizioni: https://marea.esempio.it/bologna', { bcc: [] });
invite('2026-10-02T14:20', 'Lanterna | Giro delle consegne: dall’ordine alla fattura', '2026-10-06', '16:30', '17:30', { updated: true, before: ['2026-10-02', '11:00', '12:00'] });
mail('2026-10-05T11:00', LB, [ME, IC, PS], 'Lanterna | Giro delle consegne: dall’ordine alla fattura', 'In vista di martedì vi mando lo schema del giro delle consegne, dall’ordine alla fattura. Segnate dove secondo voi si perde più tempo.', { attachments: [{ filename: 'Giro delle consegne.pdf', mimeType: 'application/pdf', size: 680000, inline: false }] });
invite('2026-10-07T09:30', 'Lanterna | Giro delle consegne: dall’ordine alla fattura', '2026-10-13', '16:30', '17:30', { updated: true, before: ['2026-10-06', '16:30', '17:30'] });
invite('2026-10-09T15:00', 'Allineamento su piano operativo', '2026-10-15', '15:00', '16:00', { tentative: true });
mail('2026-10-10T09:00', LB, [ME], 'Area soci, il tuo accesso a Lanterna', 'Ti ho dato accesso all’Area soci di Lanterna. Al primo accesso ti chiede di impostare la verifica in due passaggi.\n\nhttps://soci.lanterna.esempio.it/accesso');
invite('2026-10-13T18:10', 'Allineamento su piano operativo', '2026-10-15', '14:30', '15:30', { updated: true, tentative: true, before: ['2026-10-15', '15:00', '16:00'] });

messages.sort((a, b) => a.date.localeCompare(b.date));
const c = {
  id: 'inviti-calendario',
  title: 'Un collega con 39 messaggi divisi in 26 «argomenti», quasi tutti inviti di calendario, spostamenti e registrazioni di riunioni',
  report: 'segnalazione del 2026-10-14 dalla vista Persone con i dati veri (forma del caso descritta dall’utente)',
  messages
};
writeFileSync(join(HERE, 'inviti-calendario.json'), JSON.stringify(c, null, 2) + '\n');
console.log(`inviti-calendario.json: ${messages.length} messaggi`);
