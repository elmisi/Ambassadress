// Builds servizi-automatici.json: a large mix of automated senders like a real working mailbox
// (code review, issue tracker, CI, security alerts, receipts, parcels, newsletters, social).
// Shape reported on 2026-10-14 (Servizi e notifiche: hundreds of messages, hundreds of "topics");
// every service, repository, key and person here is made up.
//
//   node mockups/data/cases/make-servizi.mjs

import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ME = { name: 'Alex Moro', address: 'alex.moro@esempio.it' };
const NOW = new Date('2026-10-14T12:00:00+02:00');

let seed = 7;
const rnd = () => { seed = (seed * 16807) % 2147483647; return seed / 2147483647; };
const at = (daysAgo, h, m) => { const d = new Date(NOW); d.setDate(d.getDate() - daysAgo); d.setHours(h, m, 0, 0); return d.toISOString(); };

let n = 0;
const messages = [];
function note(from, subject, text, d, opts = {}) {
  n++;
  messages.push({
    id: 'case-servizi-' + n,
    date: at(d, opts.h ?? 7 + Math.floor(rnd() * 14), opts.m ?? Math.floor(rnd() * 60)),
    from, to: [ME], cc: [], subject, text: opts.html ? null : text, html: opts.html || null,
    messageId: `<case-servizi-${n}@esempio.it>`, references: [],
    attachments: opts.att || [],
    headers: [['Subject', subject]].concat(opts.headers || [['List-Unsubscribe', `<https://${from.address.split('@')[1]}/unsubscribe>`]]),
    flags: { seen: opts.unread ? false : true, flagged: false },
    gmail: { threadId: 'case-servizi-' + (opts.thread || n), labels: ['\\Inbox'].concat(opts.labels || ['CATEGORY_UPDATES']) },
    problems: []
  });
}

// Code review notifications: several pull requests, 1-4 notifications each.
const REPOHUB = { name: 'Repohub', address: 'notifications-noreply@repohub.esempio.it' };
const PEOPLE = ['Marta Riva', 'Davide Greco', 'Sofia Ricci', 'Luca Fontana'];
const PRS = [
  ['fornoverde/ricettario', 24, 'RP-213 refresh onboarding guide', ['opened', 'commented', 'approved']],
  ['fornoverde/turni', 16, 'RP-199: opening-hours specification', ['opened']],
  ['fornoverde/negozio', 380, 'WS-207 checkout cleanup v3 - step 1 (rebase)', ['opened', 'commented', 'commented']],
  ['fornoverde/ricettario', 21, 'RP-203 recipe import from spreadsheet', ['opened']],
  ['fornoverde/giri', 54, 'RP-206 add half-size loaf', ['opened', 'commented', 'approved', 'merged']],
  ['fornoverde/giri', 53, 'feat: a shop with only one oven picks that oven', ['opened', 'commented', 'approved', 'merged']],
  ['fornoverde/turni-api', 46, 'RP-206 add half-size loaf', ['opened', 'approved', 'commented', 'merged']],
  ['fornoverde/turni-api', 45, 'RP-205 add allergen fields', ['opened', 'merged']],
  ['fornoverde/giri', 52, 'RP-205 add allergen fields', ['opened', 'approved']],
  ['fornoverde/giri', 50, 'RP-177 frontend menu from planner proposals', ['opened', 'commented', 'approved']]
];
const VERB = { opened: 'ha aperto la pull request', commented: 'ha commentato la pull request', approved: 'ha approvato la pull request', merged: 'ha unito la pull request' };
PRS.forEach(([repo, num, title, events], i) => {
  const start = 40 - i * 3;
  events.forEach((ev, j) => {
    const who = PEOPLE[(i + j) % PEOPLE.length];
    const subject = `[Repohub] Pull request #${num}: ${title} (${repo})`;
    let text = `${who} ${VERB[ev]} #${num} «${title}» in ${repo}.\n\nVisualizza: https://repohub.esempio.it/${repo}/pull-requests/${num}`;
    if (ev === 'commented') text += `\n\nCommento:\n> ${['Manca un test per il caso limite.', 'Rinominerei questa variabile.', 'Ok per me, solo un dubbio sul nome della colonna.'][j % 3]}`;
    note(REPOHUB, subject, text, Math.max(0, start - j * 2), { thread: 'pr-' + repo + num, headers: [['X-Repohub-Event', 'pullrequest:' + ev]] });
  });
});
note(REPOHUB, '[Repohub] Pipeline for main failed on ae83fe6 (fornoverde/ricettario)', 'La pipeline di main è fallita al passo «test».\n\nCommit ae83fe6 di Marta Riva.\nhttps://repohub.esempio.it/fornoverde/ricettario/pipelines/812', 6);

