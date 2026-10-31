import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Profiler } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DeckTelemetry } from "../ipc/bindings";
import type { createBindingsMock } from "../test/bindingsMock";
import { deckState } from "../test/bindingsMock";
import { RATE, runningAudio, telemetryFor, track } from "../test/fixtures";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("../state/app");
const { telemetryStore } = await import("../state/telemetry");
const { hydrateDeck, releaseAllHeld } = await import("../state/actions");
const { DeckPanel } = await import("./DeckPanel");

const { commands } = bindings;

/** Deck 2 loaded, playhead at 10 s, with the engine reporting `patch`. */
function loadDeckB(patch: Partial<DeckTelemetry> = {}, leader: number | null = null) {
  act(() => {
    useAppStore.getState().patchDeck(1, { track });
    telemetryStore
      .getState()
      .push(
        telemetryFor(
          1,
          1,
          { loaded: true, position: 10 * RATE, duration: 200 * RATE, ...patch },
          leader,
        ),
      );
  });
}

beforeEach(() => {
  useAppStore.getState().reset();
  useAppStore.getState().setAudio(runningAudio);
  telemetryStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(() => {
  releaseAllHeld();
  cleanup();
});

describe("DeckPanel for deck B", () => {
  it("is deck 2 with every control disabled until a track is loaded", () => {
    render(<DeckPanel deck={1} />);
    const panel = within(screen.getByRole("region", { name: "Deck 2" }));
    expect(panel.getByRole("button", { name: "PLAY" })).toHaveProperty("disabled", true);
    expect(panel.getByRole("button", { name: "Sync" })).toHaveProperty("disabled", true);
  });

  it("plays, cues and bends deck B, not deck A", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    fireEvent.click(screen.getByRole("button", { name: "PLAY" }));
    expect(commands.togglePlaying).toHaveBeenCalledWith(1);
    fireEvent.pointerDown(screen.getByRole("button", { name: "Cue" }), {
      pointerId: 1,
    });
    expect(commands.cue).toHaveBeenCalledWith(1, true);
    fireEvent.pointerDown(screen.getByRole("button", { name: "Pitch bend up" }), { pointerId: 1 });
    expect(commands.pitchBend).toHaveBeenCalledWith(1, 1);
  });

  it("shows the track BPM and key in the header", () => {
    render(<DeckPanel deck={1} />);
    expect(screen.getByText("No track loaded")).toBeTruthy();
    loadDeckB();
    expect(screen.getByTestId("track-bpm-1").textContent).toBe("126.00");
    expect(screen.getByTestId("key-1").textContent).toBe("8A");
  });

  it("steps the key shift and syncs the key to the other deck", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 key up" }));
    expect(commands.setKeyShift).toHaveBeenLastCalledWith(1, 1);
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 key down" }));
    expect(commands.setKeyShift).toHaveBeenLastCalledWith(1, 0);
    act(() => useAppStore.getState().patchDeck(0, { track: { ...track, key_musical: "Cm" } }));
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 key sync" }));
    // Am is relative to C, Cm to Eb: three semitones up.
    expect(commands.setKeyShift).toHaveBeenLastCalledWith(1, 3);
  });
});

