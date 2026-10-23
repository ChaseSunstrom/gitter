// Checks static/js/ot.js: transform, compose and diff on random texts, then several clients typing
// at once through a model of the server (src/api/collab.rs) and the browser client (static/js/collab.js):
// everyone must end with the server's text. Run: node scripts/ot-check.mjs
import { apply, transform, compose, diff, moved } from '../static/js/ot.js';

let seed = 0x2545f491;
const rnd = (n) => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return (seed >>> 0) % Math.max(1, n); };
const pick = (a) => a[rnd(a.length)];
const words = ['a', 'bc', 'é', '😀', '\n', 'xyz'];

function randomEdit(s) {
  // One stretch replaced, as typing, pasting or deleting a selection does.
  const at = rnd(s.length + 1), del = rnd(Math.min(4, s.length - at) + 1);
  return s.slice(0, at) + (rnd(3) ? pick(words) : '') + s.slice(at + del);
}
function randomOp(s) { return diff(s, randomEdit(s)); }
const eq = (a, b, what) => { if (a !== b) { console.error(`✗ ${what}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`); process.exit(1); } };

for (let i = 0; i < 5000; i++) {
  const s = 'hello world'.slice(0, rnd(12));
  const now = randomEdit(s);
  eq(apply(s, diff(s, now)), now, 'diff');
  const a = randomOp(s), b = randomOp(s);
  const [a2, b2] = transform(a, b);
  eq(apply(apply(s, a), b2), apply(apply(s, b), a2), `transform ${JSON.stringify([s, a, b])}`);
  const c = randomOp(apply(s, a));
  eq(apply(s, compose(a, c)), apply(apply(s, a), c), 'compose');
}
// Changes travel as JSON, which has no lone halves of an emoji: diff never makes one.
const lone = /[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/;
for (const [s, now] of [['😀', '😁'], ['a😀b', 'a😁😀b'], ['😀😀', '😀'], ['x😀', 'x😀😁'], ['😁😀', '😀']]) {
  const op = diff(s, now);
  eq(apply(s, op), now, `diff ${s} -> ${now}`);
  eq(op.some((x) => typeof x === 'string' && lone.test(x)), false, `whole characters in ${JSON.stringify(op)}`);
  // Deleting or keeping half a character would leave a lone half behind for the next change.
  let at = 0;
  for (const x of op) {
    if (typeof x === 'number') { at += Math.abs(x); eq(/[\udc00-\udfff]/.test(s[at] ?? ''), false, `cut inside a character: ${JSON.stringify(op)}`); }
  }
}
eq(moved(1, [1, 'XY', 1]), 3, 'moved past an insert');
eq(moved(1, [-2]), 0, 'moved into a delete');

// Clients and a server exchanging messages in random order (each direction in order).
for (let run = 0; run < 300; run++) {
  const server = { text: 'shared text', version: 0, history: [] };
  const clients = Array.from({ length: 2 + rnd(3) }, (_, id) => ({ id, text: server.text, revision: 0, outstanding: null, buffer: null, inbox: [], outbox: [] }));
  const broadcast = (ev) => clients.forEach((c) => c.inbox.push(ev));
  const serverGet = ({ id, revision, op }) => {
    for (const h of server.history.slice(revision)) op = transform(op, h)[0];
    server.text = apply(server.text, op);
    server.version++;
    server.history.push(op);
    broadcast({ version: server.version, op, client: id });
  };
  const edit = (c) => {
    const op = randomOp(c.text);
    c.text = apply(c.text, op);
    if (c.outstanding) c.buffer = c.buffer ? compose(c.buffer, op) : op;
    else { c.outstanding = op; c.outbox.push({ id: c.id, revision: c.revision, op }); }
  };
  const hear = (c) => {
    const ev = c.inbox.shift();
    if (ev.client === c.id) {
      c.revision = ev.version;
      c.outstanding = c.buffer; c.buffer = null;
      if (c.outstanding) c.outbox.push({ id: c.id, revision: c.revision, op: c.outstanding });
      return;
    }
    let op = ev.op;
    if (c.outstanding) [c.outstanding, op] = transform(c.outstanding, op);
    if (c.buffer) [c.buffer, op] = transform(c.buffer, op);
    c.revision = ev.version;
    c.text = apply(c.text, op);
  };
  for (let step = 0; step < 200; step++) {
    const c = pick(clients);
    const k = rnd(3);
    if (k === 0) edit(c);
    else if (k === 1 && c.outbox.length) serverGet(c.outbox.shift());
    else if (c.inbox.length) hear(c);
  }
  // Drain: everything sent arrives.
  while (clients.some((c) => c.outbox.length || c.inbox.length)) {
    for (const c of clients) { if (c.outbox.length) serverGet(c.outbox.shift()); }
    for (const c of clients) { while (c.inbox.length) hear(c); }
  }
  for (const c of clients) eq(c.text, server.text, `client ${c.id} after run ${run}`);
}
console.log('✓ ot: transform, compose and diff hold; clients typing at once all end with the server\'s text');
