export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "type-enum": [2, "always", ["feat", "fix", "refactor", "perf", "test", "docs", "build", "ci", "chore", "style"]],
    "scope-enum": [
      2,
      "always",
      ["engine", "fx", "analysis", "library", "midi", "media", "contracts", "app-core", "ui", "design-system", "tauri", "ci", "adr", "docs", "repo", "release", "deps"],
    ],
    "body-max-line-length": [0],
  },
};
