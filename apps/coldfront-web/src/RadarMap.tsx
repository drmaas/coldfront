import { useEffect, useMemo, useRef, useState } from "react";
import { iemTileUrl, webRadarFrames } from "../lib/domain";

interface RadarMapProps {
  latitude: number;
  longitude: number;
  basemap: "maptiler" | "none";
}

const TILE = 256;
const MIN_ZOOM = 5;
const MAX_ZOOM = 11;
const cache = new Map<string, Promise<HTMLImageElement | null>>();

export function RadarMap({ latitude, longitude, basemap }: RadarMapProps) {
  const cardRef = useRef<HTMLElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frames = useMemo(() => webRadarFrames(), []);
  const [index, setIndex] = useState(frames.length - 1);
  const [playing, setPlaying] = useState(true);
  const [zoom, setZoom] = useState(6);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [sizeTick, setSizeTick] = useState(0);
  const drag = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const zoomRef = useRef(zoom);
  const panRef = useRef(pan);
  const wheelRaf = useRef(0);
  const tileZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.floor(zoom)));
  const frame = frames[index] ?? frames[frames.length - 1];
  const withBasemap = basemap === "maptiler";
  zoomRef.current = zoom;
  panRef.current = pan;

  useEffect(() => {
    setReady(false);
    setFailed(false);
    setPan({ x: 0, y: 0 });
  }, [latitude, longitude, basemap]);

  useEffect(() => {
    setReady(false);
    setFailed(false);
  }, [tileZoom]);

  useEffect(() => {
    if (!playing) return undefined;
    const timer = window.setInterval(() => {
      setIndex((current) => (current + 1) % frames.length);
    }, 750);
    return () => window.clearInterval(timer);
  }, [playing, frames.length]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage || typeof ResizeObserver === "undefined") return undefined;
    let timer = 0;
    const observer = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        setSizeTick((tick) => tick + 1);
      }, 100);
    });
    observer.observe(stage);
    return () => {
      window.clearTimeout(timer);
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    const onChange = () => {
      const active = document.fullscreenElement === cardRef.current;
      setFullscreen(active);
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          setSizeTick((tick) => tick + 1);
        });
      });
    };
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return undefined;
    const flushWheel = () => {
      wheelRaf.current = 0;
      setZoom(zoomRef.current);
      setPan(panRef.current);
    };
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = stage.getBoundingClientRect();
      const originX = event.clientX - rect.left;
      const originY = event.clientY - rect.top;
      const pinchZoom = event.ctrlKey || event.metaKey;
      const mouseWheelZoom = !pinchZoom && event.deltaMode !== WheelEvent.DOM_DELTA_PIXEL && event.deltaX === 0;
      if (pinchZoom || mouseWheelZoom) {
        const pixels =
          event.deltaMode === WheelEvent.DOM_DELTA_LINE
            ? event.deltaY * 16
            : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
              ? event.deltaY * rect.height
              : event.deltaY;
        const next = zoomToward(
          zoomRef.current,
          zoomRef.current - pixels * 0.008,
          originX,
          originY,
          stage.clientWidth,
          stage.clientHeight,
          panRef.current,
        );
        zoomRef.current = next.zoom;
        panRef.current = next.pan;
        if (!wheelRaf.current) wheelRaf.current = window.requestAnimationFrame(flushWheel);
        return;
      }
      panRef.current = { x: panRef.current.x - event.deltaX, y: panRef.current.y - event.deltaY };
      if (!wheelRaf.current) wheelRaf.current = window.requestAnimationFrame(flushWheel);
    };
    stage.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      stage.removeEventListener("wheel", onWheel);
      if (wheelRaf.current) window.cancelAnimationFrame(wheelRaf.current);
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    let cancelled = false;
    void paint(canvas, latitude, longitude, zoom, pan, frame.layer, withBasemap, () => !cancelled).then((drewRadar) => {
      if (cancelled) return;
      if (drewRadar) {
        setReady(true);
        setFailed(false);
      } else {
        setFailed(true);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [latitude, longitude, zoom, pan, frame.layer, withBasemap, sizeTick]);

  async function toggleFullscreen() {
    const card = cardRef.current;
    if (!card) return;
    try {
      if (document.fullscreenElement === card) await document.exitFullscreen();
      else await card.requestFullscreen();
    } catch {
      // Browser may deny fullscreen without a user gesture or support.
    }
  }

  function zoomBy(delta: number) {
    const stage = stageRef.current;
    const target = delta > 0 ? Math.floor(zoomRef.current) + 1 : Math.ceil(zoomRef.current) - 1;
    if (!stage) {
      const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, target));
      zoomRef.current = next;
      setZoom(next);
      return;
    }
    const next = zoomToward(
      zoomRef.current,
      target,
      stage.clientWidth / 2,
      stage.clientHeight / 2,
      stage.clientWidth,
      stage.clientHeight,
      panRef.current,
    );
    zoomRef.current = next.zoom;
    panRef.current = next.pan;
    setZoom(next.zoom);
    setPan(next.pan);
  }

  return (
    <section ref={cardRef} className="radar-card" aria-label="Radar">
      <div ref={stageRef} className="radar-stage">
        <canvas
          ref={canvasRef}
          className="radar-overlay"
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
          <button type="button" onClick={() => zoomBy(1)} aria-label="Zoom in">
            +
          </button>
          <button type="button" onClick={() => zoomBy(-1)} aria-label="Zoom out">
            −
          </button>
          <button
            type="button"
            disabled={Math.hypot(pan.x, pan.y) < 0.5}
            onClick={(event) => {
              panRef.current = { x: 0, y: 0 };
              setPan({ x: 0, y: 0 });
              event.currentTarget.blur();
            }}
            aria-label="Recenter on ZIP"
            title="Recenter on ZIP"
          >
            ⊕
          </button>
          <button
            type="button"
            onClick={() => {
              void toggleFullscreen();
            }}
            aria-label={fullscreen ? "Exit fullscreen" : "Enter fullscreen"}
            title={fullscreen ? "Exit fullscreen" : "Enter fullscreen"}
          >
            {fullscreen ? "×" : "⛶"}
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
        {withBasemap
          ? "Basemap © MapTiler © OpenStreetMap contributors"
          : "Radar only. Add a MapTiler key to show streets."}
      </p>
    </section>
  );
}

