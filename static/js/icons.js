// Inline stroke icons (24x24 grid). icon('star') -> Safe SVG markup.
import { raw } from './lib.js';

const P = {
  repo: '<path d="M4 19.5V5a2 2 0 0 1 2-2h13v14H6.5A2.5 2.5 0 0 0 4 19.5Zm0 0A2.5 2.5 0 0 0 6.5 22H19v-5"/><path d="M8 7h7"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z"/><path d="M14 3v5h5"/>',
  'file-code': '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z"/><path d="M14 3v5h5"/><path d="m10 12-2 2 2 2M14 12l2 2-2 2"/>',
  symlink: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z"/><path d="M14 3v5h5"/><path d="M9 17v-2a3 3 0 0 1 3-3h3m-2-2 2 2-2 2"/>',
  submodule: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/><path d="M12 11v5m-2.5-2.5h5"/>',
  branch: '<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="7" r="2"/><path d="M6 7v10M18 9a6 6 0 0 1-6 6H6"/>',
  commit: '<circle cx="12" cy="12" r="3.5"/><path d="M3 12h5.5M15.5 12H21"/>',
  tag: '<path d="M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9Z"/><circle cx="7.5" cy="7.5" r="1.5"/>',
  star: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9Z"/>',
  fork: '<circle cx="6" cy="5" r="2"/><circle cx="18" cy="5" r="2"/><circle cx="12" cy="19" r="2"/><path d="M6 7v1a3 3 0 0 0 3 3h6a3 3 0 0 0 3-3V7M12 11v6"/>',
  issue: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="1.5" fill="currentColor"/>',
  'issue-closed': '<circle cx="12" cy="12" r="9"/><path d="m8.5 12 2.5 2.5 4.5-5"/>',
  pr: '<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="19" r="2"/><path d="M6 7v10M18 17V9a3 3 0 0 0-3-3h-4m2-2-2 2 2 2"/>',
  'pr-closed': '<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="19" r="2"/><path d="M6 7v10M18 17v-5M16 4l4 4m0-4-4 4"/>',
  merged: '<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="12" r="2"/><path d="M6 7v10M6 7a7 7 0 0 0 10 5"/>',
  code: '<path d="m8 7-5 5 5 5M16 7l5 5-5 5"/>',
  history: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
  lock: '<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z"/>',
  book: '<path d="M2 4h6a4 4 0 0 1 4 4v13a3 3 0 0 0-3-3H2Z"/><path d="M22 4h-6a4 4 0 0 0-4 4v13a3 3 0 0 1 3-3h7Z"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  'chev-down': '<path d="m6 9 6 6 6-6"/>',
  'chev-up': '<path d="m6 15 6-6 6 6"/>',
  'chev-right': '<path d="m9 6 6 6-6 6"/>',
  'chev-left': '<path d="m15 6-6 6 6 6"/>',
  download: '<path d="M12 4v11m-5-5 5 5 5-5M4 20h16"/>',
  upload: '<path d="M12 20V9m-5 5 5-5 5 5M4 4h16"/>',
  edit: '<path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16Z"/><path d="m13.5 6.5 4 4"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13M9 7V4h6v3"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
  users: '<circle cx="9" cy="8" r="3.5"/><path d="M2 20a7 7 0 0 1 14 0M16 4.5a3.5 3.5 0 0 1 0 7M18 13.5a7 7 0 0 1 4 6.5"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  moon: '<path d="M21 12.8A9 9 0 1 1 11.2 3 7 7 0 0 0 21 12.8Z"/>',
  monitor: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
  command: '<path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3Z"/>',
  link: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
  'external': '<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>',
  logout: '<path d="M15 4h4a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-4M10 17l-5-5 5-5M5 12h11"/>',
  key: '<circle cx="8" cy="15" r="4"/><path d="m11 12 9-9m-3 3 3 3m-6 0 2 2"/>',
  home: '<path d="m3 11 9-8 9 8M5 9.5V20h5v-6h4v6h5V9.5"/>',
  compass: '<circle cx="12" cy="12" r="9"/><path d="m15.5 8.5-2 5-5 2 2-5Z"/>',
  bell: '<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10 21a2 2 0 0 0 4 0"/>',
  pulse: '<path d="M3 12h4l3-8 4 16 3-8h4"/>',
  chart: '<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>',
  'columns': '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/>',
  rows: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 12h18"/>',
  comment: '<path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z"/>',
  dots: '<circle cx="5" cy="12" r="1.3" fill="currentColor"/><circle cx="12" cy="12" r="1.3" fill="currentColor"/><circle cx="19" cy="12" r="1.3" fill="currentColor"/>',
  'arrow-left': '<path d="M19 12H5m6-6-6 6 6 6"/>',
  'arrow-right': '<path d="M5 12h14m-6-6 6 6-6 6"/>',
  sparkle: '<path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M6 18l2.5-2.5M15.5 8.5 18 6"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="m21 17-5-5-9 8"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>',
  flag: '<path d="M5 21V4M5 4h11l-2 4 2 4H5"/>',
  shield: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6Z"/>',
  zap: '<path d="M13 2 4 14h7l-1 8 9-12h-7Z"/>',
  filter: '<path d="M3 5h18l-7 8v6l-4 2v-8Z"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  label: '<path d="M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9Z"/><circle cx="7.5" cy="7.5" r="1.5"/>',
  keyboard: '<rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"/>',
  'git': '<circle cx="12" cy="9.5" r="3.6"/><path d="M15.6 5.9v8.4a4 4 0 0 1-4 4H10"/><circle cx="8.3" cy="18.3" r="1.8" fill="currentColor"/>',
};

