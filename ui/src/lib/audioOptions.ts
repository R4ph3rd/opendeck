import type { AudioDeviceInfo, ChannelPair } from "../ipc/bindings";

/** Rates offered when the device supports them. */
export const STANDARD_RATES = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

/** Buffer sizes in frames offered when the device supports them. */
export const STANDARD_BUFFERS = [64, 128, 256, 512, 1024, 2048];

/** Pairs shown when the device is unknown (system default before listing). */
const FALLBACK_CHANNELS = 4;

/** Sample rates in `STANDARD_RATES` that fall inside one of the device's ranges. */
export function sampleRateOptions(device: AudioDeviceInfo | undefined): number[] {
  if (!device || device.sample_rate_ranges.length === 0) {
    return STANDARD_RATES.slice(0, 2);
  }
  return STANDARD_RATES.filter((rate) =>
    device.sample_rate_ranges.some(([min, max]) => rate >= min && rate <= max),
  );
}

/** Buffer sizes in `STANDARD_BUFFERS` inside the device's range (all when unknown). */
export function bufferOptions(device: AudioDeviceInfo | undefined): number[] {
  const range = device?.buffer_frames;
  if (!range) {
    return STANDARD_BUFFERS;
  }
  const [min, max] = range;
  return STANDARD_BUFFERS.filter((frames) => frames >= min && frames <= max);
}

/** Consecutive channel pairs (0/1, 2/3, ...) the device can carry. */
export function channelPairs(device: AudioDeviceInfo | undefined): ChannelPair[] {
  const channels = Math.max(2, device?.max_channels ?? FALLBACK_CHANNELS);
  const pairs: ChannelPair[] = [];
  for (let left = 0; left + 1 < channels; left += 2) {
    pairs.push({ left, right: left + 1 });
  }
  return pairs;
}

/** One-based label, for example `3/4`. */
export function pairLabel(pair: ChannelPair): string {
  return `${pair.left + 1}/${pair.right + 1}`;
}

/** String key for a pair in a Select. */
export function pairKey(pair: ChannelPair): string {
  return `${pair.left}-${pair.right}`;
}

export function parsePairKey(key: string): ChannelPair | null {
  const [left, right] = key.split("-").map(Number);
  if (left === undefined || right === undefined || Number.isNaN(left) || Number.isNaN(right)) {
    return null;
  }
  return { left, right };
}
