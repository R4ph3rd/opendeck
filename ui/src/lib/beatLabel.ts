/** Short label for a beat count: `1/4`, `1/2`, `1`, `32`. */
export function beatLabel(beats: number): string {
  if (beats === 0.25) return "1/4";
  if (beats === 0.5) return "1/2";
  return String(beats);
}
