import { describe, expect, it } from "vitest";
import { radarAutoplays } from "./radarAutoplay";

describe("radarAutoplays", () => {
  it("starts paused when the reader asked for reduced motion", () => {
    expect(radarAutoplays({ matches: true })).toBe(false);
  });

  it("keeps autoplay when motion is allowed", () => {
    expect(radarAutoplays({ matches: false })).toBe(true);
  });

  it("keeps autoplay where matchMedia is unavailable", () => {
    expect(radarAutoplays(null)).toBe(true);
  });
});
