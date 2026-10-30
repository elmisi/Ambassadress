// Builds the cases for the tests of round v15 (app/NOTE.md, «Prove da fare»):
//   da-ritrovare.json        prova 1: things to find again (a decision in the middle of a long
//                            thread, a document shared through Drive, an old message, a date in
//                            the text, short codes, odd file names)
//   thread-difficili.json    prova 2: a subject change inside the same thread, a thread that
//                            ends with a question to me, one that ends with my question
//   allegati-nascosti.json   prova 3: attachments that look like decorations
//   pulizia-importanti.json  prova 6: important text-only service mail inside the cleanup groups,
//                            the three exclusions, repetitive invoices for the cleanup that learns
//   indirizzi-condivisi.json prova 7: Jira and similar (one address, many people), homonyms,
//                            a shortened name, a changing name, a shared company mailbox
// People, companies, addresses and texts are made up.
//
//   node mockups/data/cases/make-prove-v15.mjs
//
// Replies carry "quote": true: generate-synthetic.mjs adds the quote of the previous message of
// the thread, in the format of the sender's program (from.client).

import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ME = { name: 'Alex Moro', address: 'alex.moro@esempio.it', client: 'gmail-it' };
const ME2 = { name: 'Alex Moro', address: 'alex@studiomoro.esempio.it', client: 'thunderbird' };
const SIG_ME2 = '\n\n-- \nAlex Moro\nStudio Moro – progettazione d’interni\nwww.studiomoro.esempio.it';