export function icon(name, cls = '') {
  return raw(`<svg class="ico ${cls}" viewBox="0 0 24 24" aria-hidden="true">${P[name] || P.file}</svg>`);
}

let logoSeq = 0;
/**
 * The Gitter mark: a branch that bends into a spark where the commit lands.
 * Colors come from the active theme (--logo-bg, --brand-1..3, --logo-core).
 * `animated` draws the branch in and pops the nodes and spark (landing page).
 */
export function logo(size = 28, { animated = false, cls = '' } = {}) {
  const id = 'lg' + (++logoSeq);
  const d = animated ? ' class="draw" pathLength="1"' : '';
  const pop = (n) => (animated ? ` class="pop" style="animation-delay:${n}s"` : '');
  return raw(`<svg class="mark ${cls}" width="${size}" height="${size}" viewBox="0 0 64 64" aria-hidden="true">
<defs><linearGradient id="${id}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" style="stop-color:var(--brand-1)"/><stop offset=".55" style="stop-color:var(--brand-2)"/><stop offset="1" style="stop-color:var(--brand-3)"/></linearGradient></defs>
<rect width="64" height="64" rx="16" style="fill:var(--logo-bg)"/>
<path d="M18 46V22"${d} fill="none" stroke="url(#${id})" stroke-width="5" stroke-linecap="round"/>
<path d="M18 30c0 10 8 10 14 10h4"${d} fill="none" stroke="url(#${id})" stroke-width="5" stroke-linecap="round"${animated ? ' style="animation-delay:.35s"' : ''}/>
<circle cx="18" cy="18" r="5" style="fill:var(--brand-1)"${pop(0.15)}/>
<circle cx="18" cy="48" r="5" style="fill:var(--brand-2)"${pop(0.3)}/>
<g class="spark"><path d="M45 26l2.6 7.4L55 36l-7.4 2.6L45 46l-2.6-7.4L35 36l7.4-2.6z" fill="url(#${id})"${pop(0.7)}/>
<circle cx="45" cy="36" r="2.4" style="fill:var(--logo-core)"${pop(0.9)}/></g></svg>`);
}
