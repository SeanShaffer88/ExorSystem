// Fails if any theme drops below WCAG AA. Run: npm run check:themes
import { readFileSync } from "node:fs";

const themes = JSON.parse(readFileSync(new URL("../src/themes.json", import.meta.url), "utf8"));
const rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const lin = (v) => ((v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const lum = (h) => { const [r, g, b] = rgb(h).map(lin); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };

// [foreground token, background token, minimum ratio, why]
const rules = [
  ["fg", "bg", 4.5, "body text"], ["fg", "panel", 4.5, "text on panels"],
  ["muted", "bg", 4.5, "secondary text"], ["muted", "panel", 4.5, "secondary text on panels"],
  ["danger", "bg", 4.5, "error text"], ["danger", "panel", 4.5, "error text on panels"],
  ["accent", "bg", 3, "focus ring and selection"], ["accent", "panel", 3, "focus ring on panels"],
  ["border", "bg", 3, "control boundaries"], ["border", "panel", 3, "control boundaries on panels"],
];
const keys = ["bg", "panel", "fg", "muted", "border", "accent", "danger"];
let bad = 0;
const ids = new Set();
for (const t of themes) {
  if (ids.has(t.id)) { console.error(`duplicate id ${t.id}`); bad++; }
  ids.add(t.id);
  if (!["dark", "light"].includes(t.mode)) { console.error(`${t.id}: bad mode`); bad++; }
  for (const k of keys) if (!/^#[0-9a-f]{6}$/i.test(t.tokens[k] ?? "")) { console.error(`${t.id}: bad ${k}`); bad++; }
  for (const [f, b, min, why] of rules) {
    const r = ratio(t.tokens[f], t.tokens[b]);
    if (r < min) { console.error(`${t.id}: ${f} on ${b} is ${r.toFixed(2)} < ${min} (${why})`); bad++; }
  }
}
if (bad) { console.error(`${bad} problem(s)`); process.exit(1); }
console.log(`${themes.length} themes pass WCAG AA checks`);
