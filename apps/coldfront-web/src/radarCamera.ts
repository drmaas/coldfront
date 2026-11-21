/** SPDX-License-Identifier: GPL-3.0-only */

export const TILE = 256;
export const MIN_ZOOM = 5;
export const MAX_ZOOM = 11;

export interface Camera {
  zoom: number;
  pan: { x: number; y: number };
}

export interface SnapshotTransform {
  /** Multiply settled CSS pixels by this to reach live CSS pixels. */
  scale: number;
  /** CSS-pixel translation after scale: sx_live = scale * sx_settled + tx. */
  tx: number;
  ty: number;
}

/** Same numbers as SnapshotTransform; shared with the relief board uniforms. */
export type BoardTransform = SnapshotTransform;

/**
 * Affine map from a settled radar bitmap to the live camera.
 * Uses continuous mercator: sx = (mx - pin) * TILE * 2^zoom + W/2 + pan.x.
 */
export function snapshotTransform(
  settled: Camera,
  live: Camera,
  width: number,
  height: number,
): SnapshotTransform {
  const scale = 2 ** (live.zoom - settled.zoom);
  return {
    scale,
    tx: width / 2 + live.pan.x - scale * (width / 2 + settled.pan.x),
    ty: height / 2 + live.pan.y - scale * (height / 2 + settled.pan.y),
  };
}

/** Zoom toward a screen origin, keeping that point fixed in the view. */
export function zoomToward(
  currentZoom: number,
  nextZoom: number,
  originX: number,
  originY: number,
  width: number,
  height: number,
  pan: { x: number; y: number },
  minZoom = MIN_ZOOM,
  maxZoom = MAX_ZOOM,
): Camera {
  const zoom = Math.min(maxZoom, Math.max(minZoom, nextZoom));
  if (zoom === currentZoom) return { zoom, pan };
  const factor = 2 ** (zoom - currentZoom);
  return {
    zoom,
    pan: {
      x: factor * (width / 2 + pan.x - originX) - width / 2 + originX,
      y: factor * (height / 2 + pan.y - originY) - height / 2 + originY,
    },
  };
}

/** Mercator tile coordinates at an integer zoom. */
export function project(latitude: number, longitude: number, zoom: number): { x: number; y: number } {
  const tiles = 2 ** zoom;
  const x = ((longitude + 180) / 360) * tiles;
  const radians = (latitude * Math.PI) / 180;
  const y = ((1 - Math.log(Math.tan(radians) + 1 / Math.cos(radians)) / Math.PI) / 2) * tiles;
  return { x, y };
}

export interface View {
  cssWidth: number;
  cssHeight: number;
  viewWidth: number;
  viewHeight: number;
  ratio: number;
  scale: number;
  tileZoom: number;
  center: { x: number; y: number };
  left: number;
  top: number;
}

export function readView(
  cssWidth: number,
  cssHeight: number,
  latitude: number,
  longitude: number,
  zoom: number,
  pan: { x: number; y: number },
  devicePixelRatio = 1,
): View | null {
  if (cssWidth === 0 || cssHeight === 0) return null;
  const ratio = Math.min(devicePixelRatio || 1, 2);
  const tileZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.floor(zoom)));
  const scale = 2 ** (zoom - tileZoom);
  const viewWidth = cssWidth / scale;
  const viewHeight = cssHeight / scale;
  const center = project(latitude, longitude, tileZoom);
  const left = center.x - viewWidth / 2 / TILE - pan.x / scale / TILE;
  const top = center.y - viewHeight / 2 / TILE - pan.y / scale / TILE;
  return { cssWidth, cssHeight, viewWidth, viewHeight, ratio, scale, tileZoom, center, left, top };
}

/**
 * Map a settled CSS pixel through snapshotTransform and compare to readView geography.
 * Returns the live screen point for the same mercator location.
 */
export function screenToWorld(
  view: View,
  sx: number,
  sy: number,
): { mx: number; my: number } {
  const tileZoom = view.tileZoom;
  const wx = view.left + sx / view.scale / TILE;
  const wy = view.top + sy / view.scale / TILE;
  return { mx: wx / 2 ** tileZoom, my: wy / 2 ** tileZoom };
}

export function pinMercator(latitude: number, longitude: number): { mx: number; my: number } {
  const mx = (longitude + 180) / 360;
  const radians = (latitude * Math.PI) / 180;
  const my = (1 - Math.log(Math.tan(radians) + 1 / Math.cos(radians)) / Math.PI) / 2;
  return { mx, my };
}

export function worldToScreen(
  latitude: number,
  longitude: number,
  zoom: number,
  pan: { x: number; y: number },
  width: number,
  height: number,
  mx: number,
  my: number,
): { x: number; y: number } {
  const pin = pinMercator(latitude, longitude);
  return {
    x: (mx - pin.mx) * TILE * 2 ** zoom + width / 2 + pan.x,
    y: (my - pin.my) * TILE * 2 ** zoom + height / 2 + pan.y,
  };
}
