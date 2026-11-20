import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadDroppedFile, loadTrackById } from "../../state/libraryActions";
import { loadPayload } from "./useDeckDrop";

vi.mock("../../state/libraryActions", () => ({
  loadDroppedFile: vi.fn(),
  loadTrackById: vi.fn(),
}));

describe("loadPayload", () => {
  beforeEach(() => vi.clearAllMocks());

  it("loads the first dragged track onto the deck", () => {
    expect(loadPayload(1, { kind: "tracks", ids: [7, 8] })).toBe(true);
    expect(loadTrackById).toHaveBeenCalledWith(1, 7);
  });

  it("ignores a null payload", () => {
    expect(loadPayload(0, null)).toBe(false);
    expect(loadTrackById).not.toHaveBeenCalled();
    expect(loadDroppedFile).not.toHaveBeenCalled();
  });
});
