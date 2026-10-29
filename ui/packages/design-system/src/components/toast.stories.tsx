import type { Meta, StoryObj } from "@storybook/react-vite";
import { Button } from "./button";
import { toast, Toaster, type ToastVariant } from "./toast";

const meta = {
  title: "Components/Toast",
  component: Toaster,
  decorators: [
    (Story) => (
      <>
        <Story />
      </>
    ),
  ],
} satisfies Meta<typeof Toaster>;
export default meta;
type Story = StoryObj<typeof meta>;

const variants: ToastVariant[] = ["info", "success", "warning", "danger"];

export const Variants: Story = {
  render: () => (
    <>
      <Toaster />
      <div className="flex gap-2">
        {variants.map((variant) => (
          <Button
            key={variant}
            variant="secondary"
            onClick={() => toast({ title: `Toast: ${variant}`, description: "Something happened.", variant })}
          >
            {variant}
          </Button>
        ))}
      </div>
    </>
  ),
};

export const WithAction: Story = {
  render: () => (
    <>
      <Toaster />
      <Button
        onClick={() =>
          toast({
            title: "Track removed",
            variant: "success",
            action: { label: "Undo", onClick: () => toast({ title: "Restored" }) },
          })
        }
      >
        Remove track
      </Button>
    </>
  ),
};
