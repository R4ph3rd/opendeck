import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Toaster, toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { RATE, runningAudio, telemetry, track } from "../test/fixtures";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { open } = await import("@tauri-apps/plugin-dialog");
const { useAppStore } = await import("../state/app");
const { telemetryStore } = await import("../state/telemetry");
const { releaseAllCues } = await import("../state/actions");
const { DeckPanel } = await import("./DeckPanel");

const { commands } = bindings;

/** Loaded deck A, 200 s long, playhead at `seconds`. */
function loadDeckA(seconds = 0, playing = false) {
  act(() => {
    useAppStore.getState().patchDeck(0, { track });
    telemetryStore.getState().push(
      telemetry(1, {
        loaded: true,
        playing,
        position: seconds * RATE,
        duration: 200 * RATE,
      }),
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
  releaseAllCues();
  toast.dismiss();
  cleanup();
});

describe("DeckPanel", () => {
  it("shows the empty state until a track is loaded", () => {
    render(<DeckPanel deck={0} />);
    expect(screen.getByText("No track loaded")).toBeTruthy();
    expect(screen.getByRole("button", { name: "PLAY" })).toHaveProperty("disabled", true);
  });

  it("puts the hot cue grid, loop and jump strip and the pad mode tabs inside the deck", () => {
    render(<DeckPanel deck={0} />);
    const deck = within(screen.getByRole("region", { name: "Deck 1" }));
    expect(deck.getByRole("group", { name: "Deck 1 hot cues" })).toBeTruthy();
    expect(deck.getAllByRole("button", { name: /^Hot cue \d/ })).toHaveLength(8);
    expect(deck.getByRole("group", { name: "Loop" })).toBeTruthy();
    expect(deck.getByRole("group", { name: "Beat jump" })).toBeTruthy();
    expect(deck.getByLabelText("Deck 1 pad mode")).toBeTruthy();
  });

  it("opens the picker and loads the chosen file", async () => {
    vi.mocked(open).mockResolvedValue("/music/tone.wav");
    commands.loadTrackPath.mockResolvedValue({
      status: "ok",
      data: track,
    } as never);
    render(<DeckPanel deck={0} />);
    fireEvent.click(screen.getByRole("button", { name: "Open file" }));
    await waitFor(() => expect(commands.loadTrackPath).toHaveBeenCalledWith(0, "/music/tone.wav"));
    expect(commands.supportedAudioExtensions).toHaveBeenCalled();
    await waitFor(() => expect(screen.getByTestId("title-0").textContent).toBe("Night Drive"));
  });

  it("shows an error toast when the load command fails", async () => {
    vi.mocked(open).mockResolvedValue("/music/bad.wav");
    commands.loadTrackPath.mockResolvedValue({
      status: "error",
      error: { kind: "media", message: "unsupported format" },
    } as never);
    render(
      <Toaster>
        <DeckPanel deck={0} />
      </Toaster>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open file" }));
    expect(await screen.findByText("unsupported format")).toBeTruthy();
    expect(useAppStore.getState().decks[0]?.loading).toBe(false);
  });

  it("shows title and artist, and enables transport once the engine has the track", () => {
    render(<DeckPanel deck={0} />);
    loadDeckA();
    expect(screen.getByTestId("title-0").textContent).toBe("Night Drive");
    expect(screen.getByTestId("artist-0").textContent).toBe("Test Artist");
    expect(screen.getByRole("button", { name: "PLAY" })).toHaveProperty("disabled", false);
  });

  it("toggles play and labels the button from telemetry", () => {
    render(<DeckPanel deck={0} />);
    loadDeckA();
    fireEvent.click(screen.getByRole("button", { name: "PLAY" }));
    expect(commands.togglePlaying).toHaveBeenCalledWith(0);
    act(() =>
      telemetryStore.getState().push(telemetry(2, { loaded: true, playing: true, duration: 1 })),
    );
    expect(screen.getByRole("button", { name: "PAUSE" })).toBeTruthy();
  });

  it("sends CUE on pointer down and release on pointer up, once each", () => {
    render(<DeckPanel deck={0} />);
    loadDeckA();
    const cue = screen.getByRole("button", { name: "Cue" });
    fireEvent.pointerDown(cue, { pointerId: 1 });
    fireEvent.pointerDown(cue, { pointerId: 1 });
    expect(commands.cue.mock.calls).toEqual([[0, true]]);
    fireEvent.pointerUp(cue, { pointerId: 1 });
    fireEvent.pointerUp(cue, { pointerId: 1 });
    expect(commands.cue.mock.calls).toEqual([
      [0, true],
      [0, false],
    ]);
  });

  it("updates elapsed and remaining time from telemetry", async () => {
    render(<DeckPanel deck={0} />);
    loadDeckA(0);
    await waitFor(() => expect(screen.getByTestId("remaining-0").textContent).toBe("03:20.0"));
    for (let sequence = 2; sequence < 40; sequence++) {
      act(() =>
        telemetryStore.getState().push(
          telemetry(sequence, {
            loaded: true,
            playing: true,
            position: (sequence + 60) * RATE,
            duration: 200 * RATE,
          }),
        ),
      );
    }
    await waitFor(() => expect(screen.getByTestId("elapsed-0").textContent).toBe("01:39.0"));
    expect(screen.getByTestId("remaining-0").textContent).toBe("01:41.0");
  });

  it("seeks to the clicked fraction of the track", () => {
    render(<DeckPanel deck={0} />);
    loadDeckA();
    const bar = screen.getByRole("slider", { name: "Deck 1 position" });
    bar.getBoundingClientRect = () =>
      ({
        left: 100,
        width: 400,
        top: 0,
        height: 10,
        right: 500,
        bottom: 10,
        x: 100,
        y: 0,
      }) as DOMRect;
    fireEvent.pointerDown(bar, { clientX: 300 });
    expect(commands.seek).toHaveBeenCalledWith(0, 100);
  });

  it("changes tempo range, key lock and bend", async () => {
    render(<DeckPanel deck={0} />);
    loadDeckA();
    fireEvent.pointerDown(screen.getByRole("combobox", { name: "Tempo range" }), {
      button: 0,
      ctrlKey: false,
      pointerType: "mouse",
    });
    fireEvent.click(await screen.findByRole("option", { name: "±6" }));
    expect(commands.setTempoRange).toHaveBeenCalledWith(0, "percent6");
    fireEvent.click(screen.getByRole("button", { name: "KEY LOCK" }));
    expect(commands.setKeyLock).toHaveBeenCalledWith(0, true);
    const up = screen.getByRole("button", { name: "Pitch bend up" });
    fireEvent.pointerDown(up, { pointerId: 1 });
    fireEvent.pointerUp(up, { pointerId: 1 });
    expect(commands.pitchBend.mock.calls).toEqual([
      [0, 1],
      [0, 0],
    ]);
  });

  it("moves the tempo fader from the keyboard and shows the readout", () => {
    render(<DeckPanel deck={0} />);
    loadDeckA();
    act(() => useAppStore.getState().patchDeck(0, { tempoFader: 0.5 }));
    expect(screen.getByTestId("tempo-readout-0").textContent).toBe("+5.0%");
    const thumb = within(screen.getByRole("group", { name: "Tempo fader" })).getByRole("slider");
    fireEvent.keyDown(thumb, { key: "ArrowUp" });
    expect(commands.setTempoFader).toHaveBeenCalled();
  });
});