// "2026-09-28 09:10" in local time, as the generator's at()
const iso = (s) => new Date(s.replace(' ', 'T') + ':00').toISOString();
const MIME = { pdf: 'application/pdf', png: 'image/png', jpg: 'image/jpeg', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' };
const att = (filename, size, more) => Object.assign({ filename, mimeType: MIME[filename.split('.').pop().toLowerCase()] || 'application/octet-stream', size, inline: false }, more || {});
// signatures as people write them; iPhone adds its own line
const sign = (p) => (p.client === 'iphone' ? '\n\nInviato da iPhone' : p.sig ? (p.sigDash === false ? '\n\n' : '\n\n-- \n') + p.sig : '');
const P = (name, address, client, sig, more) => Object.assign({ name, address, client, sig }, more || {});
const who = (p) => (p.client ? { name: p.name, address: p.address, client: p.client } : { name: p.name, address: p.address });

function build(id) {
  let n = 0;
  const messages = [];
  // one Gmail thread: Message-ID, In-Reply-To, References, "Re: " and the labels
  function thread(key, subject, entries, opts = {}) {
    const refs = [];
    let prev = null;
    entries.forEach((e, i) => {
      n++;
      const me = opts.me || ME;
      const from = e.from || me;
      const mine = from.address === me.address;
      const messageId = `<case-${id}-${n}@${from.address.split('@')[1]}>`;
      const text = e.text == null ? null : e.text + (e.sign === false ? '' : mine ? (me === ME2 ? SIG_ME2 : '\n\nAlex') : sign(from));
      const m = {
        date: iso(e.at),
        from: who(from),
        to: (e.to || (mine ? [opts.with] : [me])).map(who),
        cc: (e.cc || []).map(who),
        subject: e.subject || (i === 0 ? subject : (opts.prefix ?? 'Re: ') + (e.topic || subject)),
        messageId,
        inReplyTo: prev,
        references: refs.slice(),
        text,
        html: e.html || null,
        attachments: e.att || [],
        headers: [['Subject', e.subject || subject]].concat(e.headers || opts.headers || []),
        flags: { seen: !e.unread, flagged: !!e.flagged },
        gmail: { threadId: `case-${id}-${key}`, labels: mine ? ['\\Sent'] : ['\\Inbox'].concat(e.labels || opts.labels || []) }
      };
      m.headers[0][1] = m.subject;
      if (e.replyTo) m.replyTo = e.replyTo.map(who);
      if (i > 0 && e.quote !== false && text != null) m.quote = true;
      messages.push(m);
      refs.push(messageId);
      prev = messageId;
    });
  }
  return { thread, messages };
}

function write(id, title, report, messages) {
  messages.sort((a, b) => a.date.localeCompare(b.date));
  writeFileSync(join(HERE, id + '.json'), JSON.stringify({ id, title, report, messages }, null, 2) + '\n');
  console.log(`${id}.json: ${messages.length} messaggi`);
}

/* ---------- prova 1: da ritrovare ---------- */
{
  const { thread, messages } = build('da-ritrovare');
  const valerio = P('Valerio Pace', 'valerio@pacedesign.esempio.it', 'gmail-it', 'Valerio Pace\nPace Design – siti e identità visiva\n06 5551 2290');
  const teo = P('Teo Marras', 'teo@pacedesign.esempio.it', 'thunderbird', 'Teo', { sigDash: false });
  thread('sito', 'Sito dello studio', [
    { at: '2026-09-01 10:20', from: valerio, text: 'Ciao Alex,\n\ncome d’accordo ti mando la prima proposta: tre pagine (lo studio, i lavori, i contatti) più il blog. Le bozze grafiche sono nel PDF, la palette è quella del biglietto da visita.\n\nDimmi cosa ne pensi, con calma.', att: [att('Proposta sito Studio Moro.pdf', 1150000)] },
    { at: '2026-09-02 08:45', text: 'Bella, mi piace soprattutto la pagina dei lavori con le foto grandi.\n\nSul blog ho dei dubbi: negli ultimi due anni ho scritto tre articoli. Ha senso tenerlo?' },
    { at: '2026-09-02 12:10', from: valerio, text: 'Dipende da quanto vuoi scriverci: un blog fermo da un anno fa più danno che altro.\n\nTi chiedo anche un’altra cosa, già che ci siamo: il tuo hosting attuale scade a fine novembre. Vuoi restare dove sei o cambiamo?' },
    { at: '2026-09-03 19:30', text: 'Che alternative ci sono? Quello di adesso mi costa 180 euro l’anno e il pannello è lentissimo.' },
    { at: '2026-09-04 09:05', from: valerio, cc: [teo], text: 'Io mi trovo bene con Nuvola Esempio: 96 euro l’anno, copie giornaliere, certificato compreso. Teo ci ha spostato altri tre clienti quest’anno senza problemi, lo metto in copia.' },
    { at: '2026-09-04 18:40', to: [valerio, teo], text: 'Va bene, allora facciamo così: il dominio resta quello di adesso, dal 1° dicembre passiamo a Nuvola Esempio e il blog lo chiudiamo. I tre articoli li sposto nella pagina dei lavori, come schede dei progetti.\n\nTeo, mi dici tu cosa ti serve per il trasferimento?' },
    { at: '2026-09-05 11:15', from: teo, to: [ME], cc: [valerio], text: 'Ciao Alex,\nmi servono le credenziali del pannello attuale e il codice di trasferimento del dominio. Si chiama EPP o authcode, lo chiedi al vecchio fornitore dall’area clienti.' },
    { at: '2026-09-07 08:30', to: [teo], cc: [valerio], text: 'Il codice l’ho chiesto, dicono che arriva in cinque giorni lavorativi. Le credenziali te le mando a parte, non per email.' },
    { at: '2026-09-08 16:50', from: valerio, cc: [teo], text: 'Perfetto. Intanto vado avanti con le schede dei progetti: ti mando una bozza la settimana prossima.', att: [att('Schede progetti, bozza.pdf', 640000)] }
  ], { with: valerio });

  // A document shared by a person through a service, then the same person's comment, then the person herself.
  const ferraro = P('Giulia Ferraro', 'giulia.ferraro@solaria-impianti.esempio.it', 'gmail-it', 'Giulia Ferraro\nSolaria Impianti – fotovoltaico e accumulo\ncell. 340 555 0761', { sigDash: false });
  thread('drive', 'Documento condiviso con te: "Capitolato impianto fotovoltaico rev. B"', [
    { at: '2026-09-17 15:02', from: { name: 'Giulia Ferraro (tramite Google Drive)', address: 'drive-shares-dm-noreply@google.com' }, replyTo: [ferraro], headers: [['Reply-To', 'Giulia Ferraro <giulia.ferraro@solaria-impianti.esempio.it>']], text: 'Giulia Ferraro (giulia.ferraro@solaria-impianti.esempio.it) ha condiviso il seguente documento:\n\nCapitolato impianto fotovoltaico rev. B\n\n«Ciao Alex, ecco il capitolato aggiornato con le due falde e l’accumulo da 10 kWh. Guardalo con calma prima del sopralluogo.»\n\nApri\nhttps://docs.google.com/document/d/1sinteticoCapitolatoRevB0000/edit?usp=sharing_eil\n\nGoogle LLC\nHai ricevuto questa email perché giulia.ferraro@solaria-impianti.esempio.it ha condiviso un documento con te da Google Drive.' }
  ]);
  thread('commento', 'Capitolato impianto fotovoltaico rev. B: nuovo commento', [
    { at: '2026-09-21 18:20', from: { name: 'Giulia Ferraro via Google Documenti', address: 'comments-noreply@docs.google.com' }, replyTo: [ferraro], text: 'Giulia Ferraro ha aggiunto un commento al documento\n\nCapitolato impianto fotovoltaico rev. B\n\nTesto commentato: «Inverter ibrido 6 kW»\n\nGiulia Ferraro: Alex, qui ho messo l’inverter da 6 kW. Se preferisci quello da 8 kW dimmelo entro venerdì: il prezzo sale di circa 700 euro, ma resta margine per i pannelli sul garage.\n\nRispondi   Apri\n\nGoogle LLC\nHai ricevuto questa email perché sei menzionato nei commenti o hai accesso al documento.' }
  ]);
  thread('sopralluogo', 'Sopralluogo per il fotovoltaico', [
    { at: '2026-09-15 09:30', from: ferraro, text: 'Buongiorno Alex,\n\nconfermo il sopralluogo sul tetto per martedì 22 alle 10. Mi servono le chiavi della botola e, se le trova, le bollette dell’ultimo anno: con i consumi veri dimensioniamo l’accumulo.\n\nA martedì' },
    { at: '2026-09-15 13:05', text: 'Perfetto, martedì alle 10. Le bollette le preparo, la botola la apre il portiere.' }
  ], { with: ferraro });

  // A message from years ago: memorable content, a sender seen only that time.
  const villa = P('Ermanno Villa', 'ermanno.villa@rifugioalpe.esempio.it', 'apple', 'Ermanno\nRifugio Alpe Esempio, 1.870 m', { sigDash: false });
  thread('ricetta', 'Grazie della visita', [
    { at: '2023-01-22 17:40', from: villa, text: 'Caro Alex,\n\ngrazie ancora della bella giornata, è raro avere ospiti che salgono con la neve fresca.\n\nCome promesso ecco la ricetta dei pizzoccheri della Rina, per quattro persone:\n- 300 g di farina di grano saraceno e 100 g di farina bianca, acqua quanto basta, si stende a 3 mm e si taglia a strisce larghe un dito;\n- due patate e mezza verza a pezzi, a bollire insieme ai pizzoccheri per dieci minuti;\n- a strati nella pirofila con formaggio d’alpeggio a scaglie e grana;\n- sopra burro fuso con salvia e uno spicchio d’aglio, fatto imbiondire piano.\n\nLa Rina dice che il segreto è non avere fretta con il burro.\n\nUn abbraccio, e tornate in estate' },
    { at: '2023-01-23 08:15', text: 'Grazie Ermanno! La proviamo domenica, il burro piano piano. Un saluto alla Rina, torniamo di sicuro.' }
  ], { with: villa });

  // A notice that arrives in September and talks about a date in October.
  const orti = P('Associazione Orti del Parco', 'segreteria@ortidelparco.esempio.it', 'outlook', 'La segreteria\nAssociazione Orti del Parco APS\nsegreteria@ortidelparco.esempio.it', { sigDash: false });
  thread('orti', 'Convocazione assemblea dei soci', [
    { at: '2026-09-16 21:10', from: orti, to: [{ name: 'Soci Orti del Parco', address: 'soci@ortidelparco.esempio.it' }], text: 'Cari soci,\n\nl’assemblea ordinaria è convocata per lunedì 12 ottobre alle 20:30 nella sala della parrocchia, in seconda convocazione.\n\nOrdine del giorno:\n1) bilancio 2025-2026;\n2) rinnovo del consiglio direttivo;\n3) turni dell’acqua per la prossima estate;\n4) quota associativa 2027.\n\nChi non può venire può delegare un altro socio con il modulo allegato, da consegnare firmato all’inizio dell’assemblea.', att: [att('Ordine del giorno assemblea 12-10-2026.pdf', 120000), att('delega_socio_2026.pdf', 64000)] }
  ]);

  // Short codes: a case number and a protocol, a CIG, a customer code; odd file names.
  thread('pratica', 'Comunicazione di avvio del procedimento', [
    { at: '2026-08-27 11:40', from: P('Comune di Roma - Sportello Edilizia', 'sportello.edilizia@comune.esempio.it', 'outlook', 'Sportello Unico per l’Edilizia\nMunicipio VIII – Ufficio tecnico\nNon rispondere a questo indirizzo per le integrazioni: usare il portale.', { sigDash: false }), text: 'Oggetto: CILA prot. n. 88213 del 20/08/2026 – pratica n. 2026/0417 – immobile in Via Ostiense 12, interno 9.\n\nSi comunica l’avvio del procedimento relativo alla pratica in oggetto. Responsabile del procedimento: geom. Rita Sereni. Eventuali integrazioni vanno caricate sul portale indicando il numero di pratica.\n\nDistinti saluti', att: [att('Avvio procedimento prot. 88213.pdf', 150000)] }
  ]);
  const gare = P('Consorzio di Bonifica Esempio – Ufficio gare', 'gare@consorziobonifica.esempio.it', 'outlook', 'Ufficio gare e contratti\nConsorzio di Bonifica Esempio\nPEC: gare@pec.consorziobonifica.esempio.it', { sigDash: false });
  thread('cig', 'Richiesta di preventivo – arredi sala consiliare', [
    { at: '2026-09-09 12:30', from: gare, to: [ME2], text: 'Spett.le Studio Moro,\n\nsi chiede un preventivo per la progettazione e la fornitura degli arredi della sala consiliare, come da capitolato allegato. Affidamento diretto ai sensi dell’art. 50 del d.lgs. 36/2023. CIG: B4A7E2915C.\n\nIl preventivo va inviato entro il 30 settembre, indicando il CIG nell’oggetto.\n\nDistinti saluti', att: [att('Capitolato_arredi_sala_consiliare_DEF(2).pdf', 420000)] },
    { at: '2026-09-10 09:00', from: ME2, to: [gare], text: 'Buongiorno,\n\nricevuto, grazie. Il preventivo arriva entro il 25 settembre, con il CIG nell’oggetto come indicato.\n\nCordiali saluti' }
  ], { me: ME2, with: gare });
  thread('gas', 'Variazione delle condizioni economiche dal 1° novembre', [
    { at: '2026-09-25 07:15', from: { name: 'Gas Esempio', address: 'servizioclienti@gasesempio.example' }, headers: [['Auto-Submitted', 'auto-generated']], labels: ['CATEGORY_UPDATES'], text: 'Gentile Alex Moro,\n\ncodice cliente 7730021 – PDR 00348812447102\n\ndal 1° novembre il prezzo della materia prima della sua offerta cambia come indicato nella tabella sotto. Se non è d’accordo può recedere senza costi entro il 31 ottobre, indicando il codice cliente.\n\nPrezzo attuale: 0,41 €/Smc\nNuovo prezzo: 0,47 €/Smc\n\nServizio clienti Gas Esempio' }
  ]);
  const mancuso = P('Carla Mancuso', 'carla.mancuso@esempio.it', 'iphone', '');
  thread('planimetria', 'La planimetria che mi avevi chiesto', [
    { at: '2026-09-19 13:20', from: mancuso, text: 'Eccola, l’ho scansionata ieri in copisteria. Il nome del file è quello che dà la macchina, scusa.', att: [att('SCN_0047_20260918_planim-cat-fg812-part77-sub5.pdf', 380000)] },
    { at: '2026-09-19 20:05', text: 'Grazie Carla, è proprio quella del catasto. Te la riporto sabato.' }
  ], { with: mancuso });

  write('da-ritrovare', 'Cose da ritrovare partendo dai ricordi: una decisione a metà di un thread lungo, un documento da Drive, un messaggio del 2023, una data nel testo diversa da quella del messaggio, codici brevi e nomi di file strani',
    'giro v15, prova 1 di app/NOTE.md (ritrovamento); le domande sono in mockups/data/ritrovamento.json', messages);
}

/* ---------- prova 2: thread difficili ---------- */
{
  const { thread, messages } = build('thread-difficili');
  const irene = P('Irene Valli', 'irene.valli@esempio.it', 'apple', 'Irene');
  const bruno = P('Bruno Carli', 'bruno.carli@esempio.it', 'gmail-en', 'B.', { sigDash: false });
  // The subject changes in the fourth message; the thread stays the same (References, In-Reply-To, threadId).
  thread('umbria', 'Weekend in Umbria', [
    { at: '2026-09-24 21:15', from: irene, cc: [bruno], text: 'Ciao Alex,\n\nallora per il weekend del 10 ottobre pensavamo a Todi: una notte in agriturismo, sabato giro in centro e domenica la cascata. Vi va?' },
    { at: '2026-09-25 08:20', to: [irene], cc: [bruno], text: 'Ci stiamo! Prenotate voi l’agriturismo? A noi basta una camera.' },
    { at: '2026-09-25 13:40', from: bruno, to: [ME, irene], text: 'L’agriturismo di Todi è pieno, ho chiamato stamattina.' },
    { at: '2026-09-25 18:05', from: bruno, to: [ME, irene], subject: 'Umbria: si cambia, andiamo a Spello', text: 'Cambio di programma: ho trovato posto a Spello, nella casa con l’uliveto che vi avevo fatto vedere. Due camere, 85 euro a camera con la colazione. Domenica possiamo andare a Bevagna invece della cascata. Confermo?' },
    { at: '2026-09-25 19:30', from: irene, to: [bruno, ME], topic: 'Umbria: si cambia, andiamo a Spello', text: 'Spello è anche meglio! Per me sì.' },
    { at: '2026-09-26 09:10', to: [bruno, irene], topic: 'Umbria: si cambia, andiamo a Spello', text: 'Anche per noi, conferma pure. Partiamo sabato verso le 9?' },
    { at: '2026-09-26 10:02', from: bruno, to: [ME, irene], topic: 'Umbria: si cambia, andiamo a Spello', text: 'Confermato, caparra pagata. Alle 9 sotto casa vostra.' }
  ], { with: irene });

  // Ends with a question to me, unread.
  const bruni = P('Fabrizio Bruni', 'fabrizio@bruniarredi.esempio.it', 'outlook', 'Fabrizio Bruni\nBruni Arredi – falegnameria su misura\nVia dei Platani 31, Roma · 06 5552 8814', { sigDash: false });
  thread('tavolo', 'Tavolo su misura per la sala', [
    { at: '2026-10-05 10:30', from: bruni, text: 'Buongiorno Alex,\n\ncome d’accordo le mando il disegno del tavolo in rovere: gambe a cavalletto, piano da 4 cm, finitura a olio. Il prezzo è nel disegno, in basso a destra.', att: [att('Tavolo sala, disegno.pdf', 230000)] },
    { at: '2026-10-06 21:10', text: 'Grazie Fabrizio, molto bello. Il cliente vuole sentire anche mia moglie prima di decidere la lunghezza: le facciamo sapere in settimana.' },
    { at: '2026-10-09 09:45', from: bruni, text: 'Nessun problema. Le ricordo solo che il rovere va ordinato entro la fine della prossima settimana, altrimenti si va a dicembre.' },
    { at: '2026-10-13 17:20', from: bruni, unread: true, text: 'Buonasera Alex,\n\nil fornitore mi ha chiesto oggi l’ordine definitivo. Mi conferma entro venerdì la lunghezza del piano, 220 o 240 cm? Senza la misura non posso ordinare il legno.\n\nGrazie' }
  ], { with: bruni });

  // Ends with my question, a week before the last message of the mailbox: «aspetti una risposta».
  const claudia = P('Claudia Mancini', 'claudia.mancini@esempio.it', 'iphone', '');
  thread('cantina', 'Cantina in affitto', [
    { at: '2026-10-01 19:40', from: claudia, text: 'Ciao Alex, Lucia mi ha detto che cerchi una cantina. La mia in via Ostiense è libera da novembre: 12 metri quadri, asciutta, con la luce. 60 euro al mese.' },
    { at: '2026-10-02 08:30', text: 'Ciao Claudia, grazie! Mi interessa molto. Si arriva con la macchina fino alla porta? Devo portarci dei mobili.' },
    { at: '2026-10-05 12:15', from: claudia, text: 'Sì, la rampa arriva fino al corridoio delle cantine, la mia è la terza. Se vuoi passa a vederla quando vuoi, le chiavi le ha il portiere.' },
    { at: '2026-10-07 18:50', text: 'L’ho vista stamattina, è perfetta. Ti va bene se il contratto parte dal 1° novembre, così ho il tempo di svuotare la mia? E il deposito lo preferisci con un bonifico?' }
  ], { with: claudia });

  write('thread-difficili', 'Thread difficili: l’oggetto cambia a metà nello stesso thread, un thread finisce con una domanda a me, uno con una mia domanda rimasta senza risposta',
    'giro v15, prova 2 di app/NOTE.md (la conversazione ricostruita)', messages);
}

/* ---------- prova 3: allegati che sembrano decorazioni ---------- */
{
  const { thread, messages } = build('allegati-nascosti');
  const michela = P('Michela Rota', 'michela.rota@esempio.it', 'iphone', '');
  thread('qr', 'Fiera dell’artigianato, sabato', [
    { at: '2026-10-08 20:10', from: michela, text: 'Ciao Alex! In allegato il QR per l’ingresso di sabato, va mostrato all’entrata dal telefono. Ci vediamo alle 10 davanti alla biglietteria.', att: [att('qrcode.png', 3900, { width: 200, height: 200, code: 'QR-Code' })] },
    { at: '2026-10-08 20:31', text: 'Grazie! A sabato, alle 10.' }
  ], { with: michela });
  thread('modulo', 'Corso di ceramica, il modulo', [
    { at: '2026-09-29 18:00', from: michela, text: 'Ecco il modulo per il corso di ceramica, va firmato e riportato alla prima lezione.', att: [att('documento.pdf', 96000)] }
  ]);

  // A screenshot pasted in the text: an inline image with a Content-ID, small but useful.
  const ettore = P('Ettore Bonfanti', 'ettore.bonfanti@lavoro.esempio.it', 'gmail-it', 'Ettore Bonfanti\nAmministrazione | Lavoro Esempio S.r.l.');
  thread('schermata', 'Errore nel foglio dei rimborsi', [
    { at: '2026-10-09 11:25', from: ettore, text: 'Ciao Alex,\n\nquando apro il secondo foglio dei rimborsi mi esce questo:\n\n[image: image.png]\n\nSuccede solo a me? Prima di chiamare l’assistenza volevo sentire te.', html: '<div dir="ltr">Ciao Alex,<div><br></div><div>quando apro il secondo foglio dei rimborsi mi esce questo:</div><div><br></div><div><img src="cid:ii_mg7k2x4a0" alt="image.png" width="280" height="170"><br></div><div><br></div><div>Succede solo a me? Prima di chiamare l’assistenza volevo sentire te.</div><div><br></div>-- <br><div dir="ltr">Ettore Bonfanti<br>Amministrazione | Lavoro Esempio S.r.l.</div></div>', att: [att('image.png', 21500, { inline: true, contentId: 'ii_mg7k2x4a0', width: 280, height: 170 })] },
    { at: '2026-10-09 12:02', text: 'Succede anche a me, ma solo con il file vecchio: usa quello di ottobre, nella cartella condivisa.' }
  ], { with: ettore });

  // A narrow scan: the barcode of a payment slip, wide and low.
  const patrizia = P('Patrizia Fumagalli', 'patrizia.fumagalli@esempio.it', 'outlook', 'Patrizia Fumagalli\nTesoriera – Coro Voci del Quartiere', { sigDash: false });
  thread('bollettino', 'Quota del coro', [
    { at: '2026-10-06 22:05', from: patrizia, text: 'Ciao Alex,\n\nper la quota del coro ti mando la scansione del codice a barre del bollettino: si paga anche dal tabaccaio, facendolo leggere alla cassa. Sono 90 euro e scade il 31 ottobre.\n\nGrazie!', att: [att('scansione.jpg', 47000, { width: 900, height: 100, code: 'EAN-13' })] }
  ]);

  // The same document in three messages of the same thread; "documento.pdf" again, a different file.
  const fusco = P('Renato Fusco', 'renato.fusco@cooperativa-casa.esempio.it', 'outlook', 'Renato Fusco\nPresidente – Cooperativa Casa Comune', { sigDash: false });
  const soci = [P('Nora Piras', 'nora.piras@esempio.it', 'gmail-it', 'Nora'), P('Walter Cuomo', 'walter.cuomo@esempio.it', 'gmail-it', '')];
  const statuto = () => att('Statuto cooperativa, bozza.pdf', 310000, { fileKey: 'statuto-cooperativa' });
  thread('statuto', 'Bozza dello statuto', [
    { at: '2026-09-21 09:00', from: fusco, text: 'Buongiorno Alex,\n\nti mando la bozza dello statuto che ha preparato il notaio. Guarda soprattutto l’articolo 7 sulle quote.', att: [statuto()] },
    { at: '2026-09-22 22:30', text: 'Letta. All’articolo 7 la quota minima mi sembra alta per i soci giovani: si può fare una quota ridotta sotto i 30 anni?' },
    { at: '2026-09-23 08:40', from: fusco, text: 'Ottima idea, la propongo al notaio. Ti rimando la bozza perché nell’altra email avevo allegato per sbaglio la versione di agosto: questa è quella giusta. Anzi no, era giusta anche quella, scusa.', att: [statuto()] },
    { at: '2026-09-28 19:15', from: fusco, to: [ME].concat(soci), text: 'Cari tutti,\n\nper la riunione di giovedì vi rimetto qui la bozza dello statuto, così l’avete tutti sottomano. Alex ha proposto una quota ridotta sotto i 30 anni, ne parliamo lì.', att: [statuto()] }
  ], { with: fusco });
  thread('verbale', 'Verbale della riunione di giovedì', [
    { at: '2026-10-03 10:20', from: fusco, to: [ME].concat(soci), text: 'Cari tutti,\n\nin allegato il verbale della riunione di giovedì, con la quota ridotta approvata. Se avete correzioni scrivetemi entro mercoledì.', att: [att('documento.pdf', 142000)] }
  ]);

  // The same file under two names, from two people.
  const ciampi = P('Colori Ciampi', 'preventivi@coloriciampi.esempio.it', 'outlook', 'Ufficio preventivi\nColori Ciampi – tinteggiature e decorazioni\nTel. 06 5553 0408', { sigDash: false });
  thread('tinteggiatura', 'Preventivo tinteggiatura soggiorno', [
    { at: '2026-09-30 15:10', from: ciampi, text: 'Gentile cliente,\n\nin allegato il preventivo per la tinteggiatura del soggiorno e del corridoio, con stuccatura delle crepe e due mani di idropittura traspirante. Il preventivo è valido 60 giorni.\n\nCordiali saluti', att: [att('PREV_2026_0311.pdf', 188000, { fileKey: 'tinteggiatura-0311', text: 'Preventivo n. 2026/0311\nTinteggiatura del soggiorno (32 m²) e del corridoio (9 m²)\nStuccatura delle crepe del soffitto\nDue mani di idropittura traspirante, colore bianco latte\nProtezione di pavimenti e infissi, pulizia finale\nTotale: 1.480,00 € + IVA\nValidità: 60 giorni' })] }
  ]);
  const sergio = P('Sergio Alberti', 'sergio.alberti@esempio.it', 'gmail-it', 'Sergio');
  thread('imbianchino', 'Il preventivo dell’imbianchino', [
    { at: '2026-10-02 18:45', from: sergio, text: 'Ciao Alex,\n\nti giro il preventivo che Ciampi ha fatto a mia sorella per il soggiorno, così vedi se è in linea con quello che ha fatto a te. Mi sembra caro.', att: [att('preventivo imbianchino soggiorno.pdf', 188000, { fileKey: 'tinteggiatura-0311' })] },
    { at: '2026-10-03 09:20', text: 'È identico al mio, anche nei prezzi: direi che sono i loro prezzi standard.' }
  ], { with: sergio });

  write('allegati-nascosti', 'Allegati che sembrano decorazioni: un QR, uno screenshot incorporato, una scansione stretta, lo stesso documento in tre messaggi, lo stesso file con due nomi, due file diversi con lo stesso nome',
    'giro v15, prova 3 di app/NOTE.md (gli allegati nascosti)', messages);
}

/* ---------- prova 6: la pulizia e i messaggi importanti ---------- */
{
  const { thread, messages } = build('pulizia-importanti');
  const auto = [['Auto-Submitted', 'auto-generated']];
  const news = (domain) => [['List-Unsubscribe', `<https://${domain}/disiscriviti>, <mailto:disiscrivi@${domain}>`], ['List-Id', `<avvisi.${domain}>`]];

  // A pickup code for a locker, before the «picked up» notice: it ends up in «Spedizioni concluse».
  const lampo = { name: 'Spedizioni Lampo', address: 'avvisi@spedizionilampo.example' };
  thread('lampo-1', 'Il tuo pacco è in viaggio', [{ at: '2026-09-08 08:10', from: lampo, headers: auto, labels: ['CATEGORY_UPDATES'], text: 'Il tuo pacco LMP 4471 2093 è partito dal centro di smistamento. Arriva nel locker che hai scelto tra 1-2 giorni lavorativi.' }]);
  thread('lampo-2', 'Pacco nel locker: codice di ritiro 731 204', [{ at: '2026-09-10 14:35', from: lampo, headers: auto, labels: ['CATEGORY_UPDATES'], text: 'Il tuo pacco LMP 4471 2093 è nell’armadietto Locker Lampo di Piazza dei Tigli 3, vano 14.\n\nCodice di ritiro: 731 204\nPIN: 5528\n\nHai tempo fino al 13 settembre alle 20:00. Dopo il pacco torna al mittente.' }]);
  thread('lampo-3', 'Pacco ritirato', [{ at: '2026-09-11 18:02', from: lampo, headers: auto, labels: ['CATEGORY_UPDATES'], text: 'Hai ritirato il pacco LMP 4471 2093 dal locker di Piazza dei Tigli 3. Grazie!' }]);

  // A medical appointment and a payment receipt, from senders with List-Unsubscribe: newsletter, older than 14 days.
  thread('visita', 'Promemoria: visita cardiologica di giovedì 12 novembre, ore 9:40', [{ at: '2026-09-20 10:00', from: { name: 'Poliambulatorio Salus Esempio', address: 'prenotazioni@poliambulatorio-salus.example' }, headers: news('poliambulatorio-salus.example'), labels: ['CATEGORY_UPDATES'], text: 'Gentile Alex Moro,\n\nle ricordiamo la prenotazione n. 26-118734:\n\nvisita cardiologica con elettrocardiogramma\ndott. Fabio Serra\ngiovedì 12 novembre 2026, ore 9:40\nsede di Via delle Acacie 18, secondo piano\n\nSi presenti 15 minuti prima con l’impegnativa, la tessera sanitaria e gli esami precedenti. Per disdire o spostare la visita risponda a questa email o chiami lo 06 5554 3300 almeno 48 ore prima.\n\nPoliambulatorio Salus Esempio\n\nRicevi le comunicazioni del poliambulatorio perché sei registrato. Disiscriviti' }]);
  thread('quota', 'Grazie, Alex: quota del primo trimestre', [{ at: '2026-09-14 16:45', from: { name: 'Scuola di Musica Esempio', address: 'segreteria@scuolamusica-esempio.example' }, headers: news('scuolamusica-esempio.example'), text: 'Ciao Alex,\n\nabbiamo ricevuto la quota del primo trimestre per il corso di chitarra di Leo.\n\nRicevuta n. 2026/0457\nImporto: 210,00 €\nPagato con bonifico il 12 settembre 2026\nIntestatario: Alex Moro\n\nConserva questa email: vale come ricevuta per la detrazione delle spese per le attività dei ragazzi.\n\nA presto in aula!\nLa segreteria\n\nRicevi questa email perché sei iscritto alle comunicazioni della scuola. Disiscriviti' }]);

  // The exclusions: a starred newsletter, a real attachment from a newsletter sender, a thread where I replied.
  thread('cinema', 'La programmazione di settembre', [{ at: '2026-09-03 07:30', from: { name: 'Cinema Esempio', address: 'newsletter@cinemaesempio.example' }, headers: news('cinemaesempio.example'), labels: ['CATEGORY_PROMOTIONS'], flagged: true, text: null, html: '<html><body><table width="600"><tr><td><h1>Settembre al Cinema Esempio</h1><p>Torna la rassegna del giovedì: sei film restaurati, ingresso ridotto a 5 euro con la tessera.</p><ul><li>giovedì 17 settembre, ore 21: <b>Il sorpasso</b></li><li>giovedì 24 settembre, ore 21: <b>La dolce vita</b></li><li>giovedì 1 ottobre, ore 21: <b>Divorzio all’italiana</b></li></ul><p>Abbonamento alla rassegna: 24 euro, in cassa o sul sito.</p><p style="font-size:11px">Ricevi questa email perché sei iscritto. <a href="https://cinemaesempio.example/disiscriviti">Disiscriviti</a></p></td></tr></table></body></html>' }]);
  thread('hosting', 'Il tuo documento contabile di settembre', [{ at: '2026-09-05 06:50', from: { name: 'Hosting Esempio', address: 'news@hostingesempio.example' }, headers: news('hostingesempio.example'), text: 'Ciao Alex,\n\nin allegato la fattura n. FT-2026-0912 del 1° settembre per il rinnovo annuale del dominio e dello spazio web. L’importo è già stato addebitato sulla carta registrata.\n\nIl team di Hosting Esempio\n\nRicevi questa email perché sei cliente di Hosting Esempio. Disiscriviti dalle comunicazioni', att: [att('Fattura FT-2026-0912.pdf', 64000)] }]);
  const fibra = { name: 'Fibra Esempio – Assistenza', address: 'assistenza@fibraesempio.example' };
  const fibraH = news('fibraesempio.example').concat(auto);
  thread('ticket', 'Richiesta 48213: connessione lenta la sera', [
    { at: '2026-08-28 09:20', from: fibra, headers: fibraH, text: 'Gentile cliente,\n\nabbiamo aperto la richiesta 48213 per la sua segnalazione «connessione lenta la sera». Può rispondere a questa email per aggiungere dettagli: la risposta viene allegata alla richiesta.\n\nAssistenza Fibra Esempio' },
    { at: '2026-08-28 21:40', to: [fibra], text: 'Buonasera,\n\nrallenta tutte le sere dalle 21 alle 23: la velocità scende sotto i 5 Mbit/s, di giorno è normale. Il modem è quello che mi avete dato voi, cablato. Cosa devo controllare?' },
    { at: '2026-08-31 11:05', from: fibra, headers: fibraH, quote: false, text: 'Gentile cliente,\n\nabbiamo trovato un sovraccarico sull’apparato della sua centrale e lo abbiamo risolto stamattina. La richiesta 48213 è chiusa. Se il problema si ripresenta risponda a questa email entro 10 giorni e la riapriamo.\n\nAssistenza Fibra Esempio' }
  ]);

  // Repetitive messages from a service, similar subjects with different numbers: for the cleanup that learns.
  const acqua = { name: 'Acqua Esempio', address: 'avvisi@fornitore-acqua.example' };
  [['2026-07-03 06:30', 1234, '3 luglio', '38,40', '3 agosto'], ['2026-08-02 06:30', 1301, '2 agosto', '41,10', '1 settembre'], ['2026-09-02 06:30', 1388, '2 settembre', '39,75', '2 ottobre'], ['2026-10-02 06:30', 1462, '2 ottobre', '36,90', '1 novembre']].forEach(([at, num, day, euro, due]) => {
    thread('acqua-' + num, `Fattura n. ${num} del ${day}`, [{ at, from: acqua, headers: auto, labels: ['CATEGORY_UPDATES'], text: `Gentile cliente,\n\nè disponibile la fattura n. ${num} del ${day} 2026, importo ${euro} €, scadenza ${due}. L’importo sarà addebitato sul conto che termina con 4821.\n\nLa trova nell’area clienti, sezione Fatture.\n\nAcqua Esempio – servizio clienti` }]);
  });

  // v15, dopo le prove: un registrar che manda promozioni e anche la conferma del rinnovo, fatta solo di testo;
  // gli aggiornamenti ai soci di una società con cui c'è anche corrispondenza personale, mandati come una newsletter.
  const dominiamo = { name: 'Dominiamo Esempio', address: 'noreply@dominiamo.example' };
  thread('dominiamo-promo', 'Domini .shop a 1,99 € il primo anno', [{ at: '2026-08-20 09:00', from: dominiamo, headers: news('dominiamo.example'), labels: ['CATEGORY_PROMOTIONS'], text: 'Ciao Alex,\n\nsolo per questa settimana i domini .shop e .store costano 1,99 € il primo anno. Scegli il tuo nome prima che lo prenda qualcun altro.\n\nIl team di Dominiamo Esempio\n\nRicevi questa email perché sei cliente di Dominiamo Esempio. Disiscriviti' }]);
  thread('dominiamo-rinnovo', 'Rinnovo completato: studiomoro.esempio.it', [{ at: '2026-09-12 03:15', from: dominiamo, headers: news('dominiamo.example'), labels: ['CATEGORY_UPDATES'], text: 'Ciao Alex,\n\nil rinnovo del dominio studiomoro.esempio.it è stato completato.\n\nOrdine n. DM-558201\nImporto addebitato: 14,90 € sulla carta che termina con 0915\nNuova scadenza: 12 settembre 2027\n\nIl team di Dominiamo Esempio\n\nRicevi questa email perché sei cliente di Dominiamo Esempio. Disiscriviti' }]);
  const chiara = P('Chiara Vela', 'chiara.vela@officinavela.example', 'gmail-it', 'Chiara Vela\nOfficina Vela srl – amministratrice', { sigDash: false });
  thread('vela-bilancio', 'Assemblea dei soci, la data', [
    { at: '2026-07-10 17:20', from: chiara, text: 'Ciao Alex,\n\nper l’assemblea che approva il bilancio pensavamo a martedì 28 luglio alle 18, in sede. Ti va bene o preferisci il collegamento video?' },
    { at: '2026-07-11 09:05', text: 'Ciao Chiara, il 28 va bene, vengo in sede.', quote: true },
    { at: '2026-07-11 09:40', from: chiara, text: 'Perfetto, ti mando la convocazione ufficiale la settimana prossima.', quote: true },
    { at: '2026-07-11 10:02', text: 'Grazie, a presto.', quote: true }
  ], { with: chiara });
  const vela = { name: 'Officina Vela', address: 'aggiornamenti@officinavela.example' };
  const velaH = news('officinavela.example').concat([['X-Mailer', 'Posta Massiva Esempio 4.2']]);
  thread('vela-q2', 'Aggiornamento ai soci – secondo trimestre', [{ at: '2026-07-22 08:00', from: vela, headers: velaH, text: 'Care socie, cari soci,\n\nil secondo trimestre chiude con ricavi in crescita del 12% sull’anno scorso, trainati dalle commesse nautiche. Abbiamo assunto due tecnici e firmato il contratto per il nuovo capannone di via dei Calafati.\n\nNel prossimo trimestre: consegna delle prime due barche della serie Libeccio.\n\nChiara Vela, per il consiglio di amministrazione\n\nRicevi questa comunicazione perché sei socio di Officina Vela srl. Disiscriviti' }]);
  thread('vela-q3', 'Aggiornamento ai soci – terzo trimestre', [{ at: '2026-10-15 08:00', from: vela, headers: velaH, text: 'Care socie, cari soci,\n\nil terzo trimestre è andato meno bene del previsto: la consegna della serie Libeccio slitta a gennaio per un ritardo del fornitore delle vele. I conti restano in ordine; il dettaglio lo vediamo in assemblea a dicembre.\n\nChiara Vela, per il consiglio di amministrazione\n\nRicevi questa comunicazione perché sei socio di Officina Vela srl. Disiscriviti' }]);

  write('pulizia-importanti', 'Messaggi di servizi importanti fatti solo di testo dentro i gruppi della pulizia, le tre esclusioni (stella, allegato vero, thread con una mia risposta) e fatture ripetitive per la pulizia che impara',
    'giro v15, prova 6 di app/NOTE.md (la pulizia proposta)', messages);
}

/* ---------- prova 7: indirizzi condivisi, Jira e simili ---------- */
{
  const { thread, messages } = build('indirizzi-condivisi');
  const JIRA = 'jira@acme-esempio.atlassian.net';
  const J = (name) => ({ name, address: JIRA });
  const jh = (key, ev) => [['Auto-Submitted', 'auto-generated'], ['Precedence', 'bulk'], ['X-JIRA-FingerPrint', 'a3f19c0e5b7d2864acme'], ['X-Atlassian-Issue', key], ['X-Atlassian-Event', ev]];
  const url = (key) => `\n\nVisualizza il ticket: https://acme-esempio.atlassian.net/browse/${key}\n\nHai ricevuto questa email perché segui il ticket. Gestisci le notifiche: https://acme-esempio.atlassian.net/notifiche`;
  const urlEn = (key) => `\n\nView issue: https://acme-esempio.atlassian.net/browse/${key}\n\nYou’re receiving this email because you’re watching this issue. Manage notifications: https://acme-esempio.atlassian.net/notifiche`;
  const issue = (key, title, list) => thread(key, `[JIRA] (${key}) ${title}`, list.map(([at, name, ev, text, unread]) => ({ at, from: J(name), headers: jh(key, ev), text, unread, sign: false, quote: false })), { prefix: '' });
  // prefix '' and the same subject in every message, as Jira does; the thread is the issue

  issue('ACM-142', 'Errore nel login con SSO', [
    ['2026-09-28 09:10', 'Davide Russo (Acme)', 'issue_created', 'Davide Russo ha creato questo ticket\n\nACM-142 · Errore nel login con SSO\nTipo: Bug · Priorità: Media · Assegnatario: non assegnato\n\nDa stamattina chi entra con l’accesso unico (SSO) torna alla pagina di login dopo aver inserito il codice. Succede con Chrome e Firefox, non con l’app. Il cliente Beta Logistica ha già aperto due segnalazioni.' + url('ACM-142')],
    ['2026-09-28 09:11', 'Automation for Jira (Acme)', 'issue_updated', 'Automation for Jira ha modificato questo ticket\n\nACM-142 · Errore nel login con SSO\nPriorità: Media → Alta\nEtichette: + sso, + cliente\n\nRegola: «Bug con SSO = priorità alta»' + url('ACM-142')],
    ['2026-09-28 10:05', 'Sara Conti (Acme)', 'issue_generic', 'Sara Conti changed the status to In corso\n\nACM-142 · Errore nel login con SSO\nStatus: Da fare → In corso' + urlEn('ACM-142')],
    ['2026-09-28 10:06', 'Sara Conti (Acme)', 'issue_assigned', 'Sara Conti assigned ACM-142 to you\n\nACM-142 · Errore nel login con SSO\nAssignee: Non assegnato → Alex Moro' + urlEn('ACM-142')],
    ['2026-09-29 11:20', 'Sara Conti (Acme)', 'issue_commented', 'Sara Conti ha commentato\n\nACM-142 · Errore nel login con SSO\n\n«Ho riprodotto il problema: il token torna indietro con l’orario del server spostato di due ore. Alex, puoi controllare tu la configurazione del fuso orario sul server di prova? Io intanto preparo la correzione.»' + url('ACM-142')],
    ['2026-09-29 15:40', 'Luca Neri (Acme)', 'issue_commented', 'Luca Neri ha commentato\n\nACM-142 · Errore nel login con SSO\n\n«Confermo: sul server di prova l’ora di sistema è in UTC, ma il servizio dei certificati legge l’ora legale. Lo sistemo domani mattina.»' + url('ACM-142')],
    ['2026-09-30 09:15', 'Davide Russo (Acme)', 'issue_mentioned', 'Davide Russo ti ha menzionato in ACM-142\n\nACM-142 · Errore nel login con SSO\n\n«@Alex Moro quando hai controllato il fuso scrivimi qui, così rispondo al cliente con una data.»' + url('ACM-142')],
    ['2026-10-01 17:30', 'S. Conti (Acme)', 'issue_commented', 'S. Conti ha commentato\n\nACM-142 · Errore nel login con SSO\n\n«Correzione pronta, è nella pull request #88 su Repohub. Manca solo la revisione.»' + url('ACM-142')],
    ['2026-10-06 08:50', 'Sara Conti Bassi (Acme)', 'issue_resolved', 'Sara Conti Bassi changed the status to Risolto\n\nACM-142 · Errore nel login con SSO\nStatus: In revisione → Risolto\nResolution: Risolto' + urlEn('ACM-142')]
  ]);
  issue('ACM-157', 'Export CSV senza intestazioni', [
    ['2026-10-05 14:00', 'Davide Russo (Acme)', 'issue_created', 'Davide Russo created this issue\n\nACM-157 · Export CSV senza intestazioni\nType: Bug · Priority: Media\n\nL’export CSV dei report esce senza la riga delle intestazioni da giovedì. Il cliente lo carica in un foglio di calcolo e le colonne non si capiscono più.' + urlEn('ACM-157')],
    ['2026-10-05 14:02', 'Davide Russo (Acme)', 'issue_assigned', 'Davide Russo assigned ACM-157 to you\n\nACM-157 · Export CSV senza intestazioni\nAssignee: Non assegnato → Alex Moro' + urlEn('ACM-157')],
    ['2026-10-06 10:40', 'Sara Conti (Acme)', 'issue_commented', 'Sara Conti commented on ACM-157\n\n> Succede solo con i report programmati: quelli scaricati a mano hanno le intestazioni. Alex, can you check the scheduler configuration? Credo che l’opzione sia cambiata con l’ultimo aggiornamento.' + urlEn('ACM-157')],
    ['2026-10-07 16:15', 'Davide Russo (Acme)', 'issue_mentioned', 'Davide Russo mentioned you on ACM-157\n\n> @Alex Moro il cliente chiede una data: riusciamo entro venerdì?' + urlEn('ACM-157'), true],
    ['2026-10-08 09:00', 'Automation for Jira (Acme)', 'issue_updated', 'Automation for Jira changed the status to In revisione\n\nACM-157 · Export CSV senza intestazioni\nStatus: In corso → In revisione\nRule: «PR aperta = in revisione»' + urlEn('ACM-157')],
    ['2026-10-09 12:30', 'Sara Conti (Acme)', 'issue_resolved', 'Sara Conti changed the status to Done\n\nACM-157 · Export CSV senza intestazioni\nStatus: In revisione → Done' + urlEn('ACM-157')]
  ]);
  issue('ACM-163', 'Rinnovo dei certificati del server di prova', [
    ['2026-10-02 08:40', 'Luca Neri (Acme)', 'issue_created', 'Luca Neri ha creato questo ticket\n\nACM-163 · Rinnovo dei certificati del server di prova\nTipo: Attività · Assegnatario: Luca Neri\n\nI certificati del server di prova scadono il 20 ottobre. Li rinnovo e intanto allineo l’ora del servizio, vedi ACM-142.' + url('ACM-163')],
    ['2026-10-12 17:05', 'Luca Neri (Acme)', 'issue_resolved', 'Luca Neri ha cambiato lo stato in Fatto\n\nACM-163 · Rinnovo dei certificati del server di prova\nStato: In corso → Fatto' + url('ACM-163')]
  ]);

  // The same person on a code service.
  const REPO = 'notifications@repohub.example';
  const rh = (reason) => [['List-Id', '<portale.acme.repohub.example>'], ['List-Unsubscribe', '<https://repohub.example/notifications/unsubscribe/acme-portale-88>'], ['X-Repohub-Reason', reason], ['Precedence', 'list']];
  thread('pr88', '[acme/portale] Correzione del fuso orario nel token SSO (PR #88)', [
    { at: '2026-10-01 17:25', from: { name: 'Sara Conti', address: REPO }, headers: rh('review_requested'), sign: false, text: 'Sara Conti opened this pull request and requested your review.\n\nCorregge ACM-142: la scadenza del token SSO ora si calcola in UTC, così l’ora legale non la sposta più.\n\nCommit summary\n * Usa UTC per la scadenza del token\n * Prova per il cambio dell’ora legale\n\nFile changes\n M src/auth/token.ts (12)\n M test/auth/token.test.ts (40)\n\nView it on Repohub: https://repohub.example/acme/portale/pull/88\nYou are receiving this because your review was requested.' },
    { at: '2026-10-02 10:10', from: { name: 'Sara Conti', address: REPO }, headers: rh('review_requested'), sign: false, quote: false, text: 'Sara Conti commented on this pull request.\n\nIn src/auth/token.ts:\n> +  const scadenza = adesso + durata;\n\nAlex, qui ho tolto il margine di 30 secondi che c’era prima: secondo te serve ancora con i telefoni che hanno l’ora sbagliata?\n\nReply to this email directly or view it on Repohub: https://repohub.example/acme/portale/pull/88#discussion_r1042' },
    { at: '2026-10-03 09:30', from: { name: 'Davide Russo', address: REPO }, headers: rh('review_requested'), sign: false, quote: false, text: 'Davide Russo approved this pull request.\n\nView it on Repohub: https://repohub.example/acme/portale/pull/88#pullrequestreview-551' },
    { at: '2026-10-05 18:20', from: { name: 'Sara Conti', address: REPO }, headers: rh('review_requested'), sign: false, quote: false, text: 'Merged #88 into main.\n\nView it on Repohub: https://repohub.example/acme/portale/pull/88' }
  ], { prefix: 'Re: ' });

  // Sara Conti as a real person, with her own address; the display name changes.
  const sara = P('Sara Conti', 'sara.conti@acme-esempio.it', 'gmail-it', 'Sara Conti\nAcme Esempio · Sviluppo');
  const saraBassi = P('Sara Conti Bassi', 'sara.conti@acme-esempio.it', 'gmail-it', 'Sara Conti Bassi\nAcme Esempio · Sviluppo');
  thread('sara-riunione', 'Riunione con Beta Logistica', [
    { at: '2026-09-24 12:40', from: sara, text: 'Ciao Alex,\n\nvenerdì alle 11 c’è la riunione con Beta Logistica sul login. Ci vieni anche tu? Porto io i numeri delle segnalazioni dell’ultimo mese.' },
    { at: '2026-09-24 13:15', text: 'Ci sono. Mi mandi l’indirizzo della sala?' },
    { at: '2026-09-24 13:30', from: sara, text: 'Sala Ponente, al terzo piano. A venerdì!' }
  ], { with: sara });
  thread('sara-pranzo', 'Pranzo per il compleanno di Davide', [
    { at: '2026-10-10 09:05', from: saraBassi, unread: true, text: 'Ciao! Martedì offriamo il pranzo a Davide per il compleanno, alla trattoria qui sotto. Partecipi? Sono 15 euro a testa, li raccolgo io.' }
  ]);

  // A homonym: another Luca Neri, another company, another subject. Must stay another person.
  const neri = P('Luca Neri', 'luca.neri@studio-altro.example', 'outlook', 'Luca Neri\nStudio Altro – fotografia\nVia del Faro 5, Roma · 347 555 0388', { sigDash: false });
  thread('neri', 'Corso di fotografia, le date', [
    { at: '2026-09-27 18:30', from: neri, text: 'Buonasera Alex,\n\ncome le avevo detto al mercatino, il corso di fotografia di base riparte a novembre: sei incontri il mercoledì sera, dalle 19 alle 21, più un’uscita di sabato mattina. Il costo è di 180 euro.\n\nSe le interessa le tengo un posto.' },
    { at: '2026-09-28 08:50', text: 'Buongiorno Luca, sì, mi tenga un posto. Serve una macchina reflex o va bene anche una compatta?' },
    { at: '2026-09-28 12:20', from: neri, text: 'Va benissimo la compatta, purché si possa impostare a mano. Le confermo il posto.' }
  ], { with: neri });

  // A company mailbox shared by real people, without automated headers: one conversation, different signers.
  const paola = P('Forniture Esempio - Paola', 'info@forniture-esempio.it', 'outlook', 'Paola\nForniture Esempio srl – Ufficio commerciale\nTel. 06 5550 8120', { sigDash: false });
  const gianni = P('Forniture Esempio', 'info@forniture-esempio.it', 'outlook', 'Gianni\nForniture Esempio srl – Magazzino', { sigDash: false });
  thread('sedie', 'Sedie per la sala riunioni', [
    { at: '2026-09-30 10:15', from: ME2, to: [gianni], text: 'Buongiorno,\n\nvorrei un preventivo per 8 sedie impilabili, modello Elle, colore antracite, con la consegna in studio, in via Merulana.\n\nGrazie' },
    { at: '2026-10-01 11:40', from: paola, to: [ME2], text: 'Buongiorno,\n\nle mando il preventivo per le 8 sedie Elle antracite, consegna compresa. I tempi sono di circa una settimana dall’ordine.\n\nCordiali saluti', att: [att('Preventivo 2026-1187.pdf', 96000, { text: 'Preventivo n. 2026-1187\nSedia impilabile Elle, scocca in polipropilene antracite, gambe in acciaio verniciato\n8 pezzi × 74,00 €\nTrasporto e consegna al piano: 40,00 €\nTotale: 632,00 € + IVA\nPagamento: bonifico alla conferma dell’ordine' })] },
    { at: '2026-10-02 09:10', from: ME2, to: [paola], text: 'Va bene, procediamo con l’ordine. Per la consegna serve qualcuno presente?' },
    { at: '2026-10-06 15:30', from: gianni, to: [ME2], text: 'Buongiorno, sono Gianni del magazzino.\n\nLe sedie partono giovedì mattina, il corriere la chiama mezz’ora prima di arrivare. Serve qualcuno per firmare la bolla.\n\nSaluti' }
  ], { me: ME2, with: paola });

  write('indirizzi-condivisi', 'Un indirizzo, tante persone: Jira con chi commenta e chi cambia solo lo stato, omonimi, un nome abbreviato e uno che cambia, la stessa persona su un servizio di codice, una casella aziendale firmata da persone diverse',
    'giro v15, prova 7 di app/NOTE.md e voce «Un indirizzo, tante persone (Jira e simili)»', messages);
}
