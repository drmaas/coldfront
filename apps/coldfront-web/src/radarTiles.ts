/** SPDX-License-Identifier: GPL-3.0-only */

import { iemTileUrl } from "../lib/domain";
import { MAX_ZOOM, MIN_ZOOM, TILE, readView, type View } from "./radarCamera";
import { denoiseRadar } from "./radarRelief";

const imageCache = new Map<string, Promise<HTMLImageElement | null>>();
const resolvedImages = new Map<string, HTMLImageElement>();
const DENOISE_CAP = 512;
const denoiseCache = new Map<string, HTMLCanvasElement>();

export type TileKind = "base" | "radar";

export interface PlacedTile {
  source: CanvasImageSource | null;
  dx: number;
  dy: number;
  kind: TileKind;
  url: string;
}

export function loadImage(url: string): Promise<HTMLImageElement | null> {
  const cached = imageCache.get(url);
  if (cached) return cached;
  const pending = new Promise<HTMLImageElement | null>((resolve) => {
    const image = new Image();
    if (!url.startsWith("/")) image.crossOrigin = "anonymous";
    image.onload = () => resolve(image);
    image.onerror = () => {
      imageCache.delete(url);
      resolvedImages.delete(url);
      resolve(null);
    };
    image.src = url;
  });
  imageCache.set(url, pending);
  return pending.then((image) => {
    if (!image) {
      imageCache.delete(url);
      resolvedImages.delete(url);
      return null;
    }
    resolvedImages.set(url, image);
    return image;
  });
}

function touchDenoise(url: string, canvas: HTMLCanvasElement): void {
  denoiseCache.delete(url);
  denoiseCache.set(url, canvas);
  while (denoiseCache.size > DENOISE_CAP) {
    const oldest = denoiseCache.keys().next().value;
    if (oldest === undefined) break;
    denoiseCache.delete(oldest);
  }
}

/** Decode and denoise a 256px radar tile once; later settles blit the cached canvas. */
export async function loadDenoisedRadar(url: string): Promise<HTMLCanvasElement | null> {
  const hit = denoiseCache.get(url);
  if (hit) {
    touchDenoise(url, hit);
    return hit;
  }
  const image = await loadImage(url);
  if (!image) return null;
  const canvas = document.createElement("canvas");
  canvas.width = TILE;
  canvas.height = TILE;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;
  context.drawImage(image, 0, 0);
  const pixels = context.getImageData(0, 0, TILE, TILE);
  denoiseRadar(pixels);
  context.putImageData(pixels, 0, 0);
  touchDenoise(url, canvas);
  return canvas;
}

/** Sync peek — null when not decoded yet (paint holes, then refetch). */
export function peekDenoisedRadar(url: string): HTMLCanvasElement | null {
  const hit = denoiseCache.get(url);
  if (!hit) return null;
  touchDenoise(url, hit);
  return hit;
}

export function peekResolvedImage(url: string): HTMLImageElement | null {
  return resolvedImages.get(url) ?? null;
}

export function forEachTileUrl(
  view: View,
  withBasemap: boolean,
  layer: string,
  emit: (url: string, x: number, y: number, kind: TileKind) => void,
): void {
  const x0 = Math.floor(view.left);
  const y0 = Math.floor(view.top);
  const x1 = Math.floor(view.left + view.viewWidth / TILE);
  const y1 = Math.floor(view.top + view.viewHeight / TILE);
  const limit = 2 ** view.tileZoom;
  for (let x = x0; x <= x1; x += 1) {
    for (let y = y0; y <= y1; y += 1) {
      if (x < 0 || y < 0 || x >= limit || y >= limit) continue;
      if (withBasemap) emit(`/api/map/${view.tileZoom}/${x}/${y}.png`, x, y, "base");
      emit(iemTileUrl(layer, view.tileZoom, x, y), x, y, "radar");
    }
  }
}

export function tileScreenPos(view: View, x: number, y: number): { dx: number; dy: number } {
  return { dx: (x - view.left) * TILE, dy: (y - view.top) * TILE };
}

/** Place whatever is already decoded; missing tiles stay null (holes). */
export function placeCachedTiles(
  view: View,
  withBasemap: boolean,
  layer: string,
): PlacedTile[] {
  const placed: PlacedTile[] = [];
  forEachTileUrl(view, withBasemap, layer, (url, x, y, kind) => {
    const { dx, dy } = tileScreenPos(view, x, y);
    if (kind === "radar") {
      placed.push({ source: peekDenoisedRadar(url), dx, dy, kind, url });
      return;
    }
    placed.push({ source: peekResolvedImage(url), dx, dy, kind, url });
  });
  return placed;
}

/** Fetch every missing tile for this view; denoise radar into the LRU. */
export async function fetchViewTiles(
  view: View,
  withBasemap: boolean,
  layer: string,
): Promise<void> {
  const jobs: Array<Promise<unknown>> = [];
  forEachTileUrl(view, withBasemap, layer, (url, _x, _y, kind) => {
    if (kind === "radar") {
      if (!peekDenoisedRadar(url)) jobs.push(loadDenoisedRadar(url));
      return;
    }
    if (!peekResolvedImage(url)) jobs.push(loadImage(url));
  });
  await Promise.all(jobs);
}

/** Warm neighbor integer zooms and the next loop frame without blocking paint. */
export function prefetchAround(
  cssWidth: number,
  cssHeight: number,
  latitude: number,
  longitude: number,
  zoom: number,
  pan: { x: number; y: number },
  withBasemap: boolean,
  layer: string,
  nextLayer: string | null,
  devicePixelRatio: number,
): void {
  const floor = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.floor(zoom)));
  const warmZoom = (z: number, lyr: string) => {
    const nv = readView(cssWidth, cssHeight, latitude, longitude, z, pan, devicePixelRatio);
    if (!nv) return;
    forEachTileUrl(nv, withBasemap, lyr, (url, _x, _y, kind) => {
      if (kind === "radar") void loadDenoisedRadar(url);
      else void loadImage(url);
    });
  };
  if (floor > MIN_ZOOM) warmZoom(floor - 1, layer);
  if (floor < MAX_ZOOM) warmZoom(floor + 1, layer);
  if (nextLayer && nextLayer !== layer) warmZoom(zoom, nextLayer);
}
