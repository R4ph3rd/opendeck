import type { Decorator, Preview } from "@storybook/react-vite";
import { useEffect } from "react";
import "../src/styles.css";

const WithTheme: Decorator = (Story, context) => {
  const theme = context.globals["theme"] === "light" ? "light" : "dark";
  useEffect(() => {
    document.documentElement.dataset["theme"] = theme;
  }, [theme]);
  return (
    <div data-theme={theme} className="min-h-24 bg-bg p-4 text-fg">
      <Story />
    </div>
  );
};

const preview: Preview = {
  decorators: [WithTheme],
  globalTypes: {
    theme: {
      description: "Theme",
      toolbar: {
        title: "Theme",
        icon: "circlehollow",
        items: [
          { value: "dark", title: "Dark" },
          { value: "light", title: "Light" },
        ],
        dynamicTitle: true,
      },
    },
  },
  initialGlobals: { theme: "dark" },
  parameters: {
    layout: "fullscreen",
    controls: { expanded: true },
    docs: { toc: true },
  },
  tags: ["autodocs"],
};

export default preview;
