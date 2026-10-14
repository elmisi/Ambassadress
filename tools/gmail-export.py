#!/usr/bin/env python3
"""Export Gmail messages, as they are, into the data file the prototype reads locally.

The output never leaves this machine: it is gitignored and no LLM may read it.
The script prints only counts, never subjects, addresses or text.

Sources:
  IMAP (default)   reads the latest N messages of "All Mail", read-only.
  --mbox FILE      reads a Google Takeout mbox instead.

Credentials for IMAP (one line each, never printed):
  ~/.secrets/gmail-user.txt           the Gmail address
  ~/.secrets/gmail-app-password.txt   an app password (Google account > Security >
                                      2-Step Verification > App passwords)

Only Python's standard library is used.
"""
import argparse
import base64
import datetime
import email
import email.policy
import email.utils
import hashlib
import imaplib
import json
import mailbox
import os
import re
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_OUT = os.path.join(REPO, 'mockups', 'data', 'private', 'real.js')
SECRETS = os.path.expanduser('~/.secrets')
SCHEMA = 1


def read_secret(name):
    path = os.path.join(SECRETS, name)
    try:
        with open(path, encoding='utf-8') as f:
            value = ''.join(f.read().split())
    except FileNotFoundError:
        sys.exit(f'Manca {path}: crealo con una sola riga (vedi l\'intestazione dello script).')
    if not value:
        sys.exit(f'{path} è vuoto.')
    return value


def imap_utf7_decode(s):
    """Decode IMAP modified UTF-7 (RFC 3501), used for label and mailbox names."""
    out, i = [], 0
    while i < len(s):
        if s[i] == '&':
            j = s.index('-', i)
            if j == i + 1:
                out.append('&')
            else:
                b64 = s[i + 1:j].replace(',', '/')
                b64 += '=' * (-len(b64) % 4)
                out.append(base64.b64decode(b64).decode('utf-16-be'))
            i = j + 1
        else:
            out.append(s[i])
            i += 1
    return ''.join(out)


def parse_list(text, key):
    """Return the items of a parenthesised IMAP list after KEY, honouring quotes."""
    start = text.find(key + ' (')
    if start < 0:
        return []
    i = start + len(key) + 2
    items, cur, quoted, depth = [], '', False, 1
    while i < len(text):
        c = text[i]
        if quoted:
            if c == '\\' and i + 1 < len(text):
                cur += text[i + 1]
                i += 2
                continue
            if c == '"':
                quoted = False
            else:
                cur += c
        elif c == '"':
            quoted = True
        elif c == '(':
            depth += 1
        elif c == ')':
            depth -= 1
            if depth == 0:
                break
        elif c == ' ':
            if cur:
                items.append(cur)
            cur = ''
        else:
            cur += c
        i += 1
    if cur:
        items.append(cur)
    return [imap_utf7_decode(x) for x in items]


def find_all_mail(conn):
    typ, data = conn.list()
    for line in data or []:
        text = line.decode('utf-8', 'replace') if isinstance(line, bytes) else str(line)
        m = re.match(r'\((?P<flags>[^)]*)\) "(?P<sep>[^"]*)" (?P<name>.*)$', text)
        if m and '\\All' in m.group('flags'):
            return m.group('name').strip()
    sys.exit('Non trovo la cartella "Tutti i messaggi" (flag \\All). Usa --mailbox.')


def say(text):
    print(text, file=sys.stderr, flush=True)


def duration(seconds):
    seconds = int(seconds)
    return f'{seconds // 60} min {seconds % 60:02d} s' if seconds >= 60 else f'{seconds} s'


def progress(done, total, nbytes, started):
    elapsed = max(time.monotonic() - started, 0.001)
    speed = nbytes / elapsed
    left = (elapsed / done) * (total - done) if done else 0
    line = (f'\r{done}/{total} email · {nbytes / 1048576:.1f} MB · '
            f'{speed / 1024:.0f} KB/s · trascorsi {duration(elapsed)} · mancano circa {duration(left)}   ')
    print(line, end='', file=sys.stderr, flush=True)


