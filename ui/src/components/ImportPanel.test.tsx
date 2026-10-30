import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Toaster, toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JobProgress } from "../ipc/bindings";
import type { createBindingsMock } from "../test/bindingsMock";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useLibraryStore } = await import("../state/library");
const { onImportProgress } = await import("../state/libraryActions");
const { ImportPanel, jobCounts, jobFraction } = await import("./ImportPanel");

const { commands } = bindings;

const job = (patch: Partial<JobProgress> = {}): JobProgress => ({
  job: 1,
  kind: "import_folder",
  state: "running",
  stage: "analyze",
  items_total: 40,
  items_done: 12,
  items_duplicate: 3,
  items_failed: 1,
  current_item: "/music/crate/Night Drive.flac",
  last_track: null,
  ...patch,
});

beforeEach(() => {
  localStorage.clear();
  useLibraryStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(() => {
  toast.dismiss();
  cleanup();
});

describe("job summaries", () => {
  it("counts finished items for the progress bar", () => {
    const entry = { ...job(), source: null, error: null };
    expect(jobFraction(entry)).toBe(16 / 40);
    expect(jobFraction({ ...entry, items_total: 0 })).toBe(0);
    expect(jobFraction({ ...entry, items_total: 0, state: "completed" })).toBe(1);
    expect(jobFraction({ ...entry, items_done: 99 })).toBe(1);
  });

  it("writes the counts on one line", () => {
    const entry = { ...job(), source: null, error: null };
    expect(jobCounts(entry)).toBe("12 of 40 added, 3 duplicates, 1 failed");
    expect(jobCounts({ ...entry, items_duplicate: 1, items_failed: 0 })).toBe(
      "12 of 40 added, 1 duplicate",
    );
    expect(jobCounts({ ...entry, items_duplicate: 0, items_failed: 0 })).toBe("12 of 40 added");
  });
});

describe("ImportPanel", () => {
  it("renders nothing without jobs", () => {
    const { container } = render(<ImportPanel />);
    expect(container.firstChild).toBeNull();
  });

  it("shows the stage, counts, current file and a progress bar for a running job", () => {
    render(<ImportPanel />);
    act(() => onImportProgress(job()));
    const region = screen.getByRole("region", { name: "Imports" });
    expect(within(region).getByText("Import folder")).toBeTruthy();
    expect(within(region).getByText("Analysing")).toBeTruthy();
    expect(within(region).getByText("12 of 40 added, 3 duplicates, 1 failed")).toBeTruthy();
    // The path shows as the file name only.
    expect(within(region).getByText("Night Drive.flac")).toBeTruthy();
    const bar = within(region).getByRole("progressbar", { name: "Import folder progress" });
    expect(bar.getAttribute("aria-valuenow")).toBe("40");
  });

  it("follows later progress events of the same job and keeps the last file name", () => {
    render(<ImportPanel />);
    act(() => onImportProgress(job()));
    act(() =>
      onImportProgress(job({ items_done: 30, stage: "persist", current_item: null, last_track: 4 })),
    );
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    expect(screen.getByText("Saving")).toBeTruthy();
    expect(screen.getByText("Night Drive.flac")).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("85");
  });

  it("pauses, resumes and cancels a job", async () => {
    render(<ImportPanel />);
    act(() => onImportProgress(job()));
    fireEvent.click(screen.getByRole("button", { name: "Pause Import folder" }));
    await waitFor(() => expect(commands.pauseJob).toHaveBeenCalledWith(1));
    act(() => onImportProgress(job({ state: "paused", stage: null })));
    expect(screen.getByText("Paused")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Resume Import folder" }));
    await waitFor(() => expect(commands.resumeJob).toHaveBeenCalledWith(1));
    fireEvent.click(screen.getByRole("button", { name: "Cancel Import folder" }));
    await waitFor(() => expect(commands.cancelJob).toHaveBeenCalledWith(1));
  });

  it("shows one row per job and each kind by name", () => {
    render(<ImportPanel />);
    act(() => {
      onImportProgress(job({ job: 1 }));
      onImportProgress(job({ job: 2, kind: "reanalyze", stage: "decode" }));
      onImportProgress(job({ job: 3, kind: "import_rekordbox_xml", stage: "discover" }));
    });
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getByText("Analyse tracks")).toBeTruthy();
    expect(screen.getByText("Import rekordbox XML")).toBeTruthy();
    expect(screen.getByText("Decoding")).toBeTruthy();
    expect(screen.getByText("Finding files")).toBeTruthy();
  });

  it("dismisses finished jobs one by one or all at once, keeping running ones", () => {
    render(<ImportPanel />);
    act(() => {
      onImportProgress(job({ job: 1, state: "completed", stage: null }));
      onImportProgress(job({ job: 2, state: "cancelled", stage: null }));
      onImportProgress(job({ job: 3 }));
    });
    expect(screen.queryByRole("button", { name: "Pause Import folder" })).toBeTruthy();
    const [firstDismiss] = screen.getAllByRole("button", { name: "Dismiss Import folder" });
    if (!firstDismiss) throw new Error("no dismiss button");
    fireEvent.click(firstDismiss);
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Clear finished" }));
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Pause Import folder" })).toBeTruthy();
  });

  it("announces a finished import once", async () => {
    render(
      <Toaster>
        <ImportPanel />
      </Toaster>,
    );
    act(() => onImportProgress(job()));
    act(() => onImportProgress(job({ state: "completed", stage: null, items_done: 36 })));
    expect(
      await screen.findByText("Import finished: 36 added, 3 already in the collection, 1 failed."),
    ).toBeTruthy();
    // A repeated terminal event does not announce again.
    act(() => onImportProgress(job({ state: "completed", stage: null, items_done: 36 })));
    expect(screen.getAllByText(/Import finished/)).toHaveLength(1);
  });

  it("reports a failed job", async () => {
    render(
      <Toaster>
        <ImportPanel />
      </Toaster>,
    );
    act(() => onImportProgress(job({ state: "failed", stage: null })));
    expect(await screen.findByText("The import failed.")).toBeTruthy();
  });
});

describe("job hydration", () => {
  it("shows jobs that were running at startup and hides old finished ones", () => {
    const stored = (id: number, state: JobProgress["state"]) => ({
      progress: job({ job: id, state, stage: null }),
      source: `/music/${id}`,
      error: null,
      created_at: 0,
    });
    act(() =>
      useLibraryStore.getState().hydrateJobs([
        stored(1, "running"),
        stored(2, "paused"),
        stored(3, "completed"),
        stored(4, "failed"),
      ]),
    );
    expect(useLibraryStore.getState().jobs.map((j) => j.job)).toEqual([1, 2]);
    expect(useLibraryStore.getState().jobs[0]?.source).toBe("/music/1");
  });
});
