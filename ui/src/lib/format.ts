const SECONDS_PER_MINUTE = 60;
const PERCENT = 100;

/** `m:ss`, or `h:mm:ss` from an hour up. Negative and non-finite values show as zero. */
export function formatTime(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const hours = Math.floor(total / (SECONDS_PER_MINUTE * SECONDS_PER_MINUTE));
  const minutes = Math.floor(total / SECONDS_PER_MINUTE) % SECONDS_PER_MINUTE;
  const secs = total % SECONDS_PER_MINUTE;
  const ss = String(secs).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${ss}`;
  }
  return `${minutes}:${ss}`;
}

/** Remaining time with a leading minus, for the countdown readout. */
export function formatRemaining(seconds: number): string {
  return `-${formatTime(seconds)}`;
}

/** Signed tempo change in percent with one decimal, for example `+3.2%`. */
export function formatTempo(fader: number, maxFraction: number): string {
  const percent = fader * maxFraction * PERCENT;
  const rounded = Math.abs(percent) < 0.05 ? 0 : percent;
  const sign = rounded > 0 ? "+" : rounded < 0 ? "-" : "";
  return `${sign}${Math.abs(rounded).toFixed(1)}%`;
}

/** Output buffer latency in milliseconds for a buffer size at a rate. */
export function bufferLatencyMs(frames: number, sampleRate: number): number {
  return sampleRate > 0 ? (frames * 1000) / sampleRate : 0;
}
