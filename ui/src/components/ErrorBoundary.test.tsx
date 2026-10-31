import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { ErrorBoundary } = await import("./ErrorBoundary");

function Bomb({ explode }: { explode: boolean }) {
  if (explode) {
    throw new Error("render failed");
  }
  return <p>fine</p>;
}

const writeText = vi.fn(() => Promise.resolve());

beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  Object.defineProperty(globalThis.navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
  writeText.mockClear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ErrorBoundary", () => {
  it("renders children when nothing throws", () => {
    render(
      <ErrorBoundary>
        <Bomb explode={false} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("fine")).toBeTruthy();
  });

  it("shows the recovery screen with the error", () => {
    render(
      <ErrorBoundary>
        <Bomb explode />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    expect(screen.getByText("render failed")).toBeTruthy();
    expect(screen.queryByText("fine")).toBeNull();
  });

  it("reloads the UI on request", () => {
    const onReload = vi.fn();
    render(
      <ErrorBoundary onReload={onReload}>
        <Bomb explode />
      </ErrorBoundary>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Reload UI" }));
    expect(onReload).toHaveBeenCalledOnce();
  });

  it("copies backend diagnostics and the UI error", async () => {
    render(
      <ErrorBoundary>
        <Bomb explode />
      </ErrorBoundary>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Copy diagnostics" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    expect(bindings.commands.getDiagnostics).toHaveBeenCalled();
    const text = (writeText.mock.calls[0] as unknown as [string])[0];
    expect(text).toContain("OpenDeck 0.7.0");
    expect(text).toContain("UI error: render failed");
    expect(await screen.findByRole("button", { name: "Copied" })).toBeTruthy();
  });

  it("still copies the UI error when the backend report fails", async () => {
    bindings.commands.getDiagnostics.mockRejectedValueOnce(new Error("no backend"));
    render(
      <ErrorBoundary>
        <Bomb explode />
      </ErrorBoundary>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Copy diagnostics" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    const text = (writeText.mock.calls[0] as unknown as [string])[0];
    expect(text).toContain("Backend report unavailable: no backend");
    expect(text).toContain("UI error: render failed");
  });
});
