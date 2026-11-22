/** SPDX-License-Identifier: GPL-3.0-only */

import { pinMercator, worldToScreen, type View } from "./radarCamera";

/** Paint order on the flat radar canvas: basemap → radar → location pin. */
export const FLAT_RADAR_STACK = ["basemap", "radar", "pin"] as const;

export interface PinCssPosition {
  x: number;
  y: number;
}

/** ZIP pin in stage CSS pixels for a settled camera. */
export function pinCssAt(
  latitude: number,
  longitude: number,
  zoom: number,
  pan: { x: number; y: number },
  cssWidth: number,
  cssHeight: number,
): PinCssPosition {
  const { mx, my } = pinMercator(latitude, longitude);
  return worldToScreen(latitude, longitude, zoom, pan, cssWidth, cssHeight, mx, my);
}

/** Tile-space anchor used while painting the offscreen flat buffer (pin not baked into radar). */
export function pinTileInView(view: View): PinCssPosition {
  return {
    x: (view.center.x - view.left) * 256,
    y: (view.center.y - view.top) * 256,
  };
}

export function drawPin(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  mapScale = 1,
) {
  context.save();
  context.translate(x, y);
  context.scale(mapScale, mapScale);
  context.beginPath();
  context.moveTo(0, 0);
  context.bezierCurveTo(-11, -8, -9, -22, 0, -22);
  context.bezierCurveTo(9, -22, 11, -8, 0, 0);
  context.fillStyle = "#e11d48";
  context.strokeStyle = "#fff";
  context.lineWidth = 4;
  context.stroke();
  context.lineWidth = 2;
  context.fill();
  context.stroke();
  context.beginPath();
  context.arc(0, -13, 3.2, 0, Math.PI * 2);
  context.fillStyle = "#fff";
  context.fill();
  context.restore();
}
