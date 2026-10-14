import { describe, expect, it } from "vitest";
import { safeFileName, timeAgo, timeUntil } from "./format";

const now = Date.UTC(2026, 9, 14, 12, 0, 0);

describe("timeAgo", () => {
  it.each([
    [now - 10_000, "just now"],
    [now - 5 * 60_000, "5 min ago"],
    [now - 3 * 3_600_000, "3 h ago"],
    [now - 24 * 3_600_000, "1 day ago"],
    [now - 3 * 24 * 3_600_000, "3 days ago"],
  ])("%d → %s", (t, expected) => {
    expect(timeAgo(t, now)).toBe(expected);
  });
});

describe("timeUntil", () => {
  it("formats upcoming times", () => {
    expect(timeUntil(now - 1000, now)).toBe("now");
    expect(timeUntil(now + 4 * 60_000, now)).toBe("in 4 min");
    expect(timeUntil(now + 2 * 3_600_000, now)).toBe("in 2 h");
  });
});

describe("safeFileName", () => {
  it("strips characters that are invalid in file names", () => {
    expect(safeFileName('Living room: "main"')).toBe("Living room- -main-");
    expect(safeFileName("  ")).toBe("airdash");
  });
});
