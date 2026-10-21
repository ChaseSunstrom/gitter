// Hidden and look-alike Unicode in code: bidirectional controls ("Trojan
// Source"), invisible characters, and words mixing Latin with Cyrillic or Greek
// look-alikes. Code views mark them and warn above the code.

const NAMES = {
  0x202a: 'LEFT-TO-RIGHT EMBEDDING', 0x202b: 'RIGHT-TO-LEFT EMBEDDING', 0x202c: 'POP DIRECTIONAL FORMATTING',
  0x202d: 'LEFT-TO-RIGHT OVERRIDE', 0x202e: 'RIGHT-TO-LEFT OVERRIDE', 0x2066: 'LEFT-TO-RIGHT ISOLATE',
  0x2067: 'RIGHT-TO-LEFT ISOLATE', 0x2068: 'FIRST STRONG ISOLATE', 0x2069: 'POP DIRECTIONAL ISOLATE',
  0x200e: 'LEFT-TO-RIGHT MARK', 0x200f: 'RIGHT-TO-LEFT MARK', 0x061c: 'ARABIC LETTER MARK',
  0x200b: 'ZERO WIDTH SPACE', 0x200c: 'ZERO WIDTH NON-JOINER', 0x200d: 'ZERO WIDTH JOINER', 0x2060: 'WORD JOINER',
  0x2061: 'FUNCTION APPLICATION', 0x2062: 'INVISIBLE TIMES', 0x2063: 'INVISIBLE SEPARATOR', 0x2064: 'INVISIBLE PLUS',
  0xfeff: 'ZERO WIDTH NO-BREAK SPACE', 0x00ad: 'SOFT HYPHEN', 0x180e: 'MONGOLIAN VOWEL SEPARATOR',
  0x3164: 'HANGUL FILLER', 0x115f: 'HANGUL CHOSEONG FILLER', 0x1160: 'HANGUL JUNGSEONG FILLER', 0xffa0: 'HALFWIDTH HANGUL FILLER',
};
const BIDI = new Set([0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068, 0x2069, 0x200e, 0x200f, 0x061c]);
const HIDDEN = /[‪-‮⁦-⁩‎‏؜​-‍⁠-⁤﻿­᠎ㅤᅟᅠﾠ]/gu;
// A Cyrillic or Greek letter touching a Latin one: "pаypal" with a Cyrillic "а".
const MIXED = /(?<=\p{Script=Latin})[\p{Script=Cyrillic}\p{Script=Greek}]|[\p{Script=Cyrillic}\p{Script=Greek}](?=\p{Script=Latin})/gu;
const EMOJI = /\p{Extended_Pictographic}/u;

const hex = (c) => 'U+' + c.toString(16).toUpperCase().padStart(4, '0');

/** A zero-width joiner between emoji is how 👩‍💻 is spelled, not a hidden character. */
function legit(cp, s, i, atStart) {
  if (cp === 0x200d) return EMOJI.test(String.fromCodePoint(s.codePointAt(i - 2) ?? 0) + (s[i - 1] || '')) || EMOJI.test(s[i + 1] || '');
  return cp === 0xfeff && atStart && i === 0; // a byte-order mark at the very start
}

/** Count what's hidden or confusable in `text`. */
export function scan(text) {
  const s = String(text).slice(0, 2_000_000);
  const out = { bidi: 0, hidden: 0, mixed: [] };
  for (const m of s.matchAll(HIDDEN)) {
    const cp = m[0].codePointAt(0);
    if (legit(cp, s, m.index, true)) continue;
    if (BIDI.has(cp)) out.bidi++; else out.hidden++;
  }
  const seen = new Set();
  for (const m of s.matchAll(/[\p{L}\p{M}]+/gu)) {
    const w = m[0];
    if (w.length < 64 && !seen.has(w) && (w.match(MIXED) || []).length) { seen.add(w); out.mixed.push(w); if (out.mixed.length >= 5) break; }
  }
  return out;
}

export const found = (r) => r.bidi + r.hidden + r.mixed.length > 0;

/** Replace hidden characters in an HTML string with visible markers, and flag look-alike letters. */
export function revealHidden(h, atStart = false) {
  if (!h) return h;
  return h
    .replace(HIDDEN, (c, i, s) => {
      const cp = c.codePointAt(0);
      if (legit(cp, s, i, atStart)) return c;
      return `<span class="uni${BIDI.has(cp) ? ' bidi' : ''}" data-cp="${hex(cp)}" title="${hex(cp)} ${NAMES[cp]}"></span>`;
    })
    .replace(MIXED, (c) => `<span class="uni-mix" title="${hex(c.codePointAt(0))}: a look-alike letter from another alphabet">${c}</span>`);
}

/** The warning shown above code that has any of these. Returns '' when the text is clean. */
export function warning(r) {
  if (!found(r)) return '';
  const parts = [];
  if (r.bidi) parts.push(`${r.bidi} bidirectional control ${r.bidi === 1 ? 'character' : 'characters'}, so the code may run in a different order than it reads`);
  if (r.hidden) parts.push(`${r.hidden} invisible ${r.hidden === 1 ? 'character' : 'characters'}`);
  if (r.mixed.length) parts.push(`words that mix alphabets, like “${r.mixed.slice(0, 3).join('”, “')}”`);
  const esc = (x) => x.replace(/[&<>"]/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[ch]);
  return `<div class="uni-warn" role="note"><b>Hidden or look-alike characters.</b> This has ${esc(parts.join('; '))}. They're marked below.</div>`;
}
