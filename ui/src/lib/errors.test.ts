import { describe, expect, it } from "vitest";
import { describeCommandError, errorMessage, isCommandError } from "./errors";

describe("errors", () => {
  it("recognises typed command errors", () => {
    expect(isCommandError({ kind: "audio", message: "x" })).toBe(true);
    expect(isCommandError({ kind: "other", message: "x" })).toBe(false);
    expect(isCommandError("x")).toBe(false);
  });

  it("words each kind for the user", () => {
    expect(describeCommandError({ kind: "audio", message: "no device" })).toBe(
      "Audio problem: no device",
    );
    expect(describeCommandError({ kind: "media", message: "File is too long." })).toBe(
      "File is too long.",
    );
    expect(describeCommandError({ kind: "invalid", message: "Deck is empty." })).toBe(
      "Deck is empty.",
    );
    expect(describeCommandError({ kind: "internal", message: "Lock poisoned." })).toContain(
      "Details are in the log",
    );
  });

  it("handles strings, errors and unknown values", () => {
    expect(errorMessage("plain")).toBe("plain");
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage({ message: "m" })).toBe("m");
    expect(errorMessage(42)).toBe("Something went wrong.");
  });
});
