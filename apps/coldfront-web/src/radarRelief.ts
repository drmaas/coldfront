/** One wet radar cell after downsampling. Coordinates are 0–1 across the view. */
export interface EchoSample {
  u: number;
  v: number;
  /** Relative column height from the n0q color band, 0–1. */
  height: number;
  r: number;
  g: number;
  b: number;
}

/**
 * Classic NWS / IEM n0q order: cool cyan → green → yellow → orange → red → purple.
 * Height rises with that order so green stays short and red/purple stand tall.
 */
const COLOR_BANDS: Array<{
  name: string;
  height: number;
  r: number;
  g: number;
  b: number;
  /** Inclusive hue range in degrees; may wrap past 360. */
  hueMin: number;
  hueMax: number;
}> = [
  { name: "cyan", height: 0.1, r: 0, g: 220, b: 235, hueMin: 165, hueMax: 200 },
  { name: "blue", height: 0.18, r: 0, g: 90, b: 245, hueMin: 200, hueMax: 265 },
  { name: "green", height: 0.3, r: 0, g: 200, b: 0, hueMin: 75, hueMax: 165 },
  { name: "yellow", height: 0.58, r: 245, g: 230, b: 0, hueMin: 45, hueMax: 75 },
  { name: "orange", height: 0.75, r: 255, g: 140, b: 0, hueMin: 18, hueMax: 45 },
  { name: "red", height: 0.92, r: 230, g: 0, b: 0, hueMin: 345, hueMax: 378 },
  { name: "red-low", height: 0.92, r: 230, g: 0, b: 0, hueMin: 0, hueMax: 18 },
  { name: "magenta", height: 0.97, r: 220, g: 0, b: 220, hueMin: 280, hueMax: 345 },
  { name: "purple", height: 1, r: 156, g: 85, b: 206, hueMin: 265, hueMax: 280 },
];

const MIN_ALPHA = 20;
const STAGE_RGB = { r: 213, g: 227, b: 236 };
const MAX_STAGE_DISTANCE = 28;
export const DEFAULT_COLS = 200;
export const DEFAULT_ROWS = 120;

/** Fine cells so the storm stays a hill, not a stack of blocks, at widget and fullscreen size. */
export function echoGrid(width: number, height: number): { cols: number; rows: number } {
  const cell = 2;
  return {
    cols: Math.max(120, Math.min(960, Math.round(width / cell))),
    rows: Math.max(80, Math.min(640, Math.round(height / cell))),
  };
}
/** Drop near-gray tile noise; real n0q cells stay chromatic. */
const MIN_CHROMA = 28;

/**
 * Downsample a radar-only ImageData into echo columns.
 * Transparent pixels and pale stage / basemap fill are dropped.
 * Column height follows green → yellow → red (then purple).
 */
export function sampleEchoes(
  image: ImageData,
  cols = DEFAULT_COLS,
  rows = DEFAULT_ROWS,
): EchoSample[] {
  const width = image.width;
  const imageHeight = image.height;
  if (width <= 0 || imageHeight <= 0 || cols <= 0 || rows <= 0) return [];

  const data = image.data;
  const echoes: EchoSample[] = [];
  const cellW = width / cols;
  const cellH = imageHeight / rows;

  for (let row = 0; row < rows; row += 1) {
    for (let col = 0; col < cols; col += 1) {
      const x0 = Math.floor(col * cellW);
      const y0 = Math.floor(row * cellH);
      const x1 = Math.min(width, Math.floor((col + 1) * cellW));
      const y1 = Math.min(imageHeight, Math.floor((row + 1) * cellH));
      let sumR = 0;
      let sumG = 0;
      let sumB = 0;
      let count = 0;

      for (let y = y0; y < y1; y += 1) {
        for (let x = x0; x < x1; x += 1) {
          const i = (y * width + x) * 4;
          const a = data[i + 3] ?? 0;
          if (a < MIN_ALPHA) continue;
          const r = data[i] ?? 0;
          const g = data[i + 1] ?? 0;
          const b = data[i + 2] ?? 0;
          if (isPaleStage(r, g, b)) continue;
          if (chroma(r, g, b) < MIN_CHROMA) continue;
          sumR += r;
          sumG += g;
          sumB += b;
          count += 1;
        }
      }

      if (count === 0) continue;
      const r = Math.round(sumR / count);
      const g = Math.round(sumG / count);
      const b = Math.round(sumB / count);
      if (chroma(r, g, b) < MIN_CHROMA) continue;
      const band = bandFor(r, g, b);
      if (!band) continue;

      echoes.push({
        u: (col + 0.5) / cols,
        v: (row + 0.5) / rows,
        height: band.height,
        r: band.r,
        g: band.g,
        b: band.b,
      });
    }
  }

  return echoes;
}

