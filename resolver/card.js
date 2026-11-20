// The card an LNS name's NFT shows: the wallet or any SDK app draws the
// holder's own name locally (design/lns-name-card/README.md, approved 2026-11-19).
// This is pure SVG: no font files, no network reads, no state. It runs in Node,
// a browser or a Worker. Fonts are named only, as the design requires.

const esc = (text) => text
  .replace(/&/g, "&amp;")
  .replace(/</g, "&lt;")
  .replace(/>/g, "&gt;")
  .replace(/"/g, "&quot;")
  .replace(/'/g, "&apos;");

// 12 characters or fewer at 60px; 13-32 (the LNS maximum) at 36px, wrapped to
// at most two lines. Names wrap by character, like the design's break-all.
function lines(name) {
  if (name.length <= 12) return [name];
  const perLine = 19;
  const out = [];
  for (let i = 0; i < name.length; i += perLine) out.push(name.slice(i, i + perLine));
  return out.slice(0, 2);
}

// The Names Wallet mark from the approved card: a white rounded square with
// the eta cut out. The path is the same one card.html uses, scaled to 18x18.
const LOGO = `<g transform="translate(288,420) scale(0.0703125)"><defs><mask id="lns-card-eta"><rect width="256" height="256" rx="58" fill="#fff"/><path fill="#000" transform="translate(0,10)" d="M107.68 89.94V152.55H78.65V44.35H106.03L106.42 66.71Q110.97 55.39 119.29 49.19Q127.61 43.00 140.10 43.00Q156.94 43.00 167.15 53.84Q177.35 64.68 177.35 83.74V193.00H148.23V88.77Q148.23 78.71 143.05 73.00Q137.87 67.29 128.68 67.29Q119.39 67.29 113.53 73.24Q107.68 79.19 107.68 89.94Z"/></mask></defs><rect width="256" height="256" rx="58" fill="#f5f5ef" mask="url(#lns-card-eta)"/></g>`;

/** The 480x480 card for `name` as an SVG string. */
export function nameCard(name) {
  const short = name.length <= 12;
  const size = short ? 60 : 36;
  const first = short ? 190 : 172;
  const gap = short ? 63 : 41.4;
  const text = lines(name).map((line, i) =>
    `<tspan x="40" y="${i === 0 ? first : first + gap * i}">${esc(line)}</tspan>`
  ).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 480" width="480" height="480" role="img" aria-label="LNS name card for ${esc(name)}">
<rect x="0" y="0" width="480" height="480" rx="28" fill="#152521" stroke="#2c4039" stroke-width="1"/>
<text x="440" y="66" text-anchor="end" font-family="Gloock" font-size="30" fill="#ffd328">λ</text>
<text font-family="Gloock" font-size="${size}" fill="#f5f5ef">${text}</text>
<rect x="40" y="382" width="400" height="2" fill="#ffd328"/>
${LOGO}
<text x="440" y="436" text-anchor="end" font-family="Fira Code" font-size="14" fill="#f5f5ef" fill-opacity="0.7">nameswallet.app</text>
</svg>`;
}

/** The same card as a `data:image/svg+xml` URL, for an `<img src>` with no file. */
export function nameCardDataUrl(name) {
  return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(nameCard(name));
}
