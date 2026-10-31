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
  ({
    id: 1,
    kind: { kind: "hot", index: 0 },
    start,
    end: null,
    color: null,
    name,
  }) as never;

function loadDeck(deck = 0, patch: Parameters<typeof telemetryFor>[2] = {}) {
  act(() => {
    useAppStore.getState().patchDeck(deck, { track });
    telemetryStore
      .getState()
      .push(telemetryFor(1, deck, { loaded: true, duration: 200 * RATE, ...patch }));
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

/** Opens the right-click menu of a pad. */
const openMenu = (el: HTMLElement) => fireEvent.contextMenu(el, { clientX: 5, clientY: 5 });

/** The pads of a group, in order. */
const padsOf = (group: string) =>
  within(screen.getByRole("group", { name: group })).getAllByRole("button");

describe("pad grid", () => {
  it("shows eight pads in every mode", () => {
    loadDeck();
    const groups: [Parameters<typeof PadModeView>[0]["mode"], string][] = [
      ["hot_cue", "Deck 1 hot cues"],
      ["pad_fx1", "Deck 1 pad FX"],
      ["beat_jump", "Deck 1 beat jump"],
      ["sampler", "Deck 1 sampler slots"],
      ["keyboard", "Deck 1 keyboard"],
      ["pad_fx2", "Deck 1 pad FX"],
      ["beat_loop", "Deck 1 beat loop"],
      ["key_shift", "Deck 1 key shift"],
    ];
    for (const [mode, name] of groups) {
      const { unmount } = render(<PadModeView deck={0} mode={mode} />);
      expect(padsOf(name)).toHaveLength(8);
      unmount();
    }
  });

  it("is a 2 x 4 grid that fills its container", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="hot_cue" />);
    const group = screen.getByRole("group", { name: "Deck 1 hot cues" });
    expect(group.className).toContain("grid-cols-4");
    expect(group.className).toContain("grid-rows-2");
    expect(group.className).toContain("size-full");
  });
});

describe("hot cue view", () => {
  it("shows the cue name and time on a lit pad and Empty on a free one", () => {
    loadDeck();
    act(() =>
      useAppStore.getState().patchDeck(0, { hotCues: [cue("Drop", 12.5), ...Array(7).fill(null)] }),
    );
    render(<PadModeView deck={0} mode="hot_cue" />);
    const first = pad("Hot cue 1");
    expect(first.textContent).toContain("Drop");
    expect(first.textContent).toContain("0:12");
    expect(first.getAttribute("aria-pressed")).toBe("true");
    expect(first.getAttribute("style")).toContain("--cue-1");
    expect(pad("Hot cue 2 (empty)").textContent).toContain("Empty");
    expect(pad("Hot cue 2 (empty)").getAttribute("aria-pressed")).toBe("false");
  });

  it("click is a pad press through the controller path, SHIFT+click the shifted action", () => {
    loadDeck();
    act(() =>
      useAppStore.getState().patchDeck(0, { hotCues: [cue("", 1), ...Array(7).fill(null)] }),
    );
    render(<PadModeView deck={0} mode="hot_cue" />);
    press(pad("Hot cue 1"));
    expect(commands.padPress.mock.calls).toEqual([
      [0, "hot_cue", 0, false, true],
      [0, "hot_cue", 0, false, false],
    ]);
    commands.padPress.mockClear();
    fireEvent.pointerDown(pad("Hot cue 1"), {
      button: 0,
      pointerId: 1,
      shiftKey: true,
    });
    fireEvent.pointerUp(pad("Hot cue 1"), { pointerId: 1 });
    expect(commands.padPress.mock.calls).toEqual([
      [0, "hot_cue", 0, true, true],
      [0, "hot_cue", 0, true, false],
    ]);
  });

  it("shows the pressed state while the pointer is down", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="hot_cue" />);
    const first = pad("Hot cue 1 (empty)");
    fireEvent.pointerDown(first, { button: 0, pointerId: 1 });
    expect(first.hasAttribute("data-pressed")).toBe(true);
    fireEvent.pointerUp(first, { pointerId: 1 });
    expect(first.hasAttribute("data-pressed")).toBe(false);
  });

  it("is disabled until a track is loaded", () => {
    render(<PadModeView deck={0} mode="hot_cue" />);
    expect(pad("Hot cue 1 (empty)")).toHaveProperty("disabled", true);
  });

  it("sets, renames and deletes from the right-click menu", async () => {
    loadDeck();
    act(() =>
      useAppStore.getState().patchDeck(0, { hotCues: [cue("Drop", 12.5), ...Array(7).fill(null)] }),
    );
    render(<PadModeView deck={0} mode="hot_cue" />);

    openMenu(pad("Hot cue 2 (empty)"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Set at playhead" }));
    expect(commands.setHotCue).toHaveBeenCalledWith(0, 1);

    openMenu(pad("Hot cue 1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename..." }));
    const name = await screen.findByRole("textbox", { name: "Name" });
    fireEvent.change(name, { target: { value: "Build" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(commands.renameHotCue).toHaveBeenCalledWith(0, 0, "Build");

    openMenu(pad("Hot cue 1"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    expect(commands.deleteHotCue).toHaveBeenCalledWith(0, 0);
  });
});

describe("pad FX view", () => {
  const withEcho = () =>
    act(() => {
      useFxStore.getState().setEffects(fxEffects() as never);
      const pads = Array.from({ length: 32 }, () => ({
        effect: null,
        division: 1,
        release: false,
      }));
      pads[0] = {
        effect: { provider: "builtin", id: "echo" } as never,
        division: 0.5,
        release: false,
      };
      useFxStore.getState().setState({ pads } as never);
    });

  it("engages on press, releases on release and reaches the shift layer with SHIFT+click", () => {
    withEcho();
    render(<PadModeView deck={1} mode="pad_fx1" />);
    const first = screen.getByRole("button", { name: /^Pad 1, Echo/ });
    expect(first.textContent).toContain("Echo");
    press(first);
    expect(commands.padPress).toHaveBeenNthCalledWith(1, 1, "pad_fx1", 0, false, true);
    expect(commands.padPress).toHaveBeenNthCalledWith(2, 1, "pad_fx1", 0, false, false);

    fireEvent.pointerDown(pad("Pad 3 (empty)"), {
      button: 0,
      pointerId: 1,
      shiftKey: true,
    });
    expect(commands.padPress).toHaveBeenCalledWith(1, "pad_fx1", 2, true, true);
  });

  it("uses bank 2 for PAD FX 2 and opens the assignment editor from the menu", async () => {
    act(() => {
      useFxStore.getState().setEffects(fxEffects() as never);
      useFxStore.getState().setState({
        pads: Array.from({ length: 32 }, () => ({
          effect: null,
          division: 1,
          release: false,
        })),
      } as never);
    });
    render(<PadModeView deck={0} mode="pad_fx2" />);
    press(pad("Pad 1 (empty)"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "pad_fx2", 0, false, true);
    openMenu(pad("Pad 1 (empty)"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Deck 1 edit pad effects" }));
    expect(await screen.findByRole("region", { name: "Pad 1 settings" })).toBeTruthy();
  });
});

describe("beat jump view", () => {
  it("shows signed sizes and sends pad presses", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_jump" />);
    press(pad("Jump back 2 beats"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_jump", 2, false, true);
    expect(pad("Jump forward 8 beats").textContent).toContain("8 →");
  });

  it("scales the pad sizes with the SHIFT pads from the menu", async () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_jump" />);
    openMenu(pad("Jump back 1 beats"));
    fireEvent.click(
      await screen.findByRole("menuitem", {
        name: "Deck 1 double pad jump sizes",
      }),
    );
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_jump", 7, true, true);
    openMenu(pad("Jump back 1 beats"));
    fireEvent.click(
      await screen.findByRole("menuitem", {
        name: "Deck 1 halve pad jump sizes",
      }),
    );
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_jump", 6, true, true);
    act(() => {
      usePadStore.getState().setPad(padStateWith({ jump_beats: [-2, 2, -4, 4, -8, 8, -16, 16] }));
    });
    expect(pad("Jump back 16 beats").textContent).toContain("← 16");
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
    expect(padsOf("Deck 1 sampler slots")).toHaveLength(8);
    expect(screen.getByLabelText("Sample slot 1 (empty)")).toBeTruthy();
    expect(screen.queryByLabelText("Sample slot 9 (empty)")).toBeNull();
    cleanup();
    render(<PadModeView deck={1} mode="sampler" />);
    expect(screen.getByLabelText("Sample slot 9 (empty)")).toBeTruthy();
    expect(screen.queryByLabelText("Sample slot 1 (empty)")).toBeNull();
  });

  it("names the pad after the sample and marks the slot the engine plays", () => {
    act(() => usePadStore.getState().setSampler(loadedState() as never));
    render(<PadModeView deck={0} mode="sampler" />);
    const first = pad("Sample slot 1, Kick");
    expect(first.textContent).toContain("Kick");
    expect(first.textContent).toContain("0.5 s");
    expect(first.className).not.toContain("ring-2");
    const snapshot = telemetryFor(2, 0);
    // The fixture shares its slot array between snapshots: copy before editing.
    const slots = [...snapshot.sampler] as typeof snapshot.sampler;
    slots[0] = { loaded: true, playing: true, progress: 0.5 };
    snapshot.sampler = slots;
    act(() => telemetryStore.getState().push(snapshot));
    expect(pad("Sample slot 1, Kick").className).toContain("ring-2");
  });

  it("follows the shared layout", () => {
    act(() =>
      usePadStore.getState().setSampler({ ...neutralSampler(), layout: "shared" } as never),
    );
    render(<PadModeView deck={1} mode="sampler" />);
    expect(screen.getByLabelText("Sample slot 1 (empty)")).toBeTruthy();
  });

  it("plays a slot on press and edits it from the right-click menu", async () => {
    act(() => usePadStore.getState().setSampler(loadedState() as never));
    render(<PadModeView deck={0} mode="sampler" />);
    press(pad("Sample slot 1, Kick"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "sampler", 0, false, true);

    openMenu(pad("Sample slot 1, Kick"));
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "Loop" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(
      0,
      expect.objectContaining({ play_mode: "loop", name: null }),
    );
    openMenu(pad("Sample slot 1, Kick"));
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "Sync to leader" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(
      0,
      expect.objectContaining({ sync: true }),
    );
    openMenu(pad("Sample slot 1, Kick"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Next colour" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(0, expect.objectContaining({ color: 1 }));
    openMenu(pad("Sample slot 1, Kick"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename..." }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Name" }), {
      target: { value: "Big kick" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(commands.samplerSetSlot).toHaveBeenCalledWith(
      0,
      expect.objectContaining({ name: "Big kick", gain: null }),
    );
    openMenu(pad("Sample slot 1, Kick"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Clear" }));
    expect(commands.samplerClear).toHaveBeenCalledWith(0);
  });

  it("changes layout, level and PFL from the mode strip", () => {
    render(<PadMiddlePanel />);
    act(() => useAppStore.getState().setPadMode(0, "sampler"));
    fireEvent.click(screen.getByRole("radio", { name: "Shared" }));
    expect(commands.samplerSetLayout).toHaveBeenCalledWith("shared");
    fireEvent.click(screen.getByRole("button", { name: "Deck 1 sampler PFL" }));
    expect(commands.samplerSetPfl).toHaveBeenCalledWith(true);
    expect(screen.getByLabelText("Deck 1 sampler level meter")).toBeTruthy();
  });

  it("loads a track dragged from the collection", () => {
    render(<PadModeView deck={0} mode="sampler" />);
    const target = screen.getByLabelText("Sample slot 3 (empty)");
    const data = new Map<string, string>([
      [DRAG_MIME, JSON.stringify({ kind: "tracks", ids: [42] })],
    ]);
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
  it("asks for a cue first, then plays semitones", async () => {
    loadDeck();
    act(() =>
      useAppStore.getState().patchDeck(0, {
        hotCues: [null, cue("Hook", 3), ...Array(6).fill(null)],
      }),
    );
    const { rerender } = render(<PadModeView deck={0} mode="keyboard" />);
    expect(pad("Choose hot cue 2").textContent).toContain("Hook");
    press(pad("Choose hot cue 2"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "keyboard", 1, false, true);

    act(() => {
      usePadStore.getState().setPad(padStateWith({ keyboard_cue: 1 }));
    });
    rerender(<PadModeView deck={0} mode="keyboard" />);
    expect(pad("Play cue 2 at +4 semitones")).toBeTruthy();
    expect(pad("Play cue 2 at ±0 semitones")).toBeTruthy();
    openMenu(pad("Play cue 2 at ±0 semitones"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Choose another cue" }));
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "keyboard");
    openMenu(pad("Play cue 2 at ±0 semitones"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Deck 1 pitch layout up" }));
    expect(commands.padPress).toHaveBeenCalledWith(0, "keyboard", 6, true, true);
    openMenu(pad("Play cue 2 at ±0 semitones"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Deck 1 delete chosen cue" }));
    expect(commands.padPress).toHaveBeenCalledWith(0, "keyboard", 0, true, true);
  });
});

describe("beat loop view", () => {
  it("lights the pad of the active loop and sends presses", () => {
    loadDeck(0, {
      loop_active: true,
      loop_region: { start: 0, end: 4 * RATE },
    });
    act(() => useAppStore.getState().patchDeck(0, { track: { ...track, bpm: 60 } }));
    render(<PadModeView deck={0} mode="beat_loop" />);
    expect(pad("Loop 4 beats").getAttribute("aria-pressed")).toBe("true");
    expect(pad("Loop 8 beats").getAttribute("aria-pressed")).toBe("false");
    press(pad("Loop 1/4 beats"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "beat_loop", 0, false, true);
  });

  it("lights no pad without an active loop", () => {
    loadDeck();
    render(<PadModeView deck={0} mode="beat_loop" />);
    for (const el of padsOf("Deck 1 beat loop")) {
      expect(el.getAttribute("aria-pressed")).toBe("false");
    }
  });
});

describe("key shift view", () => {
  it("marks the pitch pad and resets from the menu", async () => {
    loadDeck();
    act(() => useAppStore.getState().patchDeck(0, { keyShift: 2 }));
    render(<PadModeView deck={0} mode="key_shift" />);
    expect(pad("Key shift +2 semitones").getAttribute("aria-pressed")).toBe("true");
    expect(pad("Key shift +3 semitones").getAttribute("aria-pressed")).toBe("false");
    openMenu(pad("Key shift +2 semitones"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Reset key shift" }));
    await waitFor(() => expect(commands.setKeyShift).toHaveBeenCalledWith(0, 0));
    openMenu(pad("Key shift +2 semitones"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Deck 1 pad layout up" }));
    expect(commands.padPress).toHaveBeenCalledWith(0, "key_shift", 6, true, true);
    press(pad("Key shift +1 semitones"));
    expect(commands.padPress).toHaveBeenCalledWith(0, "key_shift", 5, false, true);
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
    expect(screen.getByLabelText("Sample slot 9 (empty)")).toBeTruthy();
  });

  it("switches by click and by SHIFT-click, through the backend", () => {
    render(<PadMiddlePanel />);
    const tabs = within(screen.getByLabelText("Deck 1 pad mode"));
    fireEvent.click(tabs.getByRole("radio", { name: /^Beat jump/ }));
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "beat_jump");
    expect(useAppStore.getState().padModes[0]).toBe("beat_jump");

    fireEvent.click(tabs.getByRole("radio", { name: /^Beat jump/ }), {
      shiftKey: true,
    });
    expect(commands.setPadMode).toHaveBeenCalledWith(0, "beat_loop");
    expect(screen.getByTestId("pad-view-0").dataset.mode).toBe("beat_loop");
    expect(tabs.getByRole("radio", { name: /^Beat jump/ }).textContent).toBe("Beat loop");

    fireEvent.click(tabs.getByRole("radio", { name: /^Sampler/ }), {
      shiftKey: true,
    });
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
