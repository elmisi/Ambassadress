/*
 * Il ponte fra l'interfaccia e il motore.
 * Nell'app (Tauri) chiama i comandi Rust e ascolta i loro eventi.
 * Aperta in un browser, per le prove, fa da motore finto con i dati sintetici del
 * prototipo: `?primo` mostra il primo avvio, `?rete=offline|auth|sync` uno stato della rete.
 */
(function () {
  'use strict';

  var T = window.__TAURI__;
  var ascolti = {};

  function emetti(evento, dati) {
    (ascolti[evento] || []).forEach(function (fn) {
      try { fn(dati); } catch (e) { console.error(e); }
    });
  }

  var vuoto = function (casella) {
    return { schema: 1, meta: { source: 'imap', count: 0 }, account: casella ? { address: casella.address, name: casella.name || '', aliases: [] } : {}, accounts: casella ? [casella] : [], messages: [] };
  };

  /* ---------- motore finto, solo nel browser ---------- */
  var finto = {
    configurata: !/[?&]primo\b/.test(location.search),
    sintetici: null,
    carica: function () {
      if (finto.sintetici) return Promise.resolve(finto.sintetici);
      return new Promise(function (ok, ko) {
        var s = document.createElement('script');
        s.src = '../../mockups/data/synthetic.js';
        s.onload = function () { finto.sintetici = window.AMB_DATA; ok(finto.sintetici); };
        s.onerror = function () { ko(new Error('Non trovo i dati sintetici: apri la pagina dalla cartella del progetto.')); };
        document.head.appendChild(s);
      });
    },
    casella: function () {
      var d = finto.sintetici;
      var a = d && d.accounts && d.accounts[0];
      return a ? { address: a.address, name: d.account.name, kind: a.kind, server: a.server } : null;
    },
    comandi: {
      avvio: function () {
        return finto.carica().then(function () {
          var m = /[?&]rete=(offline|auth|sync)/.exec(location.search);
          var rete = m ? { stato: m[1], fatti: 2412, totale: 18400, alle: Date.now() - 42 * 60000 } : { stato: 'ok', alle: Date.now() };
          return { configurata: finto.configurata, casella: finto.configurata ? finto.casella() : null, rete: rete };
        });
      },
      dati: function () {
        if (!finto.configurata) return Promise.resolve(vuoto(null));
        return finto.carica().then(function (d) {
          // gli allegati sintetici stanno nella cartella dei dati del prototipo
          var messaggi = d.messages.map(function (m) {
            if (!m.attachments || !m.attachments.length) return m;
            return Object.assign({}, m, { attachments: m.attachments.map(function (a) {
              if (!a.path) return a;
              // come il motore: anche la miniatura del PDF è un indirizzo pronto
              return Object.assign({}, a, { url: '../../mockups/data/' + a.path }, a.thumb ? { thumb: '../../mockups/data/' + a.thumb } : {});
            }) });
          });
          return Object.assign({}, d, { messages: messaggi, meta: Object.assign({}, d.meta, { source: 'imap' }) });
        });
      },
      configura: function (a) {
        var tappe = ['server', 'password', 'posta'];
        return new Promise(function (ok, ko) {
          if (/sbagliata/.test(a.password)) {
            setTimeout(function () { emetti('primo-avvio', { tappa: 'server' }); }, 300);
            setTimeout(function () { ko('Il server rifiuta la password.'); }, 800);
            return;
          }
          tappe.forEach(function (t, i) { setTimeout(function () { emetti('primo-avvio', { tappa: t }); }, 400 * (i + 1)); });
          setTimeout(function () {
            finto.configurata = true;
            ok({ casella: finto.casella() });
            var fatti = 0;
            var timer = setInterval(function () {
              fatti = Math.min(18400, fatti + 3100);
              if (fatti >= 18400) { clearInterval(timer); emetti('rete', { stato: 'ok', alle: Date.now() }); emetti('aggiornato', {}); return; }
              emetti('rete', { stato: 'sync', fatti: fatti, totale: 18400 });
              if (fatti === 3100) emetti('aggiornato', {});
            }, 500);
          }, 400 * tappe.length + 300);
        });
      },
      cambia_password: function () { return Promise.resolve(true); },
      togli_casella: function () { finto.configurata = false; return Promise.resolve(true); },
      apri_allegato: function (a) { window.open(a.url, '_blank'); return Promise.resolve(true); },
      // giro 1: nel browser questi comandi rispondono come il motore, senza toccare niente
      aggiorna: function () { setTimeout(function () { emetti('rete', { stato: 'ok', alle: Date.now() }); }, 800); return Promise.resolve(true); },
      programma_per: function (a) { return Promise.resolve(/pdf/i.test(a.tipo || '') ? 'Papers' : ''); },
      salva_messaggio: function (a) { return Promise.resolve('~/Scaricati/(prova nel browser, messaggio ' + a.id + ')'); },
      salva_testo: function (a) { return Promise.resolve('~/Scaricati/' + a.nome); },
      salva_allegato: function (a) { return Promise.resolve('~/Scaricati/' + a.nome); },
      priorita: function () { return Promise.resolve(true); },
      notifica: function (a) { console.log('[a10s] notifica:', a.titolo, '·', a.testo); return Promise.resolve(true); }
    }
  };

  function invoca(cmd, args) {
    if (T) return T.core.invoke(cmd, args || {});
    var f = finto.comandi[cmd];
    return f ? f(args || {}) : Promise.reject(new Error('Comando sconosciuto: ' + cmd));
  }

  /* ---------- giro 2: i dati arrivano interi una volta, poi solo le differenze ---------- */
  // la versione dei dati che la pagina ha già: generazione del database e numero dell'ultimo cambio
  var versione = null;
  function dal() {
    return versione ? { dal: versione.versione, generazione: versione.generazione } : {};
  }
  function quando(m) {
    return Date.parse(m.date || m.internalDate || '') || 0;
  }
  /* giro 2, lo storico: il motore manda la finestra dei più recenti (con i loro thread interi) e le
     sue differenze; i messaggi più vecchi la pagina li chiede quando servono (una persona aperta,
     una ricerca) e stanno qui accanto, fuori dalle differenze. AMB_DATA è la finestra più questi */
  var base = null;
  var storia = {};
  // gli id dello storico, dal chiesto meno di recente: oltre il tetto si tolgono i primi
  var ordine = [];
  var TETTO_STORIA = 6000;
  function componi() {
    if (!base) { window.AMB_DATA = base; return; }
    var visti = {};
    base.messages.forEach(function (m) { visti[m.id] = true; });
    var extra = ordine.filter(function (id) { return storia[id] && !visti[id]; }).map(function (id) { return storia[id]; });
    if (!extra.length) { window.AMB_DATA = base; return; }
    var lista = base.messages.concat(extra).sort(function (a, b) { return quando(a) - quando(b) || (a.localId || 0) - (b.localId || 0); });
    window.AMB_DATA = Object.assign({}, base, { messages: lista, meta: Object.assign({}, base.meta, { count: lista.length }) });
  }
  /* i messaggi dello storico arrivati dal motore: si tengono, i più vecchi chiesti escono oltre il
     tetto. Vero se ce n'è almeno uno che la pagina non aveva */
  function tieniStoria(d) {
    var nuovi = 0;
    var arrivati = {};
    var finestra = {};
    if (base) base.messages.forEach(function (m) { finestra[m.id] = true; });
    ((d && d.messages) || []).forEach(function (m) {
      if (!storia[m.id] && !finestra[m.id]) nuovi++;
      storia[m.id] = m;
      arrivati[m.id] = true;
    });
    ordine = ordine.filter(function (id) { return !arrivati[id]; }).concat(Object.keys(arrivati));
    while (ordine.length > TETTO_STORIA) delete storia[ordine.shift()];
    if (nuovi) componi();
    return nuovi > 0;
  }
  // una richiesta allo storico alla volta, e la stessa non due volte di fila
  var codaStoria = Promise.resolve(false);
  var ultimaStoria = '';
  function chiediStoria(cmd, args) {
    var chiave = cmd + ' ' + JSON.stringify(args);
    if (!T || !ponte.configurata || chiave === ultimaStoria) return Promise.resolve(false);
    ultimaStoria = chiave;
    codaStoria = codaStoria.catch(function () { return false; }).then(function () {
      return invoca(cmd, args).then(tieniStoria);
    });
    return codaStoria.catch(function (e) { ultimaStoria = ''; console.error(e); return false; });
  }

  /* mette nei dati della pagina quello che arriva: tutto, o le differenze (cambiati, nuovi, tolti).
     Vero se è cambiato qualcosa */
  function applica(d) {
    var vecchi = base && base.messages;
    var cambiato = true;
    if (d && d.delta && vecchi) {
      var via = {};
      (d.tolti || []).forEach(function (k) { via[k] = true; });
      var nuovi = {};
      d.messages.forEach(function (m) { nuovi[m.id] = m; });
      var visti = {};
      var lista = [];
      vecchi.forEach(function (m) {
        if (via[m.id]) return;
        if (nuovi[m.id]) { visti[m.id] = true; lista.push(nuovi[m.id]); } else lista.push(m);
      });
      var aggiunti = d.messages.filter(function (m) { return !visti[m.id]; });
      if (aggiunti.length) {
        // come il motore: dal più vecchio, a parità di data per id locale
        lista = lista.concat(aggiunti).sort(function (a, b) { return quando(a) - quando(b) || (a.localId || 0) - (b.localId || 0); });
      }
      cambiato = d.messages.length > 0 || (d.tolti || []).length > 0;
      // un messaggio dello storico sparito dal server sparisce anche da qui
      (d.tolti || []).forEach(function (k) { delete storia[k]; });
      var nuovo = Object.assign({}, d, { messages: lista, meta: Object.assign({}, d.meta, { count: lista.length }) });
      delete nuovo.delta;
      delete nuovo.tolti;
      base = nuovo;
    } else {
      // tutto da capo (primo avvio, casella cambiata): lo storico tenuto qui vale solo per la stessa generazione
      if (!d || !versione || d.generazione !== versione.generazione) { storia = {}; ordine = []; ultimaStoria = ''; }
      base = d;
    }
    componi();
    versione = d && d.generazione ? { generazione: d.generazione, versione: d.versione } : null;
    return cambiato;
  }
  /* il testo dentro gli allegati, per la ricerca: a parte, perché è il pezzo più pesante.
     Ha la sua versione, che avanza solo quando una risposta è arrivata e applicata: una
     richiesta fallita si rifà dalla stessa versione alla volta dopo. Le richieste vanno una
     alla volta, quindi una risposta vecchia non copre mai una più nuova. Vero se è cambiato qualcosa */
  window.AMB_TESTI = window.AMB_TESTI || {};
  var versioneTesti = null;
  var codaTesti = Promise.resolve(false);
  function caricaTesti() {
    if (!T) return Promise.resolve(false);
    codaTesti = codaTesti.catch(function () { return false; }).then(function () {
      var da = versioneTesti ? { dal: versioneTesti.versione, generazione: versioneTesti.generazione } : {};
      return invoca('testi_allegati', da).then(function (t) {
        if (!t || !t.texts) return false;
        var n = 0;
        if (!t.delta) {
          window.AMB_TESTI = {};
          n = 1;
        } else {
          // i messaggi rifatti: i loro testi di prima non valgono più
          var rifatti = {};
          (t.rifatti || []).forEach(function (p) { rifatti[p] = true; });
          // un url è prefisso + messaggio + "/" + parte, e la parte non ha barre
          if (t.rifatti && t.rifatti.length) Object.keys(window.AMB_TESTI).forEach(function (k) {
            if (rifatti[k.slice(0, k.lastIndexOf('/') + 1)]) { delete window.AMB_TESTI[k]; n++; }
          });
        }
        Object.keys(t.texts).forEach(function (k) { window.AMB_TESTI[k] = t.texts[k]; n++; });
        versioneTesti = t.generazione ? { generazione: t.generazione, versione: t.versione } : null;
        return n > 0;
      });
    });
    return codaTesti.catch(function (e) { console.error(e); return false; });
  }
  var inCorso = null;
  var ancora = false;

  var ponte = {
    app: !!T,
    configurata: false,
    casella: null,
    rete: { stato: 'ok', alle: Date.now() },
    invoca: invoca,
    on: function (evento, fn) {
      (ascolti[evento] = ascolti[evento] || []).push(fn);
      if (T) T.event.listen(evento, function (e) { fn(e.payload); });
    },
    /* il primo caricamento, prima di disegnare: chi sono, com'è la rete, i dati */
    avvia: function () {
      return invoca('avvio').then(function (s) {
        ponte.configurata = !!s.configurata;
        ponte.casella = s.casella || null;
        if (s.rete) ponte.rete = s.rete;
        return ponte.configurata ? invoca('dati') : vuoto(null);
      }).then(function (d) {
        applica(d);
        // il testo degli allegati dopo il primo disegno: la finestra si apre prima
        if (ponte.configurata) setTimeout(function () {
          caricaTesti().then(function (c) { if (c && window.__dc && window.__dc.datiNuovi) window.__dc.datiNuovi(); });
        }, 1500);
      });
    },
    /* nell'app i link delle email si aprono nel browser del sistema, anche dentro le newsletter */
    agganciaLink: function (doc) {
      if (!T || !doc || doc.__a10sLink) return;
      doc.__a10sLink = true;
      doc.addEventListener('click', function (e) {
        var a = e.target && e.target.closest && e.target.closest('a[href]');
        if (!a) return;
        var href = a.getAttribute('href') || '';
        if (/^(https?:|mailto:)/i.test(href)) {
          e.preventDefault();
          invoca('apri_link', { url: href }).catch(function (err) { console.error(err); });
        }
      }, true);
      doc.addEventListener('load', function (e) {
        var f = e.target;
        if (f && f.tagName === 'IFRAME') {
          try { ponte.agganciaLink(f.contentDocument); } catch (err) { /* un riquadro di un'altra origine */ }
        }
      }, true);
    },
    /* giro 2: i messaggi più vecchi di una persona, e quelli che una ricerca potrebbe trovare,
       dallo storico nel database. Vero se è arrivato qualcosa che la pagina non aveva: allora
       la pagina ricostruisce il modello */
    storiaPersona: function (indirizzo) {
      return chiediStoria('storia_persona', { indirizzo: indirizzo });
    },
    cercaStorico: function (domanda) {
      return chiediStoria('cerca_storico', { domanda: domanda });
    },
    /* dati nuovi dal motore: si ricostruisce il modello e si ridisegna */
    ricarica: function () {
      // una alla volta: gli avvisi che arrivano intanto diventano una ricarica sola dopo
      if (inCorso) { ancora = true; return inCorso; }
      var prima = dal();
      inCorso = invoca('dati', prima).then(function (d) {
        var cambiato = applica(d);
        return caricaTesti().then(function (t) { return cambiato || t; });
      }).then(function (cambiato) {
        // una differenza vuota non ridisegna niente
        if (cambiato && window.__dc && window.__dc.datiNuovi) window.__dc.datiNuovi();
      }).finally(function () {
        inCorso = null;
        if (ancora) { ancora = false; ponte.ricarica().catch(function (e) { console.error(e); }); }
      });
      return inCorso;
    }
  };

  // nell'app gli errori della pagina finiscono sul terminale (comando «diario»)
  if (T) {
    var diario = function (riga) { T.core.invoke('diario', { riga: String(riga) }).catch(function () {}); };
    window.addEventListener('error', function (e) { diario('errore: ' + e.message + ' @ ' + (e.filename || '') + ':' + (e.lineno || '')); });
    window.addEventListener('unhandledrejection', function (e) { diario('promessa: ' + ((e.reason && e.reason.message) || e.reason)); });
    var ce = console.error.bind(console);
    console.error = function () { diario('console: ' + Array.prototype.join.call(arguments, ' ')); ce.apply(null, arguments); };
    document.addEventListener('securitypolicyviolation', function (e) { diario('csp: ' + e.violatedDirective + ' ' + e.blockedURI); });
  }

  ponte.on('aggiornato', function () {
    if (ponte.configurata) ponte.ricarica().catch(function (e) { console.error(e); });
  });

  window.A10S = ponte;
})();
