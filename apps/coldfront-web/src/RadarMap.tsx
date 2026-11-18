import { useEffect, useMemo, useRef, useState } from "react";
import { iemTileUrl, webRadarFrames } from "../lib/domain";

interface RadarMapProps {
  latitude: number;
  longitude: number;
  basemap: "maptiler" | "none";
}

const TILE = 256;
const cache = new Map<string, Promise<HTMLImageElement | null>>();

export function RadarMap({ latitude, longitude, basemap }: RadarMapProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frames = useMemo(() => webRadarFrames(), []);
  const [index, setIndex] = useState(frames.length - 1);
  const [playing, setPlaying] = useState(true);
  const [zoom, setZoom] = useState(6);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const drag = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const frame = frames[index] ?? frames[frames.length - 1];

  useEffect(() => {
    setReady(false);
    setFailed(false);
    setPan({ x: 0, y: 0 });
  }, [latitude, longitude, zoom, basemap]);

  useEffect(() => {
    if (!playing) return undefined;
    const timer = window.setInterval(() => {
      setIndex((current) => (current + 1) % frames.length);
    }, 750);
    return () => window.clearInterval(timer);
  }, [playing, frames.length]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    let cancelled = false;
    void paint(canvas, latitude, longitude, zoom, pan, frame.layer, basemap === "maptiler", () => !cancelled).then(
      (drewRadar) => {
        if (cancelled) return;
        if (drewRadar) {
          setReady(true);
          setFailed(false);
        } else {
          setFailed(true);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [latitude, longitude, zoom, pan, frame.layer, basemap]);

  return (
    <section className="radar-card" aria-label="Radar">
      <div className="radar-stage">
        <canvas
          ref={canvasRef}
          onPointerDown={(event) => {
            drag.current = { x: event.clientX, y: event.clientY, panX: pan.x, panY: pan.y };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            const start = drag.current;
            if (!start) return;
            setPan({ x: start.panX + event.clientX - start.x, y: start.panY + event.clientY - start.y });
          }}
          onPointerUp={() => {
            drag.current = null;
          }}
        />
        {!ready && <div className="radar-placeholder">{failed ? "Radar tiles unavailable" : "Loading radar…"}</div>}
        <div className="zoom-controls">
          <button type="button" onClick={() => setZoom((value) => Math.min(11, value + 1))} aria-label="Zoom in">
            +
          </button>
          <button type="button" onClick={() => setZoom((value) => Math.max(5, value - 1))} aria-label="Zoom out">
            −
          </button>
        </div>
      </div>
      <div className="radar-controls">
        <span>55 min ago</span>
        <input
          type="range"
          min={0}
          max={frames.length - 1}
          value={index}
          aria-label="Radar time"
          onChange={(event) => {
            setPlaying(false);
            setIndex(Number(event.target.value));
          }}
        />
        <span>Now</span>
        <button type="button" className="pause" onClick={() => setPlaying((value) => !value)}>
          {playing ? "Pause" : "Play"}
        </button>
      </div>
      <p className="radar-caption">
        {frame.label} · Base reflectivity, NOAA NEXRAD
      </p>
      <p className="radar-note">
        {basemap === "maptiler"
          ? "Basemap © MapTiler © OpenStreetMap contributors"
          : "Radar only. Add a MapTiler key to show streets."}
      </p>
    </section>
  );
}

async function paint(
  canvas: HTMLCanvasElement,
  latitude: number,
  longitude: number,
  zoom: number,
  pan: { x: number; y: number },
  layer: string,
  withBasemap: boolean,
  active: () => boolean,
): Promise<boolean> {
  const cssWidth = canvas.clientWidth;
  const cssHeight = canvas.clientHeight;
  if (cssWidth === 0 || cssHeight === 0) return false;
  const ratio = window.devicePixelRatio || 1;
  const center = project(latitude, longitude, zoom);
  const left = center.x - cssWidth / 2 / TILE - pan.x / TILE;
  const top = center.y - cssHeight / 2 / TILE - pan.y / TILE;
  const x0 = Math.floor(left);
  const y0 = Math.floor(top);
  const x1 = Math.floor(left + cssWidth / TILE);
  const y1 = Math.floor(top + cssHeight / TILE);
  const limit = 2 ** zoom;
  const tiles: Array<Promise<Placed>> = [];
  for (let x = x0; x <= x1; x += 1) {
    for (let y = y0; y <= y1; y += 1) {
      if (x < 0 || y < 0 || x >= limit || y >= limit) continue;
      const dx = (x - left) * TILE;
      const dy = (y - top) * TILE;
      if (withBasemap) tiles.push(place(`/api/map/${zoom}/${x}/${y}.png`, dx, dy, "base"));
      tiles.push(place(iemTileUrl(layer, zoom, x, y), dx, dy, "radar"));
    }
  }
  const placed = await Promise.all(tiles);
  if (!active()) return false;
  const context = canvas.getContext("2d");
  if (!context) return false;
  canvas.width = Math.round(cssWidth * ratio);
  canvas.height = Math.round(cssHeight * ratio);
  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.clearRect(0, 0, cssWidth, cssHeight);
  context.fillStyle = "#d5e3ec";
  context.fillRect(0, 0, cssWidth, cssHeight);
  for (const kind of ["base", "radar"] as const) {
    for (const tile of placed) {
      if (tile.kind === kind && tile.image) context.drawImage(tile.image, tile.dx, tile.dy, TILE, TILE);
    }
  }
  drawPin(context, (center.x - left) * TILE, (center.y - top) * TILE);
  return placed.some((tile) => tile.kind === "radar" && tile.image);
}

function drawPin(context: CanvasRenderingContext2D, x: number, y: number) {
  context.save();
  context.translate(x, y);
  context.beginPath();
  context.moveTo(0, 0);
  context.bezierCurveTo(-11, -8, -9, -22, 0, -22);
  context.bezierCurveTo(9, -22, 11, -8, 0, 0);
  context.fillStyle = "#e11d48";
  context.strokeStyle = "#fff";
  context.lineWidth = 2;
  context.fill();
  context.stroke();
  context.beginPath();
  context.arc(0, -13, 3.2, 0, Math.PI * 2);
  context.fillStyle = "#fff";
  context.fill();
  context.restore();
}

interface Placed {
  image: HTMLImageElement | null;
  dx: number;
  dy: number;
  kind: "base" | "radar";
}

function place(url: string, dx: number, dy: number, kind: Placed["kind"]): Promise<Placed> {
  return loadImage(url).then((image) => ({ image, dx, dy, kind }));
}

function loadImage(url: string): Promise<HTMLImageElement | null> {
  const cached = cache.get(url);
  if (cached) return cached;
  const pending = new Promise<HTMLImageElement | null>((resolve) => {
    const image = new Image();
    image.crossOrigin = "anonymous";
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = url;
  });
  cache.set(url, pending);
  return pending;
}

function project(latitude: number, longitude: number, zoom: number): { x: number; y: number } {
  const tiles = 2 ** zoom;
  const x = ((longitude + 180) / 360) * tiles;
  const radians = (latitude * Math.PI) / 180;
  const y = ((1 - Math.log(Math.tan(radians) + 1 / Math.cos(radians)) / Math.PI) / 2) * tiles;
  return { x, y };
}