// Issue tracker: one issue with five updates, one with two.
const TRACKER = { name: 'Tracker', address: 'tracker@fornoverde.esempio.it' };
['Creata da Marta Riva', 'Stato: Da fare → In corso', 'Commento di Sofia Ricci: le proposte arrivano già ordinate dal pianificatore.', 'Stato: In corso → In revisione', 'Collegata alla pull request #50'].forEach((t, j) => {
  note(TRACKER, '[TRACKER] (RP-177) Frontend: menu from planner proposals', t + '\n\nhttps://fornoverde.tracker.esempio.it/browse/RP-177', 30 - j * 5, { thread: 'rp177' });
});
['Creata da Davide Greco', 'Stato: Da fare → In corso'].forEach((t, j) => {
  note(TRACKER, '[TRACKER] (RP-205) Allergen fields', t + '\n\nhttps://fornoverde.tracker.esempio.it/browse/RP-205', 20 - j * 6, { thread: 'rp205' });
});

// CI runs on a personal project.
const FORGE = { name: 'Gitforge', address: 'noreply@gitforge.esempio.it' };
note(FORGE, '[alexmoro/orto] PR run failed: Build - semina (0118015)', 'Build fallita sul ramo semina.\nCommit 0118015.', 4);
note(FORGE, '[alexmoro/orto] PR run failed: QA - semina (0118015)', 'QA fallito sul ramo semina.\nCommit 0118015.', 4);
note(FORGE, '[alexmoro/orto] Run failed: QA - qa/prove-base (0118015)', 'Esecuzione fallita: qa/prove-base.\nCommit 0118015.', 3);
note(FORGE, '[Gitforge] A new SSH authentication public key was added to your account', 'A new SSH key was added to your account. If you did not add it, remove it now.', 26, { headers: [], labels: ['CATEGORY_UPDATES'] });
note(FORGE, '[Gitforge] A new SSH authentication public key was added to your account', 'A new SSH key was added to your account. If you did not add it, remove it now.', 9, { headers: [], unread: true });

// Security alerts.
const ACCOUNTS = { name: 'Account Esempio', address: 'no-reply@accounts.esempio.it' };
[52, 41, 33, 19, 8, 1].forEach((d, j) => note(ACCOUNTS, 'Avviso di sicurezza', `È stato effettuato un nuovo accesso al tuo account da ${['Chrome su Linux', 'un iPhone', 'Firefox su Windows', 'Chrome su Linux', 'un telefono Android', 'Safari su Mac'][j]}. Se sei stato tu, non devi fare nulla.`, d, { headers: [], unread: d < 2 }));
const ASSIST = { name: 'Assistente', address: 'no-reply@assistente.esempio.it' };
note(ASSIST, 'Security alert: new passkey added to your Assistente account', 'A new passkey was added to your account. If this wasn’t you, secure your account now.', 12, { headers: [] });
note(ASSIST, 'Security alert: new trusted device added to your Assistente account', 'A new trusted device was added to your account.', 12, { headers: [] });
note(ASSIST, 'Security alert: new trusted device added to your Assistente account', 'A new trusted device was added to your account.', 2, { headers: [], unread: true });
note(ASSIST, '$100 USD in API credits was added', 'Your organization received $100 USD in API credits.', 15);
note({ name: 'Studio Immobiliare Esempio', address: 'verify@identicheck.esempio.it' }, 'Studio Immobiliare Esempio is inviting you to complete an Identity Check', 'Studio Immobiliare Esempio has requested that you verify your identity. Click below to start.\n\nhttps://identicheck.esempio.it/start/8f2c', 5, { headers: [] });