describe("sync, master, quantize and slip", () => {
  it("turns sync on in the chosen mode and off again", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    const sync = screen.getByRole("button", { name: "Sync" });
    fireEvent.click(sync);
    expect(commands.setSync).toHaveBeenLastCalledWith(1, "beat");
    expect(useAppStore.getState().decks[1]?.sync).toBe("beat");
    // The light follows the engine's report, not the request.
    expect(sync.getAttribute("aria-pressed")).toBe("false");
    loadDeckB({ sync_mode: "beat" });
    expect(sync.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(sync, { shiftKey: true });
    expect(commands.setSync).toHaveBeenLastCalledWith(1, "tempo");
    fireEvent.click(sync);
    expect(commands.setSync).toHaveBeenLastCalledWith(1, "off");
    // The last mode is kept for the next press.
    fireEvent.click(sync);
    expect(commands.setSync).toHaveBeenLastCalledWith(1, "tempo");
  });

  it("requests and releases the sync leader, and prefers the engine's report", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    const master = screen.getByRole("button", { name: "Master" });
    fireEvent.click(master);
    expect(commands.setLeader).toHaveBeenLastCalledWith(1);
    expect(master.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(master);
    expect(commands.setLeader).toHaveBeenLastCalledWith(null);
    expect(master.getAttribute("aria-pressed")).toBe("false");
    // The engine names deck A: deck B shows it is not the leader.
    fireEvent.click(master);
    loadDeckB({}, 0);
    expect(master.getAttribute("aria-pressed")).toBe("false");
    loadDeckB({}, 1);
    act(() => telemetryStore.getState().push(telemetryFor(3, 1, { loaded: true }, 1)));
    expect(master.getAttribute("aria-pressed")).toBe("true");
  });

  it("toggles quantize (on by default) and slip", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    const quantize = screen.getByRole("button", { name: "Quantize" });
    expect(quantize.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(quantize);
    expect(commands.setQuantize).toHaveBeenCalledWith(1, false);
    fireEvent.click(screen.getByRole("button", { name: "Slip" }));
    expect(commands.setSlip).toHaveBeenCalledWith(1, true);
  });

  it("applies the backend deck state at startup", async () => {
    commands.getDeckState.mockResolvedValueOnce({
      status: "ok",
      data: {
        ...deckState(1),
        sync: "tempo",
        slip: true,
        quantize: false,
        is_leader: true,
      },
    } as never);
    render(<DeckPanel deck={1} />);
    loadDeckB({ sync_mode: "tempo" });
    await act(() => hydrateDeck(1));
    expect(screen.getByRole("button", { name: "Sync" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Slip" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Quantize" }).getAttribute("aria-pressed")).toBe(
      "false",
    );
    expect(screen.getByRole("button", { name: "Master" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
  });
});

describe("loops", () => {
  it("sets loop in, out and toggles reloop and exit from telemetry in manual mode", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    fireEvent.click(screen.getByRole("button", { name: "Manual loop" }));
    fireEvent.click(screen.getByRole("button", { name: "IN" }));
    fireEvent.click(screen.getByRole("button", { name: "OUT" }));
    expect(commands.loopIn).toHaveBeenCalledWith(1);
    expect(commands.loopOut).toHaveBeenCalledWith(1);
    fireEvent.click(screen.getByRole("button", { name: "RELOOP" }));
    expect(commands.loopToggle).toHaveBeenCalledWith(1);

    const loop = { start: 10 * RATE, end: 12 * RATE };
    act(() =>
      telemetryStore.getState().push(
        telemetryFor(5, 1, {
          loaded: true,
          loop_region: loop,
          loop_active: true,
        }),
      ),
    );
    expect(screen.getByRole("button", { name: "EXIT" })).toBeTruthy();
  });

  it("starts auto loops of 1/4 to 32 beats from the size display", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    fireEvent.click(screen.getByRole("button", { name: "Loop 4 beats" }));
    expect(commands.autoLoop).toHaveBeenLastCalledWith(1, 4);
    for (let i = 0; i < 4; i++) fireEvent.click(screen.getByRole("button", { name: "Halve loop" }));
    fireEvent.click(screen.getByRole("button", { name: "Loop 1/4 beats" }));
    expect(commands.autoLoop).toHaveBeenLastCalledWith(1, 0.25);
    for (let i = 0; i < 8; i++)
      fireEvent.click(screen.getByRole("button", { name: "Double loop" }));
    fireEvent.click(screen.getByRole("button", { name: "Loop 32 beats" }));
    expect(commands.autoLoop).toHaveBeenLastCalledWith(1, 32);
    expect(commands.resizeLoop).not.toHaveBeenCalled();
  });

  it("halves and doubles the active loop", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    const loop = { start: 10 * RATE, end: 12 * RATE };
    act(() =>
      telemetryStore.getState().push(
        telemetryFor(5, 1, {
          loaded: true,
          loop_region: loop,
          loop_active: true,
        }),
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "Halve loop" }));
    fireEvent.click(screen.getByRole("button", { name: "Double loop" }));
    expect(commands.resizeLoop.mock.calls).toEqual([
      [1, 0.5],
      [1, 2],
    ]);
  });

  it("rolls while held in ROLL mode and does not also start an auto loop", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    fireEvent.click(screen.getByRole("button", { name: "Loop roll" }));
    const four = screen.getByRole("button", { name: "Roll 4 beats" });
    fireEvent.pointerDown(four, { pointerId: 1 });
    fireEvent.pointerUp(four, { pointerId: 1 });
    fireEvent.click(four);
    expect(commands.loopRoll.mock.calls).toEqual([
      [1, 4, true],
      [1, 4, false],
    ]);
    expect(commands.autoLoop).not.toHaveBeenCalled();
  });

  it("rolls with Shift held and releases on window blur", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    const four = screen.getByRole("button", { name: "Loop 4 beats" });
    fireEvent.pointerDown(four, { pointerId: 1, shiftKey: true });
    expect(commands.loopRoll).toHaveBeenCalledWith(1, 4, true);
    releaseAllHeld();
    expect(commands.loopRoll).toHaveBeenLastCalledWith(1, 4, false);
    // A plain click afterwards is an auto loop again.
    fireEvent.click(four);
    fireEvent.click(four);
    expect(commands.autoLoop).toHaveBeenCalledWith(1, 4);
  });
});

describe("beat jump", () => {
  it("jumps the selected size forward and back", () => {
    render(<DeckPanel deck={1} />);
    loadDeckB();
    fireEvent.click(screen.getByRole("button", { name: "Beat jump forward" }));
    fireEvent.click(screen.getByRole("button", { name: "Beat jump back" }));
    expect(commands.beatJump.mock.calls).toEqual([
      [1, 4],
      [1, -4],
    ]);
    act(() => useAppStore.getState().patchDeck(1, { jumpBeats: 16 }));
    fireEvent.click(screen.getByRole("button", { name: "Beat jump back" }));
    expect(commands.beatJump).toHaveBeenLastCalledWith(1, -16);
    expect(screen.getByRole("combobox", { name: "Beat jump size" }).textContent).toContain("16");
  });
});

describe("rendering", () => {
  it("does not re-render the panel on a telemetry tick", () => {
    let commits = 0;
    render(
      <Profiler id="deck" onRender={() => commits++}>
        <DeckPanel deck={1} />
      </Profiler>,
    );
    loadDeckB();
    const before = commits;
    for (let sequence = 10; sequence < 40; sequence++) {
      act(() =>
        telemetryStore.getState().push(
          telemetryFor(sequence, 1, {
            loaded: true,
            position: sequence * RATE,
            duration: 200 * RATE,
            meter: { peak_l: 0.5, peak_r: 0.4, rms_l: 0.2, rms_r: 0.2 },
          }),
        ),
      );
    }
    expect(commits).toBe(before);
  });
});
