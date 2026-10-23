// Operations on text for editing together: [keep, "insert", -delete], counted in UTF-16 units
// as a textarea counts. The same transform runs on the server (src/api/collab.rs).
const isIns = (p) => typeof p === 'string';
const plen = (p) => (isIns(p) ? p.length : Math.abs(p));

/** Append to an operation, merging neighbours of a kind and dropping empty parts. */
function push(op, p) {
  if (isIns(p) ? !p.length : !p) return;
  const last = op[op.length - 1];
  if (last !== undefined && isIns(last) === isIns(p) && (isIns(p) || Math.sign(last) === Math.sign(p))) op[op.length - 1] = last + p;
  else op.push(p);
}

const baseLen = (op) => op.reduce((n, p) => n + (isIns(p) ? 0 : Math.abs(p)), 0);

export function apply(s, op) {
  if (baseLen(op) !== s.length) throw new Error('change for another text');
  let out = '', at = 0;
  for (const p of op) {
    if (isIns(p)) out += p;
    else if (p > 0) { out += s.slice(at, at + p); at += p; } else at -= p;
  }
  return out;
}

/** The part of `p` after its first `m` units. */
const cut = (p, m) => (isIns(p) ? p.slice(m) : p > 0 ? p - m : p + m);

/** a and b made on one text → [a', b']: b' after a and a' after b meet; a's inserts go first. */
export function transform(a, b) {
  const a2 = [], b2 = [];
  let i = 0, j = 0, x = a[0], y = b[0];
  const nx = () => { x = a[++i]; }, ny = () => { y = b[++j]; };
  while (x !== undefined || y !== undefined) {
    if (x !== undefined && isIns(x)) { push(a2, x); push(b2, x.length); nx(); continue; }
    if (y !== undefined && isIns(y)) { push(a2, y.length); push(b2, y); ny(); continue; }
    if (x === undefined || y === undefined) throw new Error('changes for different texts');
    const m = Math.min(plen(x), plen(y));
    if (x > 0 && y > 0) { push(a2, m); push(b2, m); } else if (x < 0 && y > 0) push(a2, -m); else if (x > 0 && y < 0) push(b2, -m);
    if (plen(x) > m) x = cut(x, m); else nx();
    if (plen(y) > m) y = cut(y, m); else ny();
  }
  return [a2, b2];
}

/** a then b, as one operation. */
export function compose(a, b) {
  const out = [];
  let i = 0, j = 0, x = a[0], y = b[0];
  const nx = () => { x = a[++i]; }, ny = () => { y = b[++j]; };
  while (x !== undefined || y !== undefined) {
    if (x !== undefined && !isIns(x) && x < 0) { push(out, x); nx(); continue; }
    if (y !== undefined && isIns(y)) { push(out, y); ny(); continue; }
    if (x === undefined || y === undefined) throw new Error('changes that don\'t follow each other');
    const m = Math.min(plen(x), plen(y));
    if (isIns(x)) { if (y > 0) push(out, x.slice(0, m)); } else if (y > 0) push(out, m); else push(out, -m);
    if (plen(x) > m) x = cut(x, m); else nx();
    if (plen(y) > m) y = cut(y, m); else ny();
  }
  return out;
}

/** The operation turning `old` into `now` (one changed stretch, as typing makes). */
export function diff(old, now) {
  let p = 0;
  while (p < old.length && p < now.length && old[p] === now[p]) p++;
  let q = 0;
  while (q < old.length - p && q < now.length - p && old[old.length - 1 - q] === now[now.length - 1 - q]) q++;
  // Never between the halves of a character outside the BMP (an emoji): a lone half isn't text.
  const high = (s, i) => /[\ud800-\udbff]/.test(s[i] ?? ''), low = (s, i) => /[\udc00-\udfff]/.test(s[i] ?? '');
  if (p > 0 && high(old, p - 1)) p--;
  if (q > 0 && low(old, old.length - q)) q--;
  const op = [];
  push(op, p); push(op, -(old.length - p - q)); push(op, now.slice(p, now.length - q)); push(op, q);
  return op;
}

/** Where position `at` is after `op` (an insert there pushes it along). */
export function moved(at, op) {
  let old = 0, now = 0;
  for (const p of op) {
    if (isIns(p)) { now += p.length; continue; }
    const n = Math.abs(p);
    if (at < old + n) return p > 0 ? now + (at - old) : now;
    old += n;
    if (p > 0) now += n;
  }
  return now + Math.max(0, at - old);
}