/**
 * Drop isolated n0q speckle before paint.
 * A wet pixel stays only when it touches a small cluster; lone returns go transparent.
 */
export function denoiseRadar(image: ImageData): void {
  const width = image.width;
  const height = image.height;
  const data = image.data;
  if (width <= 0 || height <= 0) return;
  const count = width * height;
  const echo = new Uint8Array(count);
  for (let i = 0; i < count; i += 1) {
    const o = i * 4;
    if ((data[o + 3] ?? 0) < MIN_ALPHA) continue;
    const r = data[o] ?? 0;
    const g = data[o + 1] ?? 0;
    const b = data[o + 2] ?? 0;
    if (isPaleStage(r, g, b) || chroma(r, g, b) < MIN_CHROMA) continue;
    if (!bandFor(r, g, b)) continue;
    echo[i] = 1;
  }

  const core = new Uint8Array(count);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = y * width + x;
      if (!echo[i]) continue;
      let neighbors = 0;
      for (let dy = -1; dy <= 1; dy += 1) {
        for (let dx = -1; dx <= 1; dx += 1) {
          if (dx === 0 && dy === 0) continue;
          const nx = x + dx;
          const ny = y + dy;
          if (nx < 0 || ny < 0 || nx >= width || ny >= height) continue;
          if (echo[ny * width + nx]) neighbors += 1;
        }
      }
      if (neighbors >= 4) core[i] = 1;
    }
  }

  const keep = new Uint8Array(core);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = y * width + x;
      if (!echo[i] || core[i]) continue;
      let touch = false;
      for (let dy = -1; dy <= 1 && !touch; dy += 1) {
        for (let dx = -1; dx <= 1; dx += 1) {
          if (dx === 0 && dy === 0) continue;
          const nx = x + dx;
          const ny = y + dy;
          if (nx < 0 || ny < 0 || nx >= width || ny >= height) continue;
          if (core[ny * width + nx]) {
            touch = true;
            break;
          }
        }
      }
      if (touch) keep[i] = 1;
    }
  }

  for (let i = 0; i < count; i += 1) {
    if (!echo[i] || keep[i]) continue;
    data[i * 4 + 3] = 0;
  }
}

/** Dense height/color grid for a smooth storm surface (not block columns). */
export interface EchoField {
  cols: number;
  rows: number;
  height: Float32Array;
  color: Float32Array;
}

/** Rasterize echoes onto the same grid sampleEchoes used. No extra spread. */
export function buildEchoField(
  echoes: EchoSample[],
  cols = DEFAULT_COLS,
  rows = DEFAULT_ROWS,
): EchoField {
  const height = new Float32Array(cols * rows);
  const color = new Float32Array(cols * rows * 3);
  for (const echo of echoes) {
    const col = Math.min(cols - 1, Math.max(0, Math.floor(echo.u * cols)));
    const row = Math.min(rows - 1, Math.max(0, Math.floor(echo.v * rows)));
    const i = row * cols + col;
    if (echo.height < (height[i] ?? 0)) continue;
    height[i] = echo.height;
    const o = i * 3;
    color[o] = echo.r / 255;
    color[o + 1] = echo.g / 255;
    color[o + 2] = echo.b / 255;
  }
  let smoothed: Float32Array = height;
  for (let pass = 0; pass < 4; pass += 1) smoothed = softenWet(smoothed, cols, rows);
  return { cols, rows, height: smoothed, color: softenColors(color, smoothed, cols, rows) };
}

