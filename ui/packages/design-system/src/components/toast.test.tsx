import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { toast, Toaster } from "./toast";

afterEach(() => {
  act(() => toast.dismiss());
});

describe("toast", () => {
  it("renders a toast fired imperatively with variant and action", () => {
    const onClick = vi.fn();
    render(<Toaster />);
    act(() => {
      toast({ title: "Saved", description: "Set list stored", variant: "success", action: { label: "Undo", onClick } });
    });
    expect(screen.getByText("Saved")).toBeTruthy();
    expect(screen.getByText("Set list stored")).toBeTruthy();
    const li = screen.getByText("Saved").closest("[data-variant]");
    expect(li?.getAttribute("data-variant")).toBe("success");
    act(() => screen.getByText("Undo").click());
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("defaults to info and can be dismissed by id", () => {
    render(<Toaster />);
    let id = "";
    act(() => {
      id = toast({ title: "Hello" });
    });
    expect(screen.getByText("Hello").closest("[data-variant]")?.getAttribute("data-variant")).toBe("info");
    act(() => toast.dismiss(id));
    expect(screen.queryByText("Hello")).toBeNull();
  });
});
