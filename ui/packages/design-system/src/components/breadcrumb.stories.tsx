import type { Meta, StoryObj } from "@storybook/react-vite";
import { Fragment } from "react";
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "./breadcrumb";

const meta: Meta<typeof Breadcrumb> = {
  title: "Components/Breadcrumb",
  component: Breadcrumb,
  parameters: {
    docs: {
      description: {
        component:
          "Path trail for the file explorer. Parents are `BreadcrumbLink` (use `asChild` to wrap a button or router link); the last item is `BreadcrumbPage`, marked `aria-current=\"page\"`. Separators are decorative. Truncate long trails in the app by collapsing middle segments.",
      },
    },
  },
};
export default meta;
type Story = StoryObj<typeof meta>;

const path = ["Music", "Crates", "House", "2026"];

export const FilePath: Story = {
  render: () => (
    <Breadcrumb>
      <BreadcrumbList>
        {path.map((seg, i) => (
          <Fragment key={seg}>
            {i > 0 ? <BreadcrumbSeparator /> : null}
            <BreadcrumbItem>
              {i === path.length - 1 ? (
                <BreadcrumbPage>{seg}</BreadcrumbPage>
              ) : (
                <BreadcrumbLink asChild>
                  <button type="button">{seg}</button>
                </BreadcrumbLink>
              )}
            </BreadcrumbItem>
          </Fragment>
        ))}
      </BreadcrumbList>
    </Breadcrumb>
  ),
};
