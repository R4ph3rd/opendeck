// Raw colors, spacing, radii, font sizes, shadows and z-index live in
// packages/design-system/src/tokens/tokens.css only. Everywhere else CSS must
// reference tokens through var(--token).
const TOKEN_ONLY = ["/^var\\(--/", "/^(calc|min|max|clamp)\\(.*var\\(--/"];
const KEYWORDS = ["transparent", "inherit", "initial", "unset", "currentColor", "none", "auto", "0"];

export default {
  plugins: ["stylelint-declaration-strict-value"],
  rules: {
    "scale-unlimited/declaration-strict-value": [
      [
        "/color$/",
        "fill",
        "stroke",
        "/^margin/",
        "/^padding/",
        "gap",
        "row-gap",
        "column-gap",
        "border-radius",
        "font-size",
        "box-shadow",
        "z-index",
      ],
      {
        // Token references only; allow neutral keywords. Color-like keywords other
        // than transparent/currentColor are rejected because they are not tokens.
        ignoreValues: [...TOKEN_ONLY, ...KEYWORDS],
        ignoreFunctions: false,
        disableFix: true,
        message: "Use a design token: var(--token). Raw values belong in tokens.css.",
      },
    ],
  },
  ignoreFiles: [
    "dist/**",
    "storybook-static/**",
    "node_modules/**",
    "packages/*/storybook-static/**",
    "packages/design-system/src/tokens/tokens.css",
  ],
};
