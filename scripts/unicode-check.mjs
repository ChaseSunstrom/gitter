// Self-check for static/js/unicode.js: finds each kind of hidden or look-alike
// text, leaves ordinary text (and emoji) alone. Run: node scripts/unicode-check.mjs
import assert from 'node:assert/strict';
import { scan, revealHidden, warning, found } from '../static/js/unicode.js';

// Trojan Source: the comment hides a return.
const trojan = 'if (access_level != "user‮ ⁦// Check if admin⁩ ⁦") {';
const t = scan(trojan);
assert.equal(t.bidi, 4);
assert.match(revealHidden(trojan), /data-cp="U\+202E"/);
assert.doesNotMatch(revealHidden(trojan), /‮/, 'the control itself is removed from the display');

assert.equal(scan('let a​ = 1;').hidden, 1);
assert.deepEqual(scan('if (pаypal.ok) {}').mixed, ['pаypal']); // Cyrillic а
assert.match(revealHidden('pаypal'), /class="uni-mix"[^>]*>а</);

for (const ok of ['plain ascii code();', 'Привет, мир', 'Ελληνικά', 'naïve café', '👩‍💻 ships code', '﻿starts with a BOM']) {
  assert.ok(!found(scan(ok)), `clean: ${ok}`);
  assert.equal(warning(scan(ok)), '');
}
assert.equal(revealHidden('👩‍💻'), '👩‍💻');
assert.match(warning(t), /bidirectional/);
console.log('✓ unicode detector: bidi, invisible and mixed-script text found; normal text untouched');