function zoomToward(
  currentZoom: number,
  nextZoom: number,
  originX: number,
  originY: number,
  width: number,
  height: number,
  pan: { x: number; y: number },
): { zoom: number; pan: { x: number; y: number } } {
  const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, nextZoom));
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
  const view = readView(canvas, latitude, longitude, zoom, pan);
  if (!view) return false;
  const tiles: Array<Promise<Placed>> = [];
  forEachTile(view, (url, dx, dy, kind) => {
    tiles.push(place(url, dx, dy, kind));
  }, withBasemap, layer);
  const placed = await Promise.all(tiles);
  if (!active()) return false;
  const context = prepareCanvas(canvas, view);
  if (!context) return false;
  context.fillStyle = "#d5e3ec";
  context.fillRect(0, 0, view.viewWidth, view.viewHeight);
  for (const kind of ["base", "radar"] as const) {
    for (const tile of placed) {
      if (tile.kind === kind && tile.image) context.drawImage(tile.image, tile.dx, tile.dy, TILE, TILE);
    }
  }
  drawPin(context, (view.center.x - view.left) * TILE, (view.center.y - view.top) * TILE);
  return placed.some((tile) => tile.kind === "radar" && tile.image);
}

interface View {
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

function readView(
  canvas: HTMLCanvasElement,
  latitude: number,
  longitude: number,
  zoom: number,
  pan: { x: number; y: number },
): View | null {
  const cssWidth = canvas.clientWidth;
  const cssHeight = canvas.clientHeight;
  if (cssWidth === 0 || cssHeight === 0) return null;
  const ratio = Math.min(window.devicePixelRatio || 1, 2);
  const tileZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.floor(zoom)));
  const scale = 2 ** (zoom - tileZoom);
  const viewWidth = cssWidth / scale;
  const viewHeight = cssHeight / scale;
  const center = project(latitude, longitude, tileZoom);
  const left = center.x - viewWidth / 2 / TILE - pan.x / scale / TILE;
  const top = center.y - viewHeight / 2 / TILE - pan.y / scale / TILE;
  return { cssWidth, cssHeight, viewWidth, viewHeight, ratio, scale, tileZoom, center, left, top };
}

function prepareCanvas(canvas: HTMLCanvasElement, view: View): CanvasRenderingContext2D | null {
  const context = canvas.getContext("2d");
  if (!context) return null;
  canvas.width = Math.round(view.cssWidth * view.ratio);
  canvas.height = Math.round(view.cssHeight * view.ratio);
  context.setTransform(view.ratio * view.scale, 0, 0, view.ratio * view.scale, 0, 0);
  context.imageSmoothingEnabled = true;
  return context;
}

function forEachTile(
  view: View,
  emit: (url: string, dx: number, dy: number, kind: "base" | "radar") => void,
  withBasemap: boolean,
  layer: string,
): void {
  const x0 = Math.floor(view.left);
  const y0 = Math.floor(view.top);
  const x1 = Math.floor(view.left + view.viewWidth / TILE);
  const y1 = Math.floor(view.top + view.viewHeight / TILE);
  const limit = 2 ** view.tileZoom;
  for (let x = x0; x <= x1; x += 1) {
    for (let y = y0; y <= y1; y += 1) {
      if (x < 0 || y < 0 || x >= limit || y >= limit) continue;
      const dx = (x - view.left) * TILE;
      const dy = (y - view.top) * TILE;
      if (withBasemap) emit(`/api/map/${view.tileZoom}/${x}/${y}.png`, dx, dy, "base");
      emit(iemTileUrl(layer, view.tileZoom, x, y), dx, dy, "radar");
    }
  }
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
    if (!url.startsWith("/")) image.crossOrigin = "anonymous";
    image.onload = () => resolve(image);
    image.onerror = () => {
      cache.delete(url);
      resolve(null);
    };
    image.src = url;
  });
  cache.set(url, pending);
  return pending.then((image) => {
    if (!image) cache.delete(url);
    return image;
  });
}

function project(latitude: number, longitude: number, zoom: number): { x: number; y: number } {
  const tiles = 2 ** zoom;
  const x = ((longitude + 180) / 360) * tiles;
  const radians = (latitude * Math.PI) / 180;
  const y = ((1 - Math.log(Math.tan(radians) + 1 / Math.cos(radians)) / Math.PI) / 2) * tiles;
  return { x, y };
}
