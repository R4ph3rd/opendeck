import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../../test/bindingsMock";
import { neutralPadState, neutralSampler } from "../../test/bindingsMock";
import { RATE, runningAudio, telemetryFor, track } from "../../test/fixtures";

vi.mock("../../ipc/bindings", async () =>
  (await import("../../test/bindingsMock")).createBindingsMock(),
);

const bindings = (await import("../../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("../../state/app");
const { usePadStore } = await import("../../state/pads");
const { useFxStore } = await import("../../state/fx");
const { telemetryStore } = await import("../../state/telemetry");
const { DRAG_MIME } = await import("../../lib/dnd");
const { PadMiddlePanel, PadModeView } = await import("./PadMiddlePanel");
const { fxEffects } = await import("../../test/bindingsMock");
const { commands } = bindings;

const cue = (name: string, start: number) =>
  ({ id: 1, kind: { kind: "hot", index: 0 }, start, end: null, color: null, name }) as never;

function loadDeck(deck = 0, patch: Parameters<typeof telemetryFor>[2] = {}) {
  act(() => {
    useAppStore.getState().patchDeck(deck, { track });
    telemetryStore.getState().push(
      telemetryFor(1, deck, { loaded: true, duration: 200 * RATE, ...patch }),
    );
  });
}

const pad = (name: string) => screen.getByRole("button", { name });
/** Pad state with `patch` applied to deck 1. */
function padStateWith(patch: Record<string, unknown>) {
  const state = neutralPadState();
  state.decks[0] = { ...state.decks[0], ...patch } as never;
  return state as never;
}

const press = (el: HTMLElement) => {
  fireEvent.pointerDown(el, { button: 0, pointerId: 1 });
  fireEvent.pointerUp(el, { pointerId: 1 });
};

beforeEach(() => {
  useAppStore.getState().reset();
  useAppStore.getState().setAudio(runningAudio);
  usePadStore.getState().reset();
  usePadStore.getState().setPad(neutralPadState() as never);
  usePadStore.getState().setSampler(neutralSampler() as never);
  useFxStore.getState().reset();
  telemetryStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("hot cue view", () => {
  it("lists eight rows and edits a cue", async () => {
    loadDeck();
    act(() => useAppStore.getState().patchDeck(0, { hotCues: [cue("Drop", 12.5), ...Array(7).fill(null)] }));
    render(<PadModeView deck={0} mode="hot_cue" />);
    expect(screen.getAllByRole("listitem")).toHaveLength(8);
    expect(screen.getByText("0:12")).toBeTruthy();

    const name = screen.getByRole("textbox", { name: "Hot cue 1 name" });
    fireEvent.change(name, { target: { value: "Build" } });
    fireEvent.blur(name);
    expect(commands.renameHotCue).toHaveBeenCalledWith(0, 0, "Build");

    fireEvent.click(pad("Set hot cue 2 at the playhead"));
    expect(commands.setHotCue).toHaveBeenCalledWith(0, 1);
    fireEvent.click(pad("Delete hot cue 1"));
    expect(commands.deleteHotCue).toHaveBeenCalledWith(0, 0);
    expect(pad("Delete hot cue 2")).toHaveProperty("disabled", true);
  });

  it("jumps with press and release", () => {
    loadDeck();
    act(() => useAppStore.getState().patchDeck(0, { hotCues: [cue("", 1), ...Array(7).fill(null)] }));
    render(<PadModeView deck={0} mode="hot_cue" />);
    const jump = pad("Jump to hot cue 1");
    fireEvent.pointerDown(jump);
    expect(commands.triggerHotCue).toHaveBeenCalledWith(0, 0, true);
    fireEvent.pointerUp(jump);
    expect(commands.triggerHotCue).toHaveBeenCalledWith(0, 0, false);
  });
});

describe("pad FX view", () => {
  it("engages on press, releases on release and reaches the shift layer", () => {
    act(() => {
      useFxStore.getState().setEffects(fxEffects() as never);
      const pads = Array.from({ length: 32 }, () => ({ effect: null, division: 1, release: false }));
      pads[0] = { effect: { provider: "builtin", id: "echo" } as never, division: 0.5, release: false };
      useFxStore.getState().setState({ pads } as never);
    });
    render(<PadModeView deck={1} mode="pad_fx1" />);
    const first = screen.getByRole("button", { name: /^Pad 1, Echo/ });
    press(first);
    expect(commands.padPress).toHaveBeenNthCalledWith(1, 1, "pad_fx1", 0, false, true);
    expect(commands.padPress).toHaveBeenNthCalledWith(2, 1, "pad_fx1", 0, false, false);

    fireEvent.click(pad("Deck 2 pad FX shift layer"));
    press(pad("Pad 3 (empty)"));
    expect(commands.padPress).toHaveBeenCalledWith(1, "pad_fx1", 2, true, true);
  });

  it("uses bank 2 for PAD FX 2 and shows the assignment editor on demand", () => {
    act(() => {
      useFxStore.getState().setEffects(fxEffects() as never);
      useFxStore.getState().setState({
        pads: Array.from({ length: 32 }, () => ({ effect: null, division: 1, release: false })),
      } as never);
    });
    render(<PadModeView deck={0} mode="pad_fx2" />);
    press(pad("Pad 1 (empty)"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "pad_fx2", 0, false, true);
    expect(screen.queryByText("Pad FX 2")).toBeNull();
    fireEvent.click(pad("Deck 1 edit pad effects"));
    expect(screen.getByRole("region", { name: "Pad 1 settings" })).toBeTruthy();
  });
});

describe("beat jump view", () => {
  it("shows signed sizes and sends pad presses", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_jump" />);
    press(pad("Jump back 2 beats"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_jump", 2, false, true);
    expect(screen.getByRole("button", { name: "Jump forward 8 beats" }).textContent).toBe("8 →");
  });

  it("scales the pad sizes with the SHIFT pads", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_jump" />);
    fireEvent.click(pad("Deck 1 double pad jump sizes"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_jump", 7, true, true);
    fireEvent.click(pad("Deck 1 halve pad jump sizes"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_jump", 6, true, true);
    act(() => {
      usePadStore
        .getState()
        .setPad(padStateWith({ jump_beats: [-2, 2, -4, 4, -8, 8, -16, 16] }));
    });
    expect(screen.getByRole("button", { name: "Jump back 16 beats" }).textContent).toBe("← 16");
  });

  it("includes the size selector and loop controls", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_jump" />);
    expect(screen.getByRole("group", { name: "Beat jump" })).toBeTruthy();
    expect(screen.getByLabelText("Beat jump size")).toBeTruthy();
  });
});

describe("sampler view", () => {
  const loadedState = () => {
    const state = neutralSampler();
    state.slots[0] = {
      ...state.slots[0],
      name: "Kick",
      path: "/s/kick.wav",
      loaded: true,
      duration_secs: 0.5,
      peaks: [0.2, 0.9, 0.4],
    } as never;
    return state;
  };

  it("shows deck 1 slots 1-8 and deck 2 slots 9-16", () => {
    render(<PadModeView deck={0} mode="sampler" />);
    expect(screen.getAllByRole("listitem")).toHaveLength(8);
    expect(screen.getByLabelText("Sampler slot 1")).toBeTruthy();
    expect(screen.queryByLabelText("Sampler slot 9")).toBeNull();
    cleanup();
    render(<PadModeView deck={1} mode="sampler" />);
    expect(screen.getByLabelText("Sampler slot 9")).toBeTruthy();
    expect(screen.queryByLabelText("Sampler slot 1")).toBeNull();
  });

  it("follows the shared layout", () => {
    act(() => usePadStore.getState().setSampler({ ...neutralSampler(), layout: "shared" } as never));
    render(<PadModeView deck={1} mode="sampler" />);
    expect(screen.getByLabelText("Sampler slot 1")).toBeTruthy();
  });

  it("plays a loaded slot and edits name, mode, sync and layout", () => {
    act(() => usePadStore.getState().setSampler(loadedState() as never));
    render(<PadModeView deck={0} mode="sampler" />);
    expect(pad("Play slot 2")).toHaveProperty("disabled", true);
    press(pad("Play slot 1"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "sampler", 0, false, true);
    expect(screen.getByRole("img", { name: "Slot 1 0.5 s" })).toBeTruthy();

    const name = screen.getByRole("textbox", { name: "Slot 1 name" });
    fireEvent.change(name, { target: { value: "Big kick" } });
    fireEvent.blur(name);
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(0, expect.objectContaining({ name: "Big kick", gain: null }));

    const slot = within(screen.getByLabelText("Sampler slot 1"));
    fireEvent.click(slot.getByRole("radio", { name: "Loop" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(0, expect.objectContaining({ play_mode: "loop", name: null }));
    fireEvent.click(slot.getByRole("button", { name: "Slot 1 sync" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(0, expect.objectContaining({ sync: true }));
    fireEvent.click(slot.getByRole("button", { name: "Slot 1 colour" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(0, expect.objectContaining({ color: 1 }));
    fireEvent.click(slot.getByRole("button", { name: "Clear slot 1" }));
    expect(commands.samplerClear).toHaveBeenCalledWith(0);

    fireEvent.click(screen.getByRole("radio", { name: "Shared" }));
    expect(commands.samplerSetLayout).toHaveBeenCalledWith("shared");
    fireEvent.click(screen.getByRole("button", { name: "Deck 1 sampler PFL" }));
    expect(commands.samplerSetPfl).toHaveBeenCalledWith(true);
  });

  it("loads a track dragged from the collection", () => {
    render(<PadModeView deck={0} mode="sampler" />);
    const target = screen.getByLabelText("Sampler slot 3");
    const data = new Map<string, string>([[DRAG_MIME, JSON.stringify({ kind: "tracks", ids: [42] })]]);
    const transfer = {
      types: [DRAG_MIME, `${DRAG_MIME}.tracks`],
      getData: (k: string) => data.get(k) ?? "",
      dropEffect: "none",
    };
    fireEvent.dragOver(target, { dataTransfer: transfer });
    fireEvent.drop(target, { dataTransfer: transfer });
    expect(commands.samplerLoadTrack).toHaveBeenCalledWith(2, 42);
  });
});

describe("keyboard view", () => {
  it("asks for a cue first, then plays semitones", () => {
    loadDeck();
    act(() => useAppStore.getState().patchDeck(0, { hotCues: [null, cue("Hook", 3), ...Array(6).fill(null)] }));
    const { rerender } = render(<PadModeView deck={0} mode="keyboard" />);
    expect(screen.getByTestId("keyboard-cue-0").textContent).toContain("choose the hot cue");
    press(pad("Choose hot cue 2"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "keyboard", 1, false, true);

    act(() => {
      usePadStore.getState().setPad(padStateWith({ keyboard_cue: 1 }));
    });
    rerender(<PadModeView deck={0} mode="keyboard" />);
    expect(screen.getByTestId("keyboard-cue-0").textContent).toContain("Cue 2, Hook");
    expect(pad("Play cue 2 at +4 semitones")).toBeTruthy();
    expect(pad("Play cue 2 at ±0 semitones")).toBeTruthy();
    fireEvent.click(pad("Choose another cue"));
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "keyboard");
    fireEvent.click(pad("Deck 1 pitch layout up"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "keyboard", 6, true, true);
    fireEvent.click(pad("Deck 1 delete chosen cue"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "keyboard", 0, true, true);
  });
});

describe("beat loop view", () => {
  it("lights the pad of the active loop and sends presses", () => {
    loadDeck(0, { loop_active: true, loop_region: { start: 0, end: 4 * RATE } });
    act(() => useAppStore.getState().patchDeck(0, { track: { ...track, bpm: 60 } }));
    render(<PadModeView deck={0} mode="beat_loop" />);
    expect(pad("Loop 4 beats").getAttribute("aria-pressed")).toBe("true");
    expect(pad("Loop 8 beats").getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByTestId("active-loop-0").textContent).toBe("4 beats");
    press(pad("Loop 1/4 beats"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_loop", 0, false, true);
  });

  it("shows no active loop", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_loop" />);
    expect(screen.getByTestId("active-loop-0").textContent).toBe("none");
  });
});

describe("key shift view", () => {
  it("shows the shift, marks the pitch pad and resets", async () => {
    loadDeck();
    act(() => useAppStore.getState().patchDeck(0, { keyShift: 2 }));
    render(<PadModeView deck={0} mode="key_shift" />);
    expect(screen.getByTestId("key-shift-0").textContent).toBe("+2 st");
    expect(pad("Key shift +2 semitones").getAttribute("aria-pressed")).toBe("true");
    expect(pad("Key shift +3 semitones").getAttribute("aria-pressed")).toBe("false");
    press(pad("Key shift +1 semitones"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "key_shift", 5, false, true);
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    await waitFor(() => expect(commands.setKeyShift).toHaveBeenCalledWith(0, 0));
    fireEvent.click(pad("Deck 1 pad layout up"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "key_shift", 6, true, true);
  });
});

describe("middle panel", () => {
  it("shows one view per deck and follows the pad mode", () => {
    render(<PadMiddlePanel />);
    expect(screen.getByTestId("pad-view-0").dataset.mode).toBe("hot_cue");
    expect(screen.getByTestId("pad-view-1").dataset.mode).toBe("hot_cue");
    act(() => useAppStore.getState().setPadMode(1, "sampler"));
    expect(screen.getByTestId("pad-view-1").dataset.mode).toBe("sampler");
    expect(screen.getByTestId("pad-view-0").dataset.mode).toBe("hot_cue");
    expect(screen.getByLabelText("Sampler slot 9")).toBeTruthy();
  });

  it("switches by click and by SHIFT-click, through the backend", () => {
    render(<PadMiddlePanel />);
    const tabs = within(screen.getByLabelText("Deck 1 pad mode"));
    fireEvent.click(tabs.getByRole("radio", { name: /^Beat jump/ }));
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "beat_jump");
    expect(useAppStore.getState().padModes[0]).toBe("beat_jump");

    fireEvent.click(tabs.getByRole("radio", { name: /^Beat jump/ }), { shiftKey: true });
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "beat_loop");
    expect(screen.getByTestId("pad-view-0").dataset.mode).toBe("beat_loop");
    expect(tabs.getByRole("radio", { name: /^Beat jump/ }).textContent).toBe("Beat loop");

    fireEvent.click(tabs.getByRole("radio", { name: /^Sampler/ }), { shiftKey: true });
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "key_shift");
    expect(screen.getByTestId("pad-view-1").dataset.mode).toBe("hot_cue");
  });

  it("renders every mode without error", () => {
    loadDeck();
    for (const mode of [
      "hot_cue",
      "pad_fx1",
      "beat_jump",
      "sampler",
      "keyboard",
      "pad_fx2",
      "beat_loop",
      "key_shift",
    ] as const) {
      act(() => useAppStore.getState().setPadMode(0, mode));
      const { unmount } = render(<PadModeView deck={0} mode={mode} />);
      unmount();
    }
  });
});
