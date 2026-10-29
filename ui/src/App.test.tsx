import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppInfo } from "./ipc/bindings";

const info: AppInfo = {
  name: "OpenDeck",
  version: "0.0.0",
  tauri_version: "2.12.5",
  os: "linux",
  arch: "x86_64",
  max_decks: 4,
  visible_decks: 2,
};

// The generated bindings call Tauri's invoke; mock it at the IPC boundary.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) =>
    command === "app_info" ? Promise.resolve(info) : Promise.reject(new Error(command)),
  ),
}));

const { App } = await import("./App");

afterEach(cleanup);

describe("App", () => {
  it("renders the product name and backend info from app_info", async () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "OpenDeck" })).toBeTruthy();
    expect(await screen.findByText("v0.0.0 on linux/x86_64, 2 of 4 decks")).toBeTruthy();
  });
});