/** Blend band color across wet neighbors so cells do not read as separate tiles. */
function softenColors(
  source: Float32Array,
  heights: Float32Array,
  cols: number,
  rows: number,
): Float32Array {
  const out = new Float32Array(source.length);
  for (let row = 0; row < rows; row += 1) {
    for (let col = 0; col < cols; col += 1) {
      const i = row * cols + col;
      if ((heights[i] ?? 0) <= 0) continue;
      const o = i * 3;
      let r = (source[o] ?? 0) * 4;
      let g = (source[o + 1] ?? 0) * 4;
      let b = (source[o + 2] ?? 0) * 4;
      let weight = 4;
      for (let dy = -1; dy <= 1; dy += 1) {
        for (let dx = -1; dx <= 1; dx += 1) {
          if (dx === 0 && dy === 0) continue;
          const x = col + dx;
          const y = row + dy;
          if (x < 0 || y < 0 || x >= cols || y >= rows) continue;
          if ((heights[y * cols + x] ?? 0) <= 0) continue;
          const no = (y * cols + x) * 3;
          r += source[no] ?? 0;
          g += source[no + 1] ?? 0;
          b += source[no + 2] ?? 0;
          weight += 1;
        }
      }
      out[o] = r / weight;
      out[o + 1] = g / weight;
      out[o + 2] = b / weight;
    }
  }
  return out;
}

/** Soften peaks among wet cells only. Empty cells stay empty, so the footprint does not grow. */
function softenWet(source: Float32Array, cols: number, rows: number): Float32Array {
  const out = new Float32Array(source.length);
  for (let row = 0; row < rows; row += 1) {
    for (let col = 0; col < cols; col += 1) {
      const i = row * cols + col;
      const center = source[i] ?? 0;
      if (center <= 0) continue;
      let sum = center * 4;
      let weight = 4;
      for (let dy = -1; dy <= 1; dy += 1) {
        for (let dx = -1; dx <= 1; dx += 1) {
          if (dx === 0 && dy === 0) continue;
          const x = col + dx;
          const y = row + dy;
          if (x < 0 || y < 0 || x >= cols || y >= rows) continue;
          const neighbor = source[y * cols + x] ?? 0;
          if (neighbor <= 0) continue;
          sum += neighbor;
          weight += 1;
        }
      }
      out[i] = sum / weight;
    }
  }
  return out;
}

function chroma(r: number, g: number, b: number): number {
  return Math.max(r, g, b) - Math.min(r, g, b);
}

function isPaleStage(r: number, g: number, b: number): boolean {
  const dr = r - STAGE_RGB.r;
  const dg = g - STAGE_RGB.g;
  const db = b - STAGE_RGB.b;
  return Math.sqrt(dr * dr + dg * dg + db * db) <= MAX_STAGE_DISTANCE;
}

/** Hue in degrees [0, 360). */
export function hueDegrees(r: number, g: number, b: number): number {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  if (delta < 1) return 0;
  let hue = 0;
  if (max === r) hue = 60 * (((g - b) / delta) % 6);
  else if (max === g) hue = 60 * ((b - r) / delta + 2);
  else hue = 60 * ((r - g) / delta + 4);
  if (hue < 0) hue += 360;
  return hue;
}

function bandFor(
  r: number,
  g: number,
  b: number,
): { height: number; r: number; g: number; b: number } | null {
  const hue = hueDegrees(r, g, b);
  for (const band of COLOR_BANDS) {
    if (hue >= band.hueMin && hue < band.hueMax) {
      return { height: band.height, r: band.r, g: band.g, b: band.b };
    }
  }
  // Wrapped red band uses hueMax > 360.
  for (const band of COLOR_BANDS) {
    if (band.hueMax > 360 && (hue >= band.hueMin || hue < band.hueMax - 360)) {
      return { height: band.height, r: band.r, g: band.g, b: band.b };
    }
  }
  return null;
}

/** Band height for a known RGB sample, or null when the color is not wet n0q. */
export function bandHeight(r: number, g: number, b: number): number | null {
  const band = bandFor(r, g, b);
  return band ? band.height : null;
}

/** Match a view frustum to the canvas aspect. Cover fills the stage; contain letterboxes. */
export function fitOrthoHalfExtents(
  halfW: number,
  halfH: number,
  aspect: number,
  mode: "cover" | "contain",
): { halfW: number; halfH: number } {
  if (halfW <= 0 || halfH <= 0 || aspect <= 0) return { halfW, halfH };
  if (mode === "cover") {
    if (halfW / halfH > aspect) return { halfW: halfH * aspect, halfH };
    return { halfW, halfH: halfW / aspect };
  }
  if (halfW / halfH > aspect) return { halfW, halfH: halfW / aspect };
  return { halfW: halfH * aspect, halfH };
}

/** True when the browser can create a WebGL2 context. */
export function webgl2Available(): boolean {
  if (typeof document === "undefined") return false;
  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: true });
    return Boolean(gl);
  } catch {
    return false;
  }
}