def imap_source(limit, mailbox_name):
    user = read_secret('gmail-user.txt')
    password = read_secret('gmail-app-password.txt')
    say('Connessione a imap.gmail.com…')
    conn = imaplib.IMAP4_SSL('imap.gmail.com')
    try:
        conn.login(user, password)
    except imaplib.IMAP4.error:
        sys.exit('Accesso IMAP rifiutato: controlla indirizzo e password per le app.')
    del password
    say('Accesso fatto. Cerco la cartella con tutti i messaggi…')
    box = mailbox_name or find_all_mail(conn)
    typ, _ = conn.select(box, readonly=True)
    if typ != 'OK':
        sys.exit('Non riesco ad aprire la cartella in sola lettura.')
    say(f'Cartella {imap_utf7_decode(box)} aperta in sola lettura. Elenco i messaggi…')
    typ, data = conn.uid('SEARCH', None, 'ALL')
    all_uids = data[0].split()
    uids = all_uids[-limit:]
    total = len(uids)
    say(f'{len(all_uids)} messaggi nella casella: scarico i {total} più recenti, allegati compresi.')
    done = 0
    nbytes = 0
    started = time.monotonic()
    progress(0, total, 0, started)
    for k in range(0, total, 10):
        chunk = uids[k:k + 10]
        typ, resp = conn.uid('FETCH', b','.join(chunk), '(X-GM-THRID X-GM-MSGID X-GM-LABELS FLAGS INTERNALDATE BODY.PEEK[])')
        for item in resp:
            if not isinstance(item, tuple):
                continue
            meta = item[0].decode('utf-8', 'replace')
            raw = item[1]
            thr = re.search(r'X-GM-THRID (\d+)', meta)
            mid = re.search(r'X-GM-MSGID (\d+)', meta)
            idate = re.search(r'INTERNALDATE "([^"]+)"', meta)
            internal = None
            if idate:
                try:
                    internal = datetime.datetime.strptime(idate.group(1), '%d-%b-%Y %H:%M:%S %z').isoformat()
                except ValueError:
                    internal = None
            yield raw, {
                'threadId': thr.group(1) if thr else None,
                'msgId': mid.group(1) if mid else None,
                'labels': parse_list(meta, 'X-GM-LABELS'),
                'flags': parse_list(meta, 'FLAGS'),
                'internalDate': internal,
            }
            done += 1
            nbytes += len(raw)
            progress(done, total, nbytes, started)
    print(file=sys.stderr)
    say(f'Scaricato tutto in {duration(time.monotonic() - started)}. Preparo il file…')
    conn.logout()
    return user


def mbox_source(path, limit):
    say(f'Leggo {path}…')
    box = mailbox.mbox(path, create=False)
    keyed = []
    for key in box.keys():
        raw = box.get_bytes(key)
        head = email.message_from_bytes(raw.split(b'\r\n\r\n', 1)[0].split(b'\n\n', 1)[0], policy=email.policy.compat32)
        try:
            ts = email.utils.parsedate_to_datetime(head.get('Date', '')).timestamp()
        except (TypeError, ValueError, IndexError):
            ts = 0
        keyed.append((ts, key))
    keyed.sort()
    chosen = keyed[-limit:]
    say(f'{len(keyed)} messaggi nel file: tengo i {len(chosen)} più recenti.')
    started = time.monotonic()
    nbytes = 0
    for done, (ts, key) in enumerate(chosen, 1):
        raw = box.get_bytes(key)
        nbytes += len(raw)
        progress(done, len(chosen), nbytes, started)
        if done == len(chosen):
            print(file=sys.stderr)
        head = email.message_from_bytes(raw, policy=email.policy.compat32)
        labels = [x.strip() for x in str(head.get('X-Gmail-Labels', '')).split(',') if x.strip()]
        flags = []
        if 'Non letti' not in labels and 'Unread' not in labels:
            flags.append('\\Seen')
        yield raw, {
            'threadId': str(head.get('X-GM-THRID', '')) or None,
            'msgId': None,
            'labels': labels,
            'flags': flags,
            'internalDate': None,
        }


