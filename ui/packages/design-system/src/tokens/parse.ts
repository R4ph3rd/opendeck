/**
 * Minimal parser for tokens.css, shared by the contrast tests and the Tokens
 * story so neither can drift from the stylesheet.
 */

export type Theme = "dark" | "light";
export type TokenMap = Record<string, string>;

interface Rule {
  selectors: string[];
  decls: [string, string][];
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Top-level style rules only. At-rules (@media, @keyframes) are skipped. */
function topLevelRules(css: string): Rule[] {
  const src = stripComments(css);
  const rules: Rule[] = [];
  let depth = 0;
  let start = 0;
  let head = "";
  for (let i = 0; i < src.length; i++) {
    const ch = src[i];
    if (ch === "{") {
      if (depth === 0) {
        head = src.slice(start, i).trim();
        start = i + 1;
      }
      depth++;
    } else if (ch === "}") {
      depth--;
      if (depth === 0) {
        const body = src.slice(start, i);
        start = i + 1;
        if (!head.startsWith("@")) {
          rules.push({
            selectors: head.split(",").map((s) => s.trim()),
            decls: parseDecls(body),
          });
        }
      }
    }
  }
  return rules;
}

function parseDecls(body: string): [string, string][] {
  const out: [string, string][] = [];
  let depth = 0;
  let cur = "";
  for (const ch of body) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (ch === ";" && depth === 0) {
      push(cur, out);
      cur = "";
    } else {
      cur += ch;
    }
  }
  push(cur, out);
  return out;
}

function push(decl: string, out: [string, string][]): void {
  const idx = decl.indexOf(":");
  if (idx < 0) return;
  const name = decl.slice(0, idx).trim();
  const value = decl
    .slice(idx + 1)
    .replace(/\s+/g, " ")
    .trim();
  if (name.startsWith("--")) out.push([name.slice(2), value]);
}

const SELECTORS: Record<Theme, string[]> = {
  dark: [":root", '[data-theme="dark"]'],
  light: [":root", '[data-theme="light"]'],
};

/** Raw (unresolved) custom properties as the cascade leaves them for a theme. */
export function rawTokens(css: string, theme: Theme): TokenMap {
  const map: TokenMap = {};
  for (const rule of topLevelRules(css)) {
    if (rule.selectors.some((s) => SELECTORS[theme].includes(s))) {
      for (const [name, value] of rule.decls) map[name] = value;
    }
  }
  return map;
}

/** Custom properties with every var(--x) reference substituted. */
export function resolvedTokens(css: string, theme: Theme): TokenMap {
  const raw = rawTokens(css, theme);
  const resolve = (value: string, seen: string[]): string =>
    value.replace(/var\(--([\w-]+)(?:,[^)]*)?\)/g, (_m, name: string) => {
      const next = raw[name];
      if (next === undefined || seen.includes(name)) {
        throw new Error(`unresolved var(--${name})`);
      }
      return resolve(next, [...seen, name]);
    });
  const out: TokenMap = {};
  for (const [name, value] of Object.entries(raw)) out[name] = resolve(value, [name]);
  return out;
}

export interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

export function parseColor(value: string): Rgba {
  const v = value.trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v);
  if (hex?.[1]) {
    let h = hex[1];
    if (h.length === 3) {
      h = [...h].map((c) => c + c).join("");
    }
    return {
      r: parseInt(h.slice(0, 2), 16),
      g: parseInt(h.slice(2, 4), 16),
      b: parseInt(h.slice(4, 6), 16),
      a: 1,
    };
  }
  const fn = /^rgba?\(\s*(\d+)\s+(\d+)\s+(\d+)\s*(?:\/\s*([\d.]+))?\s*\)$/.exec(v);
  if (fn) {
    return {
      r: Number(fn[1]),
      g: Number(fn[2]),
      b: Number(fn[3]),
      a: fn[4] === undefined ? 1 : Number(fn[4]),
    };
  }
  throw new Error(`unsupported color: ${value}`);
}

/** Composite `top` over an opaque `bottom`. */
export function over(top: Rgba, bottom: Rgba): Rgba {
  const mix = (t: number, b: number): number => t * top.a + b * (1 - top.a);
  return { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a: 1 };
}

function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function luminance(c: Rgba): number {
  return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
}

/** WCAG 2.x contrast ratio. Translucent foregrounds are composited over `bg`. */
export function contrast(fg: Rgba, bg: Rgba): number {
  const f = luminance(over(fg, bg));
  const b = luminance(bg);
  const [hi, lo] = f > b ? [f, b] : [b, f];
  return (hi + 0.05) / (lo + 0.05);
}
