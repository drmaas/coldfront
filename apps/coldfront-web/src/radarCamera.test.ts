import { describe, expect, it } from "vitest";
import {
  pinMercator,
  readView,
  screenToWorld,
  snapshotTransform,
  worldToScreen,
  zoomToward,
} from "./radarCamera";

describe("snapshotTransform", () => {
  it("maps a settled screen point to the live camera screen point", () => {
    const latitude = 41.6;
    const longitude = -93.6;
    const width = 400;
    const height = 280;
    const settled = { zoom: 6.25, pan: { x: 40, y: -20 } };
    const live = { zoom: 7.1, pan: { x: -15, y: 30 } };
    const sx = 120;
    const sy = 90;

    const settledView = readView(width, height, latitude, longitude, settled.zoom, settled.pan, 1);
    expect(settledView).not.toBeNull();
    const world = screenToWorld(settledView!, sx, sy);
    const expected = worldToScreen(
      latitude,
      longitude,
      live.zoom,
      live.pan,
      width,
      height,
      world.mx,
      world.my,
    );

    const { scale, tx, ty } = snapshotTransform(settled, live, width, height);
    const mapped = { x: scale * sx + tx, y: scale * sy + ty };
    expect(mapped.x).toBeCloseTo(expected.x, 6);
    expect(mapped.y).toBeCloseTo(expected.y, 6);
  });

  it("is identity when cameras match", () => {
    const camera = { zoom: 6, pan: { x: 12, y: -8 } };
    const xform = snapshotTransform(camera, camera, 320, 240);
    expect(xform.scale).toBe(1);
    expect(xform.tx).toBeCloseTo(0, 10);
    expect(xform.ty).toBeCloseTo(0, 10);
  });
});

describe("zoomToward", () => {
  it("keeps the origin screen point fixed", () => {
    const latitude = 35.1;
    const longitude = -106.6;
    const width = 500;
    const height = 300;
    const originX = 200;
    const originY = 100;
    const before = { zoom: 6, pan: { x: 0, y: 0 } };
    const after = zoomToward(before.zoom, 7, originX, originY, width, height, before.pan);
    const pin = pinMercator(latitude, longitude);
    const world = {
      mx: pin.mx + (originX - width / 2 - before.pan.x) / (256 * 2 ** before.zoom),
      my: pin.my + (originY - height / 2 - before.pan.y) / (256 * 2 ** before.zoom),
    };
    const screen = worldToScreen(
      latitude,
      longitude,
      after.zoom,
      after.pan,
      width,
      height,
      world.mx,
      world.my,
    );
    expect(screen.x).toBeCloseTo(originX, 6);
    expect(screen.y).toBeCloseTo(originY, 6);
  });
});
