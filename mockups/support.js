/*
 * Local runtime for the .dc.html mockups, so they open and navigate from disk.
 * The Claude Design canvas serves its own support.js: never publish this file.
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
      // sc-src, sc-onload…: the page markup is live in <x-dc> before the first render, so a plain
      // src or onload with a {{hole}} would load or run as written; the sc- prefix keeps them inert
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
          el.addEventListener(ev, fn);
        }
        return;
      }
      if (attr.value.indexOf('{{') === -1) return;
      var v = WHOLE.test(attr.value) ? whole(attr.value, scope) : interp(attr.value, scope);
      if (v == null || (v === false && name.indexOf('aria-') !== 0)) el.removeAttribute(name);
      else el.setAttribute(name, String(v));
    });
    if (tag === 'form') {
      el.addEventListener('submit', function (e) { e.preventDefault(); });
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

  function boot() {
    var host = document.querySelector('x-dc');
    var script = document.querySelector('script[data-dc-script]');
    if (!host || !script) return;

    var tpl = document.createElement('template');
    tpl.innerHTML = host.innerHTML;

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

      var nodes = renderAll(tpl.content.childNodes, inst.renderVals(), []);
      host.replaceChildren.apply(host, nodes);

      Object.keys(scrolled).forEach(function (id) {
        var n = document.getElementById(id);
        if (n) n.scrollTop = scrolled[id];
      });
      window.scrollTo(window.scrollX, pageY);

      if (focusId) {
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
    host.setAttribute('data-ready', '');
    mounted = true;
    if (typeof inst.componentDidMount === 'function') inst.componentDidMount();
  }

  // ?data=<name> swaps the synthetic dataset for data/private/<name>.js, a local export
  // that never leaves this machine. If it is missing, no data is shown rather than
  // silently falling back to the synthetic set.
  function loadData(done) {
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

  function start() {
    loadData(boot);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
