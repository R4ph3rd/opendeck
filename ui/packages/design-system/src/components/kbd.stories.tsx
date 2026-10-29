import type { Meta, StoryObj } from "@storybook/react-vite";
import { Kbd } from "./kbd";

const meta = { title: "Components/Kbd", component: Kbd, args: { children: "Space" } } satisfies Meta<typeof Kbd>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
