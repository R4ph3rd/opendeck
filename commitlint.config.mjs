export default {
  extends: ["@commitlint/config-conventional"],
  // Agent branch-sync merges ("merge: origin/workbench into feat/x") are not changes of their own.
  ignores: [(message) => /^merge: \S+ into \S+/.test(message)],
  rules: {
    "type-enum": [2, "always", ["feat", "fix", "refactor", "perf", "test", "docs", "build", "ci", "chore", "style"]],
    "scope-enum": [
      2,
      "always",
      ["engine", "fx", "analysis", "library", "midi", "media", "plugin-host", "contracts", "app-core", "ui", "design", "design-system", "tauri", "ci", "adr", "docs", "repo", "release", "deps"],
    ],
    "body-max-line-length": [0],
  },
};
