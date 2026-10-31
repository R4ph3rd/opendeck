/** Semitone of each note letter from C. */
const NOTE_PC: Record<string, number> = {
  C: 0,
  D: 2,
  E: 4,
  F: 5,
  G: 7,
  A: 9,
  B: 11,
};

/** Pitch class (0..11) of a key such as `Am`, `F#m` or `Bb`, as its relative major. `null` when unparsable. */
export function keyPitchClass(key: string | null | undefined): number | null {
  const match = /^([A-G])([#b]?)(m|min)?$/.exec((key ?? "").trim());
  const letter = match?.[1];
  if (!letter) {
    return null;
  }
  const accidental = match[2] === "#" ? 1 : match[2] === "b" ? -1 : 0;
  const minor = match[3] !== undefined;
  const root = (NOTE_PC[letter] ?? 0) + accidental + (minor ? 3 : 0);
  return ((root % 12) + 12) % 12;
}

/**
 * Key shift in semitones (-6..=5) that moves a track in key `from` onto the
 * sounding key of the other deck (`to` plus its own shift `toShift`). Relative
 * keys match, so Am lines up with C. `null` when either key is unknown.
 */
export function keySyncShift(
  from: string | null | undefined,
  to: string | null | undefined,
  toShift: number,
): number | null {
  const a = keyPitchClass(from);
  const b = keyPitchClass(to);
  if (a === null || b === null) {
    return null;
  }
  const diff = (((b + toShift - a) % 12) + 12) % 12;
  return diff > 5 ? diff - 12 : diff;
}
