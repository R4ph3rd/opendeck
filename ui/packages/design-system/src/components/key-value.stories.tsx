import type { Meta, StoryObj } from "@storybook/react-vite";
import { KeyValueList } from "./key-value";

const meta = {
  title: "Components/KeyValueList",
  component: KeyValueList,
  decorators: [(Story) => <div className="w-96"><Story /></div>],
  args: {
    items: [
      { label: "Sample rate", value: "44100 Hz" },
      { label: "Bit depth", value: "24 bit" },
      { label: "Format", value: "FLAC" },
      { label: "Path", value: "/home/dj/Music/House/strobe.flac" },
    ],
  },
  parameters: {
    docs: {
      description: {
        component:
          "Semantic `<dl>` for track info and settings summaries. Labels are muted and truncate; values wrap. `mono` switches values to the mono face for paths and hashes.",
      },
    },
  },
} satisfies Meta<typeof KeyValueList>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
export const Mono: Story = { args: { mono: true } };