def addresses(msg, name, problems):
    try:
        values = [str(v) for v in msg.get_all(name, [])]
    except Exception as e:  # malformed header that the parser cannot render
        problems.append(f'header {name}: {type(e).__name__}')
        return []
    out = []
    for display, addr in email.utils.getaddresses(values):
        if display or addr:
            out.append({'name': display, 'address': addr})
    return out


def part_text(part, problems, label):
    try:
        return part.get_content()
    except (LookupError, UnicodeError, AssertionError) as e:
        problems.append(f'{label}: {type(e).__name__}')
        payload = part.get_payload(decode=True) or b''
        return payload.decode('utf-8', 'replace')


DATA_DIR = os.path.join(REPO, 'mockups', 'data')


def safe_name(name, fallback):
    name = re.sub(r'[\x00-\x1f/\\:*?"<>|]+', '_', name or '').strip(' .')
    return (name or fallback)[:120]


def save_attachment(save_dir, msg_key, index, filename, payload):
    """Write one attachment under save_dir and return its path relative to mockups/data."""
    folder = os.path.join(save_dir, safe_name(msg_key, 'msg'))
    os.makedirs(folder, mode=0o700, exist_ok=True)
    path = os.path.join(folder, f'{index:02d}-' + safe_name(filename, 'allegato'))
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'wb') as f:
        f.write(payload)
    return os.path.relpath(path, DATA_DIR)


def convert(raw, meta, max_kb, save_dir=None):
    problems = []
    digest = hashlib.sha1(raw).hexdigest()[:16]
    msg_key = meta.get('msgId') or digest
    try:
        msg = email.message_from_bytes(raw, policy=email.policy.default)
    except Exception as e:
        return {'id': meta.get('msgId') or digest, 'problems': [f'parse: {type(e).__name__}'], 'size': len(raw)}
    for defect in msg.defects:
        problems.append(f'defect: {type(defect).__name__}')

    headers = []
    for k in msg.keys():
        try:
            values = msg.get_all(k, [])
            for v in values:
                headers.append([k, str(v)])
        except Exception as e:
            problems.append(f'header {k}: {type(e).__name__}')
    seen_names = set()
    unique_headers = []
    for h in headers:
        key = (h[0], h[1])
        if key not in seen_names:
            seen_names.add(key)
            unique_headers.append(h)

    date_raw = str(msg.get('Date', '') or '')
    date = None
    try:
        date = email.utils.parsedate_to_datetime(date_raw).isoformat()
    except (TypeError, ValueError, IndexError):
        problems.append('date: non leggibile')

    text = html = None
    attachments, other_parts = [], []
    for part in msg.walk():
        if part.is_multipart():
            continue
        ctype = part.get_content_type()
        disp = part.get_content_disposition()
        filename = None
        try:
            filename = part.get_filename()
        except Exception as e:
            problems.append(f'filename: {type(e).__name__}')
        if filename or disp == 'attachment':
            payload = part.get_payload(decode=True) or b''
            item = {
                'filename': filename or '',
                'mimeType': ctype,
                'size': len(payload),
                'inline': disp == 'inline',
                'contentId': str(part.get('Content-ID', '') or '') or None,
            }
            if save_dir:
                try:
                    item['path'] = save_attachment(save_dir, msg_key, len(attachments), filename, payload)
                except OSError as e:
                    problems.append(f'allegato non salvato: {type(e).__name__}')
            attachments.append(item)
        elif ctype == 'text/plain' and text is None:
            text = part_text(part, problems, 'text/plain')
        elif ctype == 'text/html' and html is None:
            html = part_text(part, problems, 'text/html')
        else:
            other_parts.append(ctype)

    truncated = False
    limit = max_kb * 1024
    if text and len(text) > limit:
        text, truncated = text[:limit], True
    if html and len(html) > limit:
        html, truncated = html[:limit], True

    def one(name):
        v = msg.get(name)
        return str(v) if v is not None else None

    flags = meta.get('flags') or []
    return {
        'id': meta.get('msgId') or one('Message-ID') or digest,
        'date': date,
        'dateRaw': date_raw,
        'internalDate': meta.get('internalDate'),
        'from': (addresses(msg, 'From', problems) or [None])[0],
        'sender': (addresses(msg, 'Sender', problems) or [None])[0],
        'replyTo': addresses(msg, 'Reply-To', problems),
        'to': addresses(msg, 'To', problems),
        'cc': addresses(msg, 'Cc', problems),
        'bcc': addresses(msg, 'Bcc', problems),
        'subject': one('Subject'),
        'messageId': one('Message-ID'),
        'inReplyTo': one('In-Reply-To'),
        'references': (one('References') or '').split(),
        'text': text,
        'html': html,
        'truncated': truncated,
        'attachments': attachments,
        'otherParts': other_parts,
        'headers': unique_headers,
        'flags': {'seen': '\\Seen' in flags, 'flagged': '\\Flagged' in flags},
        'gmail': {'threadId': meta.get('threadId'), 'labels': meta.get('labels') or []},
        'size': len(raw),
        'problems': problems,
    }


