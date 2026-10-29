// @vitest-environment node
// Proof that the token lint rules reject raw hex, color functions and magic sizes.
import { Linter } from "eslint";
import { fileURLToPath } from "node:url";
import stylelint from "stylelint";
import { describe, expect, it } from "vitest";
import plugin from "../../eslint/no-raw-style-literals.js";

const uiRoot = fileURLToPath(new URL("../../../../", import.meta.url));

function eslintMessages(code: string): string[] {
  const linter = new Linter();
  const results = linter.verify(code, [
    {
      files: ["**/*.js"],
      languageOptions: { ecmaVersion: 2022, sourceType: "module" },
      plugins: { t: plugin },
      rules: { "t/no-raw-style-literals": "error" },
    },
  ]);
  return results.map((m) => m.messageId ?? m.message);
}

describe("eslint no-raw-style-literals", () => {
  it("rejects hex colors", () => {
    expect(eslintMessages('const c = "#ff4f7b";')).toEqual(["hex"]);
    expect(eslintMessages('const c = "#fff";')).toEqual(["hex"]);
    expect(eslintMessages("const c = `border-[#abcdef]`;")).toEqual(["hex"]);
  });
  it("rejects rgb/hsl/oklch functions", () => {
    expect(eslintMessages('const c = "rgb(0 0 0 / 0.5)";')).toEqual(["colorFn"]);
    expect(eslintMessages('const c = "hsl(10, 20%, 30%)";')).toEqual(["colorFn"]);
  });
  it("rejects magic pixel sizes in class strings", () => {
    expect(eslintMessages('const c = "h-[28px] flex";')).toEqual(["arbitraryLength"]);
  });
  it("accepts token references and scale classes", () => {
    expect(eslintMessages('const c = "bg-surface h-7 p-[var(--space-2)]";')).toEqual([]);
    expect(eslintMessages('const c = "var(--accent)";')).toEqual([]);
    expect(eslintMessages('const href = "#main-content";')).toEqual([]);
  });
});

async function stylelintWarnings(code: string, codeFilename: string): Promise<string[]> {
  const result = await stylelint.lint({ code, codeFilename, cwd: uiRoot });
  return result.results.flatMap((r) => r.warnings.map((w) => w.text));
}

describe("stylelint declaration-strict-value", () => {
  const file = `${uiRoot}packages/design-system/src/example.css`;
  it("rejects raw hex colors", async () => {
    expect(await stylelintWarnings(".a { color: #ff0000; }", file)).not.toEqual([]);
  });
  it("rejects raw sizes, radii, shadows, font sizes and z-index", async () => {
    for (const decl of [
      "padding: 12px",
      "border-radius: 6px",
      "font-size: 13px",
      "box-shadow: 0 1px 2px black",
      "z-index: 99",
      "background-color: rgb(0 0 0)",
    ]) {
      expect(await stylelintWarnings(`.a { ${decl}; }`, file), decl).not.toEqual([]);
    }
  });
  it("accepts token references", async () => {
    const css = ".a { color: var(--fg); padding: var(--space-2); z-index: var(--z-modal); background: transparent; }";
    expect(await stylelintWarnings(css, file)).toEqual([]);
  });
  it("exempts tokens.css", async () => {
    const tokens = `${uiRoot}packages/design-system/src/tokens/tokens.css`;
    expect(await stylelintWarnings(":root { --x: 1px; } .a { color: #ff0000; }", tokens)).toEqual([]);
  });
});
