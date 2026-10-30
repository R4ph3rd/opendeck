import type { StorybookConfig } from "@storybook/react-vite";

/**
 * Storybook for the app-level visual components in `src/viz`. The design
 * system has its own Storybook in `packages/design-system`; both load the same
 * token stylesheet, so themes match.
 */
const config: StorybookConfig = {
  stories: ["../src/viz/**/*.stories.@(ts|tsx)"],
  addons: ["@storybook/addon-docs"],
  framework: "@storybook/react-vite",
  core: { disableTelemetry: true },
  async viteFinal(viteConfig) {
    const { default: tailwindcss } = await import("@tailwindcss/vite");
    return { ...viteConfig, plugins: [...(viteConfig.plugins ?? []), tailwindcss()] };
  },
};

export default config;