// Payments, parcels, purchases.
const PAGA = { name: 'PagaFacile', address: 'ricevute@pagafacile.esempio.it' };
note(PAGA, 'Ricevuta transazione PagaFacile', 'Pagamento di 24,90 € a Ferramenta Centrale. Carta **** 4821.', 18, { att: [{ filename: 'Ricevuta 0918.pdf', mimeType: 'application/pdf', size: 42000, inline: false }] });
note(PAGA, 'Ricevuta transazione PagaFacile', 'Pagamento di 7,50 € a Bar Stazione. Carta **** 4821.', 6, { att: [{ filename: 'Ricevuta 1008.pdf', mimeType: 'application/pdf', size: 41000, inline: false }] });
const SHOP = { name: 'Negozio Esempio', address: 'spedizioni@negozio.esempio.it' };
note(SHOP, 'Pacco pronto per il ritiro: Locker Esempio - Lavanderia Rapida Via [indirizzo] TO, ritiro entro il 14 ottobre', 'Il tuo pacco è nel locker. Codice di ritiro: 482 913. Ritira entro il 14 ottobre.', 3, { unread: true });
note(SHOP, 'Ritirato: Batterie a bottone al litio CR2032 (Confe...', 'Hai ritirato il pacco. Grazie!', 2);
note({ name: 'Borsa Esempio', address: 'comunicazioni@borsa.esempio.it' }, 'Aggiornamento dei termini e condizioni del sito Borsa Esempio', 'Dal 1° novembre entrano in vigore i nuovi termini e condizioni del sito.', 11);
note({ name: 'Servizio Esempio', address: 'hello@servizio.esempio.it' }, 'Confirm Your Signup', 'Follow this link to confirm your user: https://servizio.esempio.it/confirm/7d1e', 23);
note({ name: 'Dott.ssa Elisa Marchi', address: 'segreteria@studiomarchi.esempio.it' }, 'Dott.ssa Elisa Marchi: informazioni dallo studio', 'Lo studio resterà chiuso dal 2 al 6 novembre. Per urgenze chiamare la guardia medica.', 7);

// Newsletters and social.
const NEWS = [
  [{ name: 'Nova AI', address: 'news@nova.esempio.it' }, 'Early access: Try Nova 4 Argon today'],
  [{ name: 'Toolkit Weekly', address: 'weekly@toolkit.esempio.it' }, 'toolkit 2.0 + operation deepcode phase 3'],
  [{ name: 'Toolkit Weekly', address: 'weekly@toolkit.esempio.it' }, 'toolkit 1.9: faster builds'],
  [{ name: 'Nova AI', address: 'news@nova.esempio.it' }, 'What’s new in Nova this month']
];
NEWS.forEach(([from, subject], i) => note(from, subject, null, 4 + i * 9, {
  labels: ['CATEGORY_PROMOTIONS'],
  html: `<html><body><h1>${subject}</h1><p>Le novità di questa settimana, in breve.</p><ul><li>Primo punto</li><li>Secondo punto</li></ul><a href="https://${from.address.split('@')[1]}/leggi">Leggi</a></body></html>`
}));
const RETE = { name: 'Rete Pro', address: 'notifiche@retepro.esempio.it' };
note(RETE, 'Visualizza il post di Sara Arrighi e i tuoi prossimi passi', 'Sara Arrighi ha pubblicato un post che potrebbe interessarti.', 10, { labels: ['CATEGORY_SOCIAL'] });
note(RETE, 'Hai 3 nuove richieste di collegamento', 'Marta Riva e altre 2 persone vogliono collegarsi con te.', 17, { labels: ['CATEGORY_SOCIAL'] });

messages.sort((a, b) => a.date.localeCompare(b.date));
writeFileSync(join(HERE, 'servizi-automatici.json'), JSON.stringify({
  id: 'servizi-automatici',
  title: 'Tante notifiche automatiche di tipo diverso: revisione codice, issue, build, sicurezza, ricevute, pacchi, newsletter, social',
  report: 'segnalazione del 2026-10-14 sulla voce Servizi e notifiche (forma descritta dall’utente)',
  messages
}, null, 2) + '\n');
console.log(`servizi-automatici.json: ${messages.length} messaggi`);
