/**
 * Deterministic synthetic data for stories, tests and the perf check: a
 * four-on-the-floor track with a breakdown, as peak levels in the same layout
 * the analysis crate produces (64, 256, 1024 frames per point plus an overview
 * of about 2048 points). No randomness beyond a fixed-seed generator.
 */
import type { BeatGrid, StereoMeter } from "../ipc/bindings";
import { EMPTY_DECK_FRAME, type DeckFrame, type DeckFrameSource } from "./deckFrame";
import type { DeckMarkers } from "./markers";
import type { TrackPeaks } from "./peakCache";
import type { PeakLevelData } from "./peaks";

export const MOCK_SAMPLE_RATE = 44100;

/** Small fast PRNG, fixed seed so every run draws the same waveform. */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const byte = (v: number): number => Math.max(0, Math.min(255, Math.round(v * 255)));

/** Peak levels for a synthetic track of `seconds` at `bpm`. */
export function synthPeaks(seconds: number, bpm = MOCK_BPM, seed = 1): TrackPeaks {
  const rand = mulberry32(seed);
  const frames = Math.floor(seconds * MOCK_SAMPLE_RATE);
  const fpp0 = 64;
  const count0 = Math.ceil(frames / fpp0);
  const beat = 60 / bpm;
  const l0 = new Uint8Array(count0 * 4);
  for (let i = 0; i < count0; i += 1) {
    const t = (i * fpp0) / MOCK_SAMPLE_RATE;
    const bar = Math.floor(t / (beat * 4));
    // 8 bars on, 4 bars breakdown without kick.
    const breakdown = bar % 12 >= 8;
    const phase = (t % beat) / beat;
    const kick = breakdown ? 0 : Math.exp(-phase * 7);
    const hat = Math.exp(-(((phase + 0.5) % 1) * 14)) * (0.5 + 0.3 * rand());
    const pad = 0.25 + 0.15 * Math.sin(t * 0.7) + (breakdown ? 0.2 : 0);
    const low = Math.min(1, kick * 0.95 + 0.05);
    const mid = Math.min(1, pad * (0.6 + 0.4 * rand()) + kick * 0.15);
    const high = Math.min(1, hat * 0.6 + 0.05 * rand());
    const peak = Math.min(1, Math.max(low, mid, high) * 0.85 + 0.12 * rand());
    const o = i * 4;
    l0[o] = byte(peak);
    l0[o + 1] = byte(low);
    l0[o + 2] = byte(mid);
    l0[o + 3] = byte(high);
  }
  const level = (fpp: number, points: Uint8Array): PeakLevelData => ({
    sampleRate: MOCK_SAMPLE_RATE,
    framesPerPoint: fpp,
    count: points.length / 4,
    points,
  });
  const coarsen = (src: Uint8Array, group: number): Uint8Array => {
    const n = Math.ceil(src.length / 4 / group);
    const out = new Uint8Array(n * 4);
    for (let i = 0; i < src.length / 4; i += 1) {
      const o = Math.floor(i / group) * 4;
      for (let k = 0; k < 4; k += 1) {
        const v = src[i * 4 + k] ?? 0;
        if (v > (out[o + k] ?? 0)) out[o + k] = v;
      }
    }
    return out;
  };
  const l1 = coarsen(l0, 4);
  const l2 = coarsen(l1, 4);
  const overviewFpp = Math.ceil(frames / 2048);
  const group = Math.max(1, Math.round(overviewFpp / fpp0));
  const ov = coarsen(l0, group);
  return {
    detail: [level(fpp0, l0), level(256, l1), level(1024, l2)],
    overview: level(group * fpp0, ov),
  };
}

export const MOCK_BPM = 124;
export const MOCK_GRID: BeatGrid = { anchor: 0.18, bpm: MOCK_BPM, beats_per_bar: 4 };

/** Markers with a beat grid, eight hot cues, two memory cues and a saved loop. */
export function mockMarkers(duration: number): DeckMarkers {
  const bar = (60 / MOCK_BPM) * 4;
  const cue = (index: number, start: number, end: number | null = null) => ({
    id: index + 1,
    kind: { kind: "hot" as const, index },
    start,
    end,
    color: null,
    name: "",
  });
  return {
    grid: MOCK_GRID,
    hotCues: [
      cue(0, 0.18),
      cue(1, 0.18 + bar * 8),
      cue(2, 0.18 + bar * 16, 0.18 + bar * 18),
      null,
      cue(4, duration * 0.5),
      null,
      null,
      cue(7, duration * 0.9),
    ],
    memoryCues: [
      { id: 20, kind: { kind: "memory" }, start: 0.18 + bar * 4, end: null, color: null, name: "" },
      { id: 21, kind: { kind: "memory" }, start: 0.18 + bar * 12, end: null, color: null, name: "" },
    ],
  };
}

/** A controllable fake deck driven by the wall clock, for stories and the perf check. */
export class MockDeck {
  private anchorPos: number;
  private anchorAt: number;
  playing: boolean;
  rate: number;
  loop: { start: number; end: number } | null = null;
  private seq = 0;

  constructor(
    readonly duration: number,
    { position = 0, playing = true, rate = 1 }: { position?: number; playing?: boolean; rate?: number } = {},
  ) {
    this.anchorPos = position;
    this.anchorAt = performance.now();
    this.playing = playing;
    this.rate = rate;
  }

  private positionAt(now: number): number {
    let p = this.anchorPos + (this.playing ? ((now - this.anchorAt) / 1000) * this.rate : 0);
    if (this.loop && this.playing && p >= this.loop.end) {
      const len = this.loop.end - this.loop.start;
      p = this.loop.start + ((p - this.loop.start) % len);
      this.anchorPos = p;
      this.anchorAt = now;
    }
    return Math.min(this.duration, Math.max(0, p));
  }

  seek(seconds: number): void {
    this.anchorPos = seconds;
    this.anchorAt = performance.now();
    this.seq += 1;
  }

  setPlaying(playing: boolean): void {
    this.anchorPos = this.positionAt(performance.now());
    this.anchorAt = performance.now();
    this.playing = playing;
    this.seq += 1;
  }

  setRate(rate: number): void {
    this.anchorPos = this.positionAt(performance.now());
    this.anchorAt = performance.now();
    this.rate = rate;
    this.seq += 1;
  }

  readonly source: DeckFrameSource = (now): DeckFrame => ({
    ...EMPTY_DECK_FRAME,
    loaded: true,
    playing: this.playing,
    position: this.positionAt(now),
    duration: this.duration,
    rate: this.rate,
    loop: this.loop,
    loopActive: this.loop !== null && this.playing,
    sequence: this.seq,
  });
}

/** Meter values that move like music: a slow swell plus a beat, optionally clipping. */
export function mockMeter(now: number, { clip = false, offset = 0 }: { clip?: boolean; offset?: number } = {}): StereoMeter {
  const t = now / 1000 + offset;
  const beat = Math.exp(-(((t * 2) % 1) * 5));
  const swell = 0.5 + 0.5 * Math.sin(t * 0.6);
  const peak = Math.min(clip ? 1.2 : 0.98, 0.05 + 0.5 * swell * 0.6 + 0.6 * beat * (0.5 + 0.5 * swell));
  const rms = peak * 0.55;
  return { peak_l: peak, peak_r: peak * 0.92, rms_l: rms, rms_r: rms * 0.92 };
}
