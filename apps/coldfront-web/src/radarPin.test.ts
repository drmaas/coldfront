/** SPDX-License-Identifier: GPL-3.0-only */

import { describe, expect, it } from "vitest";
import { FLAT_RADAR_STACK, pinCssAt, pinTileInView } from "./radarPin";
import { readView } from "./radarCamera";

describe("FLAT_RADAR_STACK", () => {
  it("keeps the location pin above radar tiles", () => {
    expect(FLAT_RADAR_STACK.indexOf("radar")).toBeLessThan(FLAT_RADAR_STACK.indexOf("pin"));
    expect(FLAT_RADAR_STACK.indexOf("basemap")).toBeLessThan(FLAT_RADAR_STACK.indexOf("radar"));
  });
});

describe("pinCssAt", () => {
  const latitude = 37.8044;
  const longitude = -122.2712;

  it("places the pin at the stage center when pan is zero", () => {
    const cssWidth = 320;
    const cssHeight = 280;
    const pin = pinCssAt(latitude, longitude, 6, { x: 0, y: 0 }, cssWidth, cssHeight);
    expect(pin.x).toBeCloseTo(cssWidth / 2, 5);
    expect(pin.y).toBeCloseTo(cssHeight / 2, 5);
  });

  it("moves with pan offset", () => {
    const cssWidth = 320;
    const cssHeight = 280;
    const pan = { x: 24, y: -12 };
    const pin = pinCssAt(latitude, longitude, 6, pan, cssWidth, cssHeight);
    expect(pin.x).toBeCloseTo(cssWidth / 2 + pan.x, 5);
    expect(pin.y).toBeCloseTo(cssHeight / 2 + pan.y, 5);
  });
});

describe("pinTileInView", () => {
  it("anchors at the geographic center of the current view", () => {
    const view = readView(320, 280, 37.8044, -122.2712, 6, { x: 0, y: 0 }, 1);
    expect(view).not.toBeNull();
    const pin = pinTileInView(view!);
    const expectedX = (view!.center.x - view!.left) * 256;
    const expectedY = (view!.center.y - view!.top) * 256;
    expect(pin.x).toBeCloseTo(expectedX, 4);
    expect(pin.y).toBeCloseTo(expectedY, 4);
  });
});