import type { Meta, StoryObj } from "@storybook/react-vite";
import { ProgressBar } from "./progress-bar";

const meta = {
  title: "Components/ProgressBar",
  component: ProgressBar,
  args: { "aria-label": "Importing library", value: 45 },
  decorators: [(Story) => <div className="w-72"><Story /></div>],
  parameters: {
    docs: {
      description: {
        component:
          "Determinate when `value` is set (0 to `max`, default 100); indeterminate when `value` is `null` or omitted (unknown duration, for example while a folder is being scanned). Always give it an `aria-label`. Under reduced motion the indeterminate sweep becomes a slow pulse. Pair with text (\"120 of 800 files\") when you know the count.",
      },
    },
  },
} satisfies Meta<typeof ProgressBar>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Determinate: Story = {};
export const Indeterminate: Story = { args: { value: null } };
export const Complete: Story = { args: { value: 100 } };
