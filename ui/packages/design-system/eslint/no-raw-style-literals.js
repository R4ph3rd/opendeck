// ESLint rule: no raw colors or magic pixel sizes in TS/TSX string literals.
// Colors and sizes live in tokens.css. Use token classes (bg-surface, h-7) or
// var(--token) references instead. Tokens files are exempt via eslint config.

const HEX = /(?:^|[^\w&])#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{4}|[0-9a-f]{3})(?![\w-])/i;
const COLOR_FN = /\b(?:rgba?|hsla?|hwb|oklch|oklab)\(/i;
// Tailwind arbitrary lengths such as h-[28px] or p-[0.5rem].
const ARBITRARY_LENGTH = /\[-?\d*\.?\d+(?:px|rem|em)\]/;

/** @param {string} text */
// Glass is banned: Tailwind blur/backdrop utilities, backdrop-filter, filter: blur().
const GLASS = /(?:^|[\s:"'`])(?:backdrop-|blur-)|backdrop-filter|\bblur\(/i;

function check(text) {
  if (GLASS.test(text)) return "glass";
  if (HEX.test(text)) return "hex";
  if (COLOR_FN.test(text)) return "colorFn";
  if (ARBITRARY_LENGTH.test(text)) return "arbitraryLength";
  return null;
}

/** @type {import("eslint").Rule.RuleModule} */
export const noRawStyleLiterals = {
  meta: {
    type: "problem",
    schema: [],
    messages: {
      glass: "Glassy effect in {{ value }}. Backdrop filters and blur are not allowed; use a flat token surface.",
      hex: "Raw hex color {{ value }}. Use a design token (var(--token) or a token class).",
      colorFn: "Raw color function in {{ value }}. Use a design token (var(--token) or a token class).",
      arbitraryLength:
        "Magic pixel size in {{ value }}. Use a spacing/size token (var(--token)) or a scale class.",
    },
  },
  create(context) {
    /** @param {import("estree").Node} node @param {string} text */
    const report = (node, text) => {
      const id = check(text);
      if (id) {
        const value = JSON.stringify(text.length > 40 ? `${text.slice(0, 37)}...` : text);
        context.report({ node, messageId: id, data: { value } });
      }
    };
    return {
      Property(node) {
        const key = node.key.type === "Identifier" ? node.key.name : node.key.value;
        if (key === "backdropFilter" || key === "WebkitBackdropFilter") {
          context.report({ node, messageId: "glass", data: { value: String(key) } });
        }
      },
      Literal(node) {
        if (typeof node.value === "string") report(node, node.value);
      },
      TemplateElement(node) {
        report(node, node.value.cooked ?? node.value.raw);
      },
    };
  },
};

export default { rules: { "no-raw-style-literals": noRawStyleLiterals } };
