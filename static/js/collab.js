// Editing a file together. Local edits become operations ([keep, "insert", -delete], counted in
// UTF-16 units as a textarea counts) sent to the server, which puts everyone's in one order; the
// others' arrive over a stream and are transformed over our own unconfirmed ones before they're
// applied here. The same transform runs on the server (src/api/collab.rs).
import { html, send, get, esc, toast, state, confirmDialog } from './lib.js';

import { apply, transform, compose, diff, moved } from './ot.js';

const newClient = () => Math.random().toString(36).slice(2) + Date.now().toString(36);

/**
 * Join the session for `path` on `branch` and keep `ta` in step with it. `original` is the text as
 * loaded: edits made to it before joining are kept when the session still has that text.
 * Callbacks: `onPeople(list)` when who's here changes, `onAuthors(lines)` when who gets credit
 * changes, `onBase(sha, who)` when someone commits, `onStatus(text)` for the connection, `onEnd()`
 * when the session ends here (the text stays; editing goes on alone).
 * Returns null when the person chose not to join, else { base, coAuthors(), sync(), committed(sha), stop() }.
 */
export async function together({ api, branch, path, ta, layer, original, onPeople, onAuthors, onBase, onStatus, onEnd }) {
  const joined = await send('POST', `${api}/collab`, { branch, path });
  // Edits made before joining: kept if the session has the text they were made on, else asked.
  let carry = ta.value !== original ? diff(original, ta.value) : null;
  if (carry && joined.text !== original) {
    if (!await confirmDialog({ title: 'Others changed this file', text: 'Joining replaces your unsaved changes here with the text everyone is editing.', ok: 'Join and replace' })) return null;
    carry = null;
  }
  const url = `${api}/collab/${joined.id}`;
  const me = state.me?.username;
  let client = newClient(), revision = 0, outstanding = null, buffer = null, shadow = ta.value, ready = false, composing = false, ended = false;
  let people = joined.people, coAuthors = joined.co_authors, base = joined.base;
  const credited = new Set();
  const queued = [];

  // Others' cursors, drawn in a layer laid out like the text; names show while they're active.
  let frame = 0, fade;
  const paintPeers = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      const others = people.filter((p) => p.user !== me);
      if (!others.length) { layer.innerHTML = ''; return; }
      const now = Date.now();
      const marks = others.map((p) => ({ ...p, at: Math.min(p.cursor[1], shadow.length) })).sort((x, y) => x.at - y.at);
      let out = '', last = 0;
      for (const m of marks) {
        out += esc(shadow.slice(last, m.at)) + html`<span class="ed-peer peer-c${m.color % 8} ${now - (m.active || 0) < 3000 ? 'named' : ''}" data-name="${m.user}"></span>`.s;
        last = m.at;
      }
      layer.innerHTML = out + esc(shadow.slice(last)) + '\n';
      clearTimeout(fade);
      if (marks.some((m) => now - (m.active || 0) < 3000)) fade = setTimeout(paintPeers, 3000);
    });
  };
  const setPeople = (list) => { people = list; onPeople?.(people); paintPeers(); };
  const active = (user) => { const p = people.find((x) => x.user === user); if (p) p.active = Date.now(); };
  const setAuthors = (list) => { coAuthors = list; onAuthors?.(coAuthors); };

  const setText = (text) => {
    const s = ta.selectionStart, e = ta.selectionEnd;
    ta.value = text;
    shadow = text;
    ta.setSelectionRange(Math.min(s, text.length), Math.min(e, text.length));
    ta.dispatchEvent(new Event('input'));
  };
  const reset = (st) => {
    // A new client id: changes of the old one still on their way come back as someone else's.
    client = newClient();
    revision = st.version; outstanding = buffer = null; base = st.base;
    setAuthors(st.co_authors);
    if (ta.value !== st.text) setText(st.text);
    shadow = ta.value;
    setPeople(st.people);
    ready = true;
    ta.readOnly = false;
    onStatus?.('');
    if (carry) {
      const op = carry;
      carry = null;
      if (st.text !== original) toast('Someone typed as you joined: your unsaved changes were replaced', 'err');
      else { ta.value = apply(st.text, op); local(); ta.dispatchEvent(new Event('input')); }
    }
  };
  const end = (why) => {
    if (ended) return;
    stop();
    toast(`${why}. You're editing alone now; your text is still here.`, 'err');
    onEnd?.();
  };
  const resync = async (why) => {
    ready = false;
    try { reset(await get(url, { fresh: true })); } catch (e) { return end(`The live session ended (${e.message})`); }
    if (why) toast(why);
  };
  const sendOp = async (op) => {
    try { await send('POST', `${url}/ops`, { client, revision, op }); } catch (e) { resync(`Caught up with the others (${e.message})`); }
  };
  const local = () => {
    if (!ready || composing || ta.value === shadow) return;
    const op = diff(shadow, ta.value);
    shadow = ta.value;
    for (const p of people) p.cursor = [moved(p.cursor[0], op), moved(p.cursor[1], op)];
    if (outstanding) buffer = buffer ? compose(buffer, op) : op;
    else { outstanding = op; sendOp(op); }
    paintPeers();
  };
  const remote = (op) => {
    const s = ta.selectionStart, e = ta.selectionEnd, top = ta.scrollTop;
    shadow = apply(shadow, op);
    ta.value = shadow;
    ta.setSelectionRange(moved(s, op), moved(e, op));
    ta.scrollTop = top;
    for (const p of people) p.cursor = [moved(p.cursor[0], op), moved(p.cursor[1], op)];
    ta.dispatchEvent(new Event('input'));
  };
  // Where a cursor on the server's text is in ours (which has our unconfirmed changes too).
  const here = (c) => [outstanding, buffer].reduce((at, op) => (op ? moved(at, op) : at), c);

  const onChange = (ev) => {
    if (ev.type === 'op') {
      if (ev.version <= revision) return;
      if (ev.client === client && ev.user === me) {
        // Ours, confirmed: the next buffered change goes out on top of it.
        revision = ev.version;
        outstanding = buffer; buffer = null;
        if (outstanding) sendOp(outstanding);
        return;
      }
      let op = ev.op;
      if (outstanding) [outstanding, op] = transform(outstanding, op);
      if (buffer) [buffer, op] = transform(buffer, op);
      revision = ev.version;
      remote(op);
      active(ev.user);
      if (ev.user && ev.user !== me && !credited.has(ev.user)) {
        credited.add(ev.user);
        get(url, { fresh: true }).then((st) => setAuthors(st.co_authors)).catch(() => {});
      }
      paintPeers();
    } else if (ev.type === 'presence') {
      const cursor = [here(ev.cursor[0]), here(ev.cursor[1])];
      const p = people.find((x) => x.user === ev.user);
      if (p) { p.cursor = cursor; p.color = ev.color ?? p.color; } else people.push({ user: ev.user, color: ev.color ?? 0, cursor });
      active(ev.user);
      setPeople(people);
    } else if (ev.type === 'leave') {
      setPeople(people.filter((x) => x.user !== ev.user));
    } else if (ev.type === 'base') {
      base = ev.sha;
      onBase?.(ev.sha, ev.user);
    }
  };

  ta.readOnly = true;
  const onInput = () => local();
  const onStart = () => { composing = true; };
  // Others' changes wait while an input method is composing (writing the text would break it).
  const onEndCompose = () => { composing = false; local(); for (const ev of queued.splice(0)) onChange(ev); };
  ta.addEventListener('input', onInput);
  ta.addEventListener('compositionstart', onStart);
  ta.addEventListener('compositionend', onEndCompose);
  let ptimer;
  const where = () => {
    clearTimeout(ptimer);
    ptimer = setTimeout(() => {
      if (ready) send('POST', `${url}/presence`, { cursor: [ta.selectionStart, ta.selectionEnd] }).catch(() => {});
    }, 250);
  };
  const cursorEvents = ['keyup', 'mouseup', 'focus', 'select'];
  for (const ev of cursorEvents) ta.addEventListener(ev, where);

  onStatus?.('Connecting…');
  const es = new EventSource(`/api${url}/events`);
  const slow = setTimeout(() => { if (!ready) end("Couldn't reach the live session"); }, 15000);
  es.addEventListener('init', (e) => {
    if (ready && (outstanding || buffer)) toast('Reconnected: check your last few keystrokes');
    reset(JSON.parse(e.data));
  });
  es.addEventListener('resync', () => resync('Caught up with the others'));
  es.addEventListener('change', (e) => {
    const ev = JSON.parse(e.data);
    if (composing) queued.push(ev);
    else onChange(ev);
  });
  es.onerror = () => {
    if (ended) return;
    // Closed for good (the session is gone, or access was taken away): carry on alone.
    if (es.readyState === EventSource.CLOSED) return end('The live session ended');
    ready = false;
    ta.readOnly = true;
    onStatus?.('Reconnecting…');
  };

  function stop() {
    ended = true;
    es.close();
    clearTimeout(ptimer); clearTimeout(slow); clearTimeout(fade);
    if (frame) cancelAnimationFrame(frame);
    ta.removeEventListener('input', onInput);
    ta.removeEventListener('compositionstart', onStart);
    ta.removeEventListener('compositionend', onEndCompose);
    for (const ev of cursorEvents) ta.removeEventListener(ev, where);
    ta.readOnly = false;
    layer.innerHTML = '';
    onStatus?.('');
  }
  return {
    get base() { return base; },
    coAuthors: () => coAuthors,
    /** The latest base and credits (the server carries the session on to the branch's head when it can). */
    async sync() { const st = await get(url, { fresh: true }); base = st.base; setAuthors(st.co_authors); return base; },
    committed: (sha) => send('POST', `${url}/committed`, { sha }).catch((e) => toast(`The others weren't told about your commit (${e.message}); theirs will be checked against the branch`, 'err')),
    stop,
  };
}
