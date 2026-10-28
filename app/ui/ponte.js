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
        window.AMB_DATA = d;
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
    /* dati nuovi dal motore: si ricostruisce il modello e si ridisegna */
    ricarica: function () {
      return invoca('dati').then(function (d) {
        window.AMB_DATA = d;
        if (window.__dc && window.__dc.datiNuovi) window.__dc.datiNuovi();
      });
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
