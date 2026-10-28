/*
 * Local runtime for the .dc.html mockups, so they open and navigate from disk.
 * The Claude Design canvas serves its own support.js: never publish this file.
 * This copy is the app's: data come from the engine through ponte.js.
 * It covers the subset of the format the mockups use: {{holes}}, <sc-for>,
 * <sc-if>, on* events, <helmet>, data-props defaults and DCLogic state.
 */
(function () {
  'use strict';

  var hide = document.createElement('style');
  hide.textContent = 'x-dc{display:block;visibility:hidden}x-dc[data-ready]{visibility:visible}';
  document.head.appendChild(hide);

  class DCLogic {
    constructor(props) {
      this.props = props || {};
      this.state = {};
    }
    setState(patch) {
      var next = typeof patch === 'function' ? patch(this.state, this.props) : patch;
      this.state = Object.assign({}, this.state, next);
      this.forceUpdate();
    }
    forceUpdate() {
      if (this.__render) this.__render();
    }
    renderVals() {
      return {};
    }
  }
  window.DCLogic = DCLogic;

  var HOLE = /\{\{\s*([^}]+?)\s*\}\}/g;
  var WHOLE = /^\s*\{\{\s*([^}]+?)\s*\}\}\s*$/;

  function lookup(expr, scope) {
    if (expr === 'true') return true;
    if (expr === 'false') return false;
    if (expr === 'null') return null;
    if (/^-?\d+(\.\d+)?$/.test(expr)) return Number(expr);
    if (/^'.*'$|^".*"$/.test(expr)) return expr.slice(1, -1);
    return expr.split('.').reduce(function (v, k) {
      return v == null ? undefined : v[k];
    }, scope);
  }

  function whole(value, scope) {
    var m = WHOLE.exec(value || '');
    return m ? lookup(m[1], scope) : undefined;
  }

  function interp(str, scope) {
    return str.replace(HOLE, function (_, expr) {
      var v = lookup(expr, scope);
      return v == null ? '' : String(v);
    });
  }

  function renderAll(nodes, scope, out) {
    for (var i = 0; i < nodes.length; i++) renderNode(nodes[i], scope, out);
    return out;
  }

  function renderNode(node, scope, out) {
    if (node.nodeType === 3) {
      out.push(document.createTextNode(interp(node.nodeValue, scope)));
      return;
    }
    if (node.nodeType !== 1) return;
    var tag = node.localName;
    if (tag === 'helmet') return;
    if (tag === 'sc-for') {
      var list = whole(node.getAttribute('list'), scope) || [];
      var as = node.getAttribute('as') || 'item';
      list.forEach(function (item, i) {
        var inner = Object.assign({}, scope, { $index: i });
        inner[as] = item;
        renderAll(node.childNodes, inner, out);
      });
      return;
    }
    if (tag === 'sc-if') {
      if (whole(node.getAttribute('value'), scope)) renderAll(node.childNodes, scope, out);
      return;
    }

    var el = node.cloneNode(false);
    Array.prototype.slice.call(node.attributes).forEach(function (attr) {
      var name = attr.name;
      // sc-src, sc-onload…: attributi che nel modello non devono partire prima del disegno
      if (name.indexOf('sc-') === 0) {
        el.removeAttribute(name);
        name = name.slice(3);
        if (attr.value.indexOf('{{') === -1 && name.indexOf('on') !== 0) { el.setAttribute(name, attr.value); return; }
      }
      if (name.indexOf('hint-') === 0) {
        el.removeAttribute(name);
        return;
      }
      if (name.indexOf('on') === 0) {
        el.removeAttribute(name);
        var fn = whole(attr.value, scope);
        if (typeof fn === 'function') {
          var ev = name.slice(2).toLowerCase();
          if (ev === 'change' && (tag === 'input' || tag === 'textarea')) ev = 'input';
          // i gestori stanno in una tabella: quando un elemento resta, si cambiano quelli
          (el.__on = el.__on || {})[ev] = fn;
        }
        return;
      }
      if (attr.value.indexOf('{{') === -1) return;
      var v = WHOLE.test(attr.value) ? whole(attr.value, scope) : interp(attr.value, scope);
      if (v == null || (v === false && name.indexOf('aria-') !== 0)) el.removeAttribute(name);
      else el.setAttribute(name, String(v));
    });
    if (tag === 'form') {
      (el.__on = el.__on || {}).submit = function (e) { e.preventDefault(); };
    }
    renderAll(node.childNodes, scope, []).forEach(function (child) {
      el.appendChild(child);
    });
    if (tag === 'textarea' && el.hasAttribute('value')) {
      el.textContent = el.getAttribute('value');
      el.removeAttribute('value');
    }
    out.push(el);
  }

  /* ---------- aggiornare solo quello che cambia (giro 1 dell'app) ----------
   * La pagina nuova si confronta con quella che c'è: gli elementi uguali restano (con i loro
   * iframe, immagini, fuoco, cursore e scorrimento), cambiano solo attributi, testi e figli
   * diversi. Gli elementi con un id si ritrovano anche se cambiano posto. */
  function listen(el, on) {
    var have = el.__ears || (el.__ears = {});
    el.__on = on || {};
    Object.keys(el.__on).forEach(function (ev) {
      if (have[ev]) return;
      have[ev] = true;
      el.addEventListener(ev, function (e) {
        var fn = el.__on && el.__on[ev];
        if (fn) return fn(e);
      });
    });
  }
  function wire(el) {
    if (el.nodeType !== 1) return;
    if (el.__on) listen(el, el.__on);
    for (var c = el.firstChild; c; c = c.nextSibling) wire(c);
  }
  function sameKind(a, b) {
    if (a.nodeType !== b.nodeType) return false;
    if (a.nodeType !== 1) return true;
    if (a.localName !== b.localName) return false;
    if ((a.id || b.id) && a.id !== b.id) return false;
    // un input non cambia tipo sul posto
    if (a.localName === 'input' && a.type !== b.type) return false;
    // un iframe con un altro documento si sostituisce, così non si mescolano
    if (a.localName === 'iframe' && (a.getAttribute('srcdoc') !== b.getAttribute('srcdoc') || a.getAttribute('data-src') !== b.getAttribute('data-src'))) return false;
    return true;
  }
  function morphAttrs(old, nw) {
    var i, a;
    var frame = old.localName === 'iframe';
    for (i = old.attributes.length - 1; i >= 0; i--) {
      a = old.attributes[i];
      // in un iframe src e altezza li mette il codice dopo il disegno (data-src, adattamento all'altezza)
      if (frame && (a.name === 'src' || a.name === 'style')) continue;
      if (!nw.hasAttribute(a.name)) old.removeAttribute(a.name);
    }
    for (i = 0; i < nw.attributes.length; i++) {
      a = nw.attributes[i];
      if (old.getAttribute(a.name) !== a.value) old.setAttribute(a.name, a.value);
    }
    var tag = old.localName;
    var focused = old === document.activeElement;
    if (tag === 'input') {
      if (old.type === 'checkbox' || old.type === 'radio') {
        var on = nw.hasAttribute('checked');
        if (old.checked !== on) old.checked = on;
      } else if (!focused) {
        var v = nw.getAttribute('value') || '';
        if (old.value !== v) old.value = v;
      }
    }
    if (tag === 'textarea' && !focused && old.value !== nw.textContent) old.value = nw.textContent;
    listen(old, nw.__on);
  }
  function morphChildren(parent, fresh) {
    var byId = {};
    var wanted = {};
    for (var c = parent.firstChild; c; c = c.nextSibling) if (c.nodeType === 1 && c.id) byId[c.id] = c;
    for (var k = 0; k < fresh.length; k++) if (fresh[k].nodeType === 1 && fresh[k].id) wanted[fresh[k].id] = true;
    var cur = parent.firstChild;
    for (var i = 0; i < fresh.length; i++) {
      var nw = fresh[i];
      var match = null;
      if (nw.nodeType === 1 && nw.id && byId[nw.id] && sameKind(byId[nw.id], nw)) match = byId[nw.id];
      else if (cur && !(cur.nodeType === 1 && cur.id && wanted[cur.id]) && sameKind(cur, nw)) match = cur;
      if (match) {
        if (match !== cur) parent.insertBefore(match, cur);
        else cur = cur.nextSibling;
        morph(match, nw);
      } else {
        wire(nw);
        parent.insertBefore(nw, cur);
      }
    }
    while (cur) {
      var next = cur.nextSibling;
      parent.removeChild(cur);
      cur = next;
    }
  }
  function morph(old, nw) {
    if (old.nodeType === 3) {
      if (old.nodeValue !== nw.nodeValue) old.nodeValue = nw.nodeValue;
      return;
    }
    if (old.nodeType !== 1) return;
    morphAttrs(old, nw);
    // dentro un iframe o un textarea non si entra
    if (old.localName === 'iframe' || old.localName === 'textarea') return;
    morphChildren(old, Array.prototype.slice.call(nw.childNodes));
  }

  function boot() {
    var host = document.querySelector('x-dc');
    var script = document.querySelector('script[data-dc-script]');
    if (!host || !script) return;

    var tpl = document.createElement('template');
    tpl.innerHTML = stash != null ? stash : host.innerHTML;

    var helmet = tpl.content.querySelector('helmet');
    if (helmet) {
      Array.prototype.forEach.call(helmet.children, function (n) {
        document.head.appendChild(document.importNode(n, true));
      });
    }

    var defs = {};
    try {
      defs = JSON.parse(script.getAttribute('data-props') || '{}');
    } catch (e) {
      console.error('[dc] data-props is not valid JSON', e);
    }
    var props = {};
    Object.keys(defs).forEach(function (k) {
      if (k.charAt(0) !== '$' && defs[k] && 'default' in defs[k]) props[k] = defs[k].default;
    });

    var Component = new Function('DCLogic', script.textContent + '\nreturn Component;')(DCLogic);
    var inst = new Component(props);
    if (!inst.state) inst.state = {};

    inst.__render = function () {
      var active = document.activeElement;
      var focusId = active && active.id;
      var sel = null;
      try {
        if (active && typeof active.selectionStart === 'number') sel = [active.selectionStart, active.selectionEnd];
      } catch (e) { /* inputs without a text selection */ }
      var pageY = window.scrollY;
      var scrolled = {};
      Array.prototype.forEach.call(host.querySelectorAll('[id]'), function (n) {
        if (n.scrollTop) scrolled[n.id] = n.scrollTop;
      });

      var t0 = performance.now();
      var nodes = renderAll(tpl.content.childNodes, inst.renderVals(), []);
      var t1 = performance.now();
      if (host.__morphed) morphChildren(host, nodes);
      else {
        nodes.forEach(wire);
        host.replaceChildren.apply(host, nodes);
        host.__morphed = true;
      }
      window.__dcTempi = { valori: t1 - t0, pagina: performance.now() - t1 };

      Object.keys(scrolled).forEach(function (id) {
        var n = document.getElementById(id);
        if (n) n.scrollTop = scrolled[id];
      });
      window.scrollTo(window.scrollX, pageY);

      if (focusId && document.activeElement !== active) {
        var again = document.getElementById(focusId);
        if (again) {
          again.focus();
          if (sel) try { again.setSelectionRange(sel[0], sel[1]); } catch (e) { /* not a text input */ }
        }
      }
    };

    var mounted = false;
    var render = inst.__render;
    inst.__render = function () {
      render();
      if (mounted && typeof inst.componentDidUpdate === 'function') inst.componentDidUpdate();
    };

    inst.__render();
    window.__dc = inst;
    host.setAttribute('data-ready', '');
    mounted = true;
    if (typeof inst.componentDidMount === 'function') inst.componentDidMount();
  }

  // ?data=<name> swaps the synthetic dataset for data/private/<name>.js, a local export
  // that never leaves this machine. If it is missing, no data is shown rather than
  // silently falling back to the synthetic set.
  function loadData(done) {
    // nell'app i dati arrivano dal motore, attraverso il ponte (ponte.js)
    if (window.A10S) {
      window.A10S.avvia().then(done, function (e) {
        window.AMB_DATA_ERROR = String((e && e.message) || e);
        done();
      });
      return;
    }
    var m = /[?&]data=([A-Za-z0-9_-]+)/.exec(window.location.search);
    if (!m || m[1] === 'synthetic') return done();
    window.AMB_DATA = null;
    var s = document.createElement('script');
    s.src = 'data/private/' + m[1] + '.js';
    s.onload = done;
    s.onerror = function () {
      window.AMB_DATA_ERROR = 'Non trovo data/private/' + m[1] + '.js: lancia prima tools/gmail-export.py.';
      done();
    };
    document.head.appendChild(s);
  }

  // mentre i dati arrivano dal motore, il modello non deve restare vivo nella pagina:
  // i suoi iframe e le sue immagini partirebbero con i buchi {{…}} ancora dentro
  var stash = null;
  function start() {
    var host = document.querySelector('x-dc');
    if (host && window.A10S) {
      stash = host.innerHTML;
      Array.prototype.slice.call(host.childNodes).forEach(function (n) {
        if (!(n.matches && n.matches('script[data-dc-script]'))) host.removeChild(n);
      });
    }
    loadData(boot);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
