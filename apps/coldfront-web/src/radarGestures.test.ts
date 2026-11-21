import { describe, expect, it } from "vitest";
import {
  RADAR_STAGE_GESTURE,
  pinchTargetZoom,
  pointerCenter,
  pointerSpan,
  radarGestureConfig,
  type RadarViewMode,
} from "./radarGestures";

describe("radarGestureConfig", () => {
  const modes: RadarViewMode[] = ["flat", "relief", "fullscreen-flat", "fullscreen-relief"];

  it.each(modes)("%s enables pinch and pan with touch-action none", (mode) => {
    const config = radarGestureConfig(mode);
    expect(config).toEqual(RADAR_STAGE_GESTURE);
    expect(config.pinchZoomEnabled).toBe(true);
    expect(config.panEnabled).toBe(true);
    expect(config.touchAction).toBe("none");
  });
});

describe("pinchTargetZoom", () => {
  it("doubles span => +1 zoom step", () => {
    expect(pinchTargetZoom(6, 100, 200, 5, 11)).toBe(7);
  });

  it("halves span => -1 zoom step", () => {
    expect(pinchTargetZoom(6, 100, 50, 5, 11)).toBe(5);
  });

  it("clamps to min and max", () => {
    expect(pinchTargetZoom(5, 100, 400, 5, 11)).toBe(7);
    expect(pinchTargetZoom(5, 100, 10, 5, 11)).toBe(5);
    expect(pinchTargetZoom(11, 100, 400, 5, 11)).toBe(11);
  });

  it("ignores invalid span", () => {
    expect(pinchTargetZoom(6.5, 0, 120, 5, 11)).toBe(6.5);
  });
});

describe("pointerSpan and pointerCenter", () => {
  it("measures distance and midpoint", () => {
    const a = { x: 0, y: 0 };
    const b = { x: 3, y: 4 };
    expect(pointerSpan(a, b)).toBe(5);
    expect(pointerCenter(a, b)).toEqual({ x: 1.5, y: 2 });
  });
});