def main():
    ap = argparse.ArgumentParser(description='Esporta email da Gmail per il prototipo (solo in locale).')
    ap.add_argument('--limit', type=int, default=1000, help='quanti messaggi, i più recenti (default 1000)')
    ap.add_argument('--mailbox', help='cartella IMAP (default: Tutti i messaggi)')
    ap.add_argument('--mbox', help='leggi un file mbox di Google Takeout invece di IMAP')
    ap.add_argument('--alias', action='append', default=[], help='altri tuoi indirizzi (ripetibile)')
    ap.add_argument('--max-body-kb', type=int, default=512, help='tronca testo e HTML oltre questa misura')
    ap.add_argument('--out', default=DEFAULT_OUT, help='file di uscita (default mockups/data/private/real.js)')
    ap.add_argument('--no-attachments', action='store_true', help='non salvare gli allegati su disco (solo nome, tipo e dimensione)')
    args = ap.parse_args()
    save_dir = None if args.no_attachments else os.path.join(os.path.dirname(os.path.abspath(args.out)), 'attachments')

    records = []
    account = None
    if args.mbox:
        source = mbox_source(args.mbox, args.limit)
    else:
        source = imap_source(args.limit, args.mailbox)
    while True:
        try:
            raw, meta = next(source)
        except StopIteration as stop:
            account = stop.value
            break
        records.append(convert(raw, meta, args.max_body_kb, save_dir))

    data = {
        'schema': SCHEMA,
        'meta': {
            'source': 'mbox' if args.mbox else 'gmail-imap',
            'exportedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'count': len(records),
            'shiftToNow': False,
        },
        'account': {'address': account or '', 'aliases': args.alias},
        'messages': records,
    }

    out = os.path.abspath(args.out)
    os.makedirs(os.path.dirname(out), mode=0o700, exist_ok=True)
    tmp = out + '.tmp'
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w', encoding='utf-8') as f:
        f.write('window.AMB_DATA = ')
        json.dump(data, f, ensure_ascii=False)
        f.write(';\n')
    os.replace(tmp, out)

    with_problems = sum(1 for r in records if r.get('problems'))
    size_mb = os.path.getsize(out) / 1024 / 1024
    print(f'Esportati {len(records)} messaggi in {out} ({size_mb:.1f} MB).')
    if save_dir:
        saved = sum(1 for r in records for a in r.get('attachments', []) if a.get('path'))
        print(f'Allegati salvati: {saved}, in {save_dir}.')
    print(f'Messaggi con problemi di lettura: {with_problems}.')
    print('Apri il prototipo con ?data=real per usarli.')


if __name__ == '__main__':
    main()
