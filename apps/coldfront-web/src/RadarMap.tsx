import { useEffect, useMemo, useRef, useState } from "react";
import { webRadarFrames } from "../lib/domain";
import {
  MAX_ZOOM,
  MIN_ZOOM,
  TILE,
  readView,
  snapshotTransform,
  zoomToward,
  type BoardTransform,
  type Camera,
  type View,
} from "./radarCamera";
import { RadarRelief } from "./RadarRelief";
import {
  pinchTargetZoom,
  pointerCenter,
  pointerSpan,
  RADAR_STAGE_GESTURE,
  touchPoints,
} from "./radarGestures";
import {
  echoGrid,
  sampleEchoes,
  webgl2Available,
  type EchoSample,
  type ReliefScene,
} from "./radarRelief";
import {
  fetchViewTiles,
  placeCachedTiles,
  prefetchAround,
  type PlacedTile,
} from "./radarTiles";

interface RadarMapProps {
  latitude: number;
  longitude: number;
  basemap: "maptiler" | "none";
}

const SETTLE_MS = 50;

type RadarMode = "flat" | "relief";

interface Snapshot {
  canvas: HTMLCanvasElement;
  camera: Camera;
  cssWidth: number;
  cssHeight: number;
}

export function RadarMap({ latitude, longitude, basemap }: RadarMapProps) {
  const cardRef = useRef<HTMLElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frames = useMemo(() => webRadarFrames(), []);
  const glOk = useMemo(() => webgl2Available(), []);
  const [index, setIndex] = useState(frames.length - 1);
  const [playing, setPlaying] = useState(true);
  const [zoom, setZoom] = useState(6);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [sizeTick, setSizeTick] = useState(0);
  const [mode, setMode] = useState<RadarMode>("flat");
  const [echoes, setEchoes] = useState<EchoSample[]>([]);
  const [echoCols, setEchoCols] = useState(200);
  const [echoRows, setEchoRows] = useState(120);
  const [ground, setGround] = useState<ImageData | null>(null);
  const drag = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const zoomRef = useRef(zoom);
  const panRef = useRef(pan);
  const modeRef = useRef(mode);
  const wheelRaf = useRef(0);
  const blitRaf = useRef(0);
  const settleTimer = useRef(0);
  const settleGen = useRef(0);
  const gesturingRef = useRef(false);
  const lastGestureAt = useRef(0);
  const snapshotRef = useRef<Snapshot | null>(null);
  const boardTransformRef = useRef<BoardTransform>({ scale: 1, tx: 0, ty: 0 });
  const flatBufferRef = useRef<HTMLCanvasElement | null>(null);
  const groundBufferRef = useRef<HTMLCanvasElement | null>(null);
  const reliefSceneRef = useRef<ReliefScene | null>(null);
  const reliefRecenter = useRef<(() => void) | null>(null);
  const frame = frames[index] ?? frames[frames.length - 1];
  const nextFrame = frames[(index + 1) % frames.length] ?? frame;
  const withBasemap = basemap === "maptiler";
  const relief = mode === "relief" && glOk;
  const settleArgsRef = useRef({
    latitude,
    longitude,
    withBasemap,
    layer: frame.layer,
    nextLayer: nextFrame.layer,
    glOk,
  });
  const apiRef = useRef<{
    scheduleBlit: () => void;
    scheduleSettle: (ms?: number) => void;
    onCameraGesture: () => void;
  }>({
    scheduleBlit: () => {},
    scheduleSettle: () => {},
    onCameraGesture: () => {},
  });
  // zoomRef/panRef are the live camera during gestures. Do not write them from
  // React state here — a stale setState would yank the map back mid-pan.
  modeRef.current = mode;
  settleArgsRef.current = {
    latitude,
    longitude,
    withBasemap,
    layer: frame.layer,
    nextLayer: nextFrame.layer,
    glOk,
  };

  const syncReactCamera = () => {
    setZoom(zoomRef.current);
    setPan(panRef.current);
  };

  const applyBoardTransform = () => {
    const snap = snapshotRef.current;
    const stage = stageRef.current;
    if (!snap || !stage) {
      boardTransformRef.current = { scale: 1, tx: 0, ty: 0 };
      return;
    }
    boardTransformRef.current = snapshotTransform(
      snap.camera,
      { zoom: zoomRef.current, pan: panRef.current },
      stage.clientWidth,
      stage.clientHeight,
    );
  };

  const cameraUnchanged = (camera: Camera) =>
    camera.zoom === zoomRef.current &&
    camera.pan.x === panRef.current.x &&
    camera.pan.y === panRef.current.y;

  const blitGesture = () => {
    blitRaf.current = 0;
    applyBoardTransform();
    if (modeRef.current === "relief" && glOk) return;
    const canvas = canvasRef.current;
    const snap = snapshotRef.current;
    if (!canvas || !snap) return;
    const cssWidth = canvas.clientWidth;
    const cssHeight = canvas.clientHeight;
    if (cssWidth === 0 || cssHeight === 0) return;
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.round(cssWidth * ratio);
    const height = Math.round(cssHeight * ratio);
    const context = canvas.getContext("2d");
    if (!context) return;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    const { scale, tx, ty } = boardTransformRef.current;
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.fillStyle = "#d5e3ec";
    context.fillRect(0, 0, width, height);
    // Scale the device-pixel snapshot in place — no CSS downscale then DPR upscale.
    context.setTransform(scale, 0, 0, scale, tx * ratio, ty * ratio);
    context.imageSmoothingEnabled = Math.abs(scale - 1) > 0.001;
    context.drawImage(snap.canvas, 0, 0);
  };

  const scheduleBlit = () => {
    if (!blitRaf.current) blitRaf.current = window.requestAnimationFrame(blitGesture);
  };

  const ensureBuffer = (
    slot: { current: HTMLCanvasElement | null },
    width: number,
    height: number,
  ): HTMLCanvasElement => {
    let canvas = slot.current;
    if (!canvas) {
      canvas = document.createElement("canvas");
      slot.current = canvas;
    }
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    return canvas;
  };

  const paintSettled = (
    canvas: HTMLCanvasElement,
    view: View,
    placed: PlacedTile[],
    wantRelief: boolean,
  ): { drewRadar: boolean; ground: ImageData | null; echoes: EchoSample[]; cols: number; rows: number } => {
    const empty = { drewRadar: false, ground: null as ImageData | null, echoes: [] as EchoSample[], cols: 200, rows: 120 };
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) return empty;
    const width = Math.round(view.cssWidth * view.ratio);
    const height = Math.round(view.cssHeight * view.ratio);
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    context.setTransform(view.ratio * view.scale, 0, 0, view.ratio * view.scale, 0, 0);
    // Integer zoom: nearest. Fractional zoom: smooth the scaled tiles.
    context.imageSmoothingEnabled = Math.abs(view.scale - 1) > 0.001;
    context.fillStyle = "#d5e3ec";
    context.fillRect(0, 0, view.viewWidth, view.viewHeight);
    for (const tile of placed) {
      if (tile.kind === "base" && tile.source) {
        context.drawImage(tile.source, tile.dx, tile.dy, TILE, TILE);
      }
    }
    const drewRadar = placed.some((tile) => tile.kind === "radar" && tile.source);
    if (!wantRelief) {
      // Direct device-res tile blit — no CSS-sized mosaic stretch.
      for (const tile of placed) {
        if (tile.kind === "radar" && tile.source) {
          context.drawImage(tile.source, tile.dx, tile.dy, TILE, TILE);
        }
      }
      drawPin(context, (view.center.x - view.left) * TILE, (view.center.y - view.top) * TILE);
      return { drewRadar, ground: null, echoes: [], cols: 200, rows: 120 };
    }

    // Ground = basemap (or fill) + pin only. Radar lives in the heightfield.
    drawPin(context, (view.center.x - view.left) * TILE, (view.center.y - view.top) * TILE);
    context.setTransform(1, 0, 0, 1, 0, 0);
    const ground = context.getImageData(0, 0, canvas.width, canvas.height);
    const radar = mosaicRadar(placed, view);
    const grid = radar ? echoGrid(radar.image.width, radar.image.height) : { cols: 200, rows: 120 };
    const echoes = radar ? sampleEchoes(radar.image, grid.cols, grid.rows) : [];
    return { drewRadar, ground, echoes, cols: grid.cols, rows: grid.rows };
  };

  const presentFlat = (view: View, camera: Camera, source: HTMLCanvasElement) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const width = source.width;
    const height = source.height;
    const context = canvas.getContext("2d");
    if (!context) return;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.drawImage(source, 0, 0);
    const snapCanvas = document.createElement("canvas");
    snapCanvas.width = width;
    snapCanvas.height = height;
    const snapCtx = snapCanvas.getContext("2d");
    if (!snapCtx) return;
    snapCtx.drawImage(source, 0, 0);
    snapshotRef.current = {
      canvas: snapCanvas,
      camera: { zoom: camera.zoom, pan: { ...camera.pan } },
      cssWidth: view.cssWidth,
      cssHeight: view.cssHeight,
    };
    boardTransformRef.current = { scale: 1, tx: 0, ty: 0 };
  };

  const runSettle = async () => {
    const stage = stageRef.current;
    if (!stage) return;
    // Keep transforming the last snapshot while a finger/wheel is active.
    if (gesturingRef.current || performance.now() - lastGestureAt.current < SETTLE_MS) {
      scheduleSettle();
      return;
    }
    const args = settleArgsRef.current;
    const gen = ++settleGen.current;
    const cssWidth = stage.clientWidth;
    const cssHeight = stage.clientHeight;
    const camera: Camera = { zoom: zoomRef.current, pan: { ...panRef.current } };
    const view = readView(
      cssWidth,
      cssHeight,
      args.latitude,
      args.longitude,
      camera.zoom,
      camera.pan,
      window.devicePixelRatio || 1,
    );
    if (!view) return;
    const wantRelief = modeRef.current === "relief" && args.glOk;
    const hadSnapshot = snapshotRef.current !== null;

    let placed = placeCachedTiles(view, args.withBasemap, args.layer);
    const missing = placed.some((tile) => tile.source === null);
    // Never wipe a good frame with pale holes — fetch first, then paint once.
    if (missing) {
      await fetchViewTiles(view, args.withBasemap, args.layer);
      if (gen !== settleGen.current) return;
      if (!cameraUnchanged(camera)) {
        scheduleSettle(0);
        return;
      }
      placed = placeCachedTiles(view, args.withBasemap, args.layer);
    }

    const width = Math.round(view.cssWidth * view.ratio);
    const height = Math.round(view.cssHeight * view.ratio);
    const flatTarget = ensureBuffer(flatBufferRef, width, height);
    const result = wantRelief
      ? paintSettled(ensureBuffer(groundBufferRef, width, height), view, placed, true)
      : paintSettled(flatTarget, view, placed, false);
    if (wantRelief) paintSettled(flatTarget, view, placed, false);
    if (gen !== settleGen.current) return;
    if (!cameraUnchanged(camera)) {
      scheduleSettle(0);
      return;
    }

    if (!result.drewRadar && hadSnapshot) {
      // Keep the previous snapshot on screen; do not paint pale holes over it.
      prefetchAround(
        cssWidth,
        cssHeight,
        args.latitude,
        args.longitude,
        camera.zoom,
        camera.pan,
        args.withBasemap,
        args.layer,
        args.nextLayer,
        window.devicePixelRatio || 1,
      );
      return;
    }

    // Relief: publish the new board before identity transform so WebGL never
    // draws the previous ground at the new camera for a frame.
    if (wantRelief && result.ground) {
      reliefSceneRef.current = {
        ground: result.ground,
        echoes: result.echoes,
        cols: result.cols,
        rows: result.rows,
      };
      setEchoes(result.echoes);
      setEchoCols(result.cols);
      setEchoRows(result.rows);
      setGround(result.ground);
    }
    presentFlat(view, camera, flatTarget);

    if (result.drewRadar) {
      setReady(true);
      setFailed(false);
    } else if (!hadSnapshot) {
      setFailed(true);
      if (wantRelief) {
        setEchoes([]);
        setGround(null);
      }
    }

    prefetchAround(
      cssWidth,
      cssHeight,
      args.latitude,
      args.longitude,
      camera.zoom,
      camera.pan,
      args.withBasemap,
      args.layer,
      args.nextLayer,
      window.devicePixelRatio || 1,
    );
  };

  const scheduleSettle = (ms = SETTLE_MS) => {
    window.clearTimeout(settleTimer.current);
    settleTimer.current = window.setTimeout(() => {
      const quiet = performance.now() - lastGestureAt.current;
      if (quiet < ms) {
        scheduleSettle(ms);
        return;
      }
      gesturingRef.current = false;
      void runSettle();
    }, ms);
  };

  const onCameraGesture = () => {
    gesturingRef.current = true;
    lastGestureAt.current = performance.now();
    scheduleBlit();
    if (!wheelRaf.current) {
      wheelRaf.current = window.requestAnimationFrame(() => {
        wheelRaf.current = 0;
        syncReactCamera();
      });
    }
    scheduleSettle();
  };

  apiRef.current = { scheduleBlit, scheduleSettle, onCameraGesture };

  useEffect(() => {
    setReady(false);
    setFailed(false);
    panRef.current = { x: 0, y: 0 };
    setPan({ x: 0, y: 0 });
    setEchoes([]);
    setGround(null);
    reliefSceneRef.current = null;
    snapshotRef.current = null;
    boardTransformRef.current = { scale: 1, tx: 0, ty: 0 };
  }, [latitude, longitude, basemap]);

  useEffect(() => {
    if (!glOk && mode === "relief") setMode("flat");
  }, [glOk, mode]);

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

  // Settle on place, frame, size, mode — not on every pan/zoom tick.
  useEffect(() => {
    window.clearTimeout(settleTimer.current);
    if (gesturingRef.current) {
      scheduleSettle();
      return undefined;
    }
    void runSettle();
    return () => {
      settleGen.current += 1;
      window.clearTimeout(settleTimer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- gesture settle uses scheduleSettle
  }, [latitude, longitude, frame.layer, withBasemap, sizeTick, relief, index]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return undefined;
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
        apiRef.current.onCameraGesture();
        return;
      }
      panRef.current = { x: panRef.current.x - event.deltaX, y: panRef.current.y - event.deltaY };
      apiRef.current.onCameraGesture();
    };
    stage.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      stage.removeEventListener("wheel", onWheel);
    };
  }, []);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return undefined;

    type Point = { x: number; y: number };
    type Pinch = {
      startSpan: number;
      startZoom: number;
      startPan: { x: number; y: number };
      originX: number;
      originY: number;
    };
    let pinch: Pinch | null = null;
    let gestureRaf = 0;
    let touchOwns = false;
    const touchPointers = new Map<number, Point>();
    let mouseDrag: { pointerId: number; x: number; y: number; panX: number; panY: number } | null =
      null;

    const blockedTarget = (target: EventTarget | null) =>
      (target as Element | null)?.closest?.(".zoom-controls, button, input, a");

    const flushPanZoom = () => {
      gestureRaf = 0;
      syncReactCamera();
    };

    const scheduleFlush = () => {
      if (!gestureRaf) gestureRaf = window.requestAnimationFrame(flushPanZoom);
      apiRef.current.scheduleBlit();
      apiRef.current.scheduleSettle();
    };

    const beginPinch = (pts: Point[]) => {
      if (pts.length < 2) return;
      const rect = stage.getBoundingClientRect();
      const center = pointerCenter(pts[0], pts[1]);
      drag.current = null;
      pinch = {
        startSpan: pointerSpan(pts[0], pts[1]),
        startZoom: zoomRef.current,
        startPan: { ...panRef.current },
        originX: center.x - rect.left,
        originY: center.y - rect.top,
      };
    };

    const applyPinch = (pts: Point[]) => {
      if (pts.length < 2) return;
      if (!pinch) beginPinch(pts);
      const active = pinch;
      if (!active || active.startSpan <= 0) return;
      const span = pointerSpan(pts[0], pts[1]);
      const targetZoom = pinchTargetZoom(
        active.startZoom,
        active.startSpan,
        span,
        MIN_ZOOM,
        MAX_ZOOM,
      );
      const next = zoomToward(
        active.startZoom,
        targetZoom,
        active.originX,
        active.originY,
        stage.clientWidth,
        stage.clientHeight,
        active.startPan,
      );
      zoomRef.current = next.zoom;
      panRef.current = next.pan;
      scheduleFlush();
    };

    const beginOneFingerPan = (pt: Point) => {
      pinch = null;
      drag.current = { x: pt.x, y: pt.y, panX: panRef.current.x, panY: panRef.current.y };
    };

    const applyOneFingerPan = (pt: Point) => {
      const start = drag.current;
      if (!start) return;
      panRef.current = {
        x: start.panX + pt.x - start.x,
        y: start.panY + pt.y - start.y,
      };
      scheduleFlush();
    };

    const syncTouchPointerGesture = () => {
      const pts = [...touchPointers.values()];
      if (pts.length >= 2) {
        applyPinch(pts);
        return;
      }
      if (pts.length === 1) {
        if (!drag.current || pinch) beginOneFingerPan(pts[0]);
        else applyOneFingerPan(pts[0]);
        return;
      }
      pinch = null;
      drag.current = null;
    };

    const onTouchStart = (event: TouchEvent) => {
      if (blockedTarget(event.target)) return;
      touchOwns = true;
      touchPointers.clear();
      const pts = touchPoints(event.touches);
      if (pts.length >= 2) {
        event.preventDefault();
        beginPinch(pts);
        return;
      }
      if (pts.length === 1) beginOneFingerPan(pts[0]);
    };

    const onTouchMove = (event: TouchEvent) => {
      if (!touchOwns) return;
      if (blockedTarget(event.target) && !drag.current && !pinch) return;
      const pts = touchPoints(event.touches);
      if (pts.length >= 2) {
        event.preventDefault();
        applyPinch(pts);
        return;
      }
      if (pts.length === 1 && drag.current) {
        event.preventDefault();
        applyOneFingerPan(pts[0]);
      }
    };

    const onTouchEnd = (event: TouchEvent) => {
      if (!touchOwns) return;
      const pts = touchPoints(event.touches);
      if (pts.length >= 2) {
        beginPinch(pts);
        return;
      }
      pinch = null;
      if (pts.length === 1) beginOneFingerPan(pts[0]);
      else {
        drag.current = null;
        touchOwns = false;
      }
    };

    const onPointerDown = (event: PointerEvent) => {
      if (event.button !== 0 || event.shiftKey) return;
      if (blockedTarget(event.target)) return;

      if (event.pointerType === "touch") {
        if (touchOwns) return;
        touchPointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
        if (touchPointers.size === 1) beginOneFingerPan({ x: event.clientX, y: event.clientY });
        else if (touchPointers.size >= 2) beginPinch([...touchPointers.values()]);
        return;
      }

      mouseDrag = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        panX: panRef.current.x,
        panY: panRef.current.y,
      };
      drag.current = { x: event.clientX, y: event.clientY, panX: panRef.current.x, panY: panRef.current.y };
      try {
        stage.setPointerCapture(event.pointerId);
      } catch {
        // Some browsers reject capture on unsupported targets.
      }
    };

    const onPointerMove = (event: PointerEvent) => {
      if (event.pointerType === "touch") {
        if (touchOwns || !touchPointers.has(event.pointerId)) return;
        event.preventDefault();
        touchPointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
        syncTouchPointerGesture();
        return;
      }
      if (!mouseDrag || mouseDrag.pointerId !== event.pointerId) return;
      panRef.current = {
        x: mouseDrag.panX + event.clientX - mouseDrag.x,
        y: mouseDrag.panY + event.clientY - mouseDrag.y,
      };
      scheduleFlush();
    };

    const endPointer = (event: PointerEvent) => {
      if (event.pointerType === "touch") {
        if (touchOwns || !touchPointers.has(event.pointerId)) return;
        touchPointers.delete(event.pointerId);
        const pts = [...touchPointers.values()];
        if (pts.length >= 2) beginPinch(pts);
        else if (pts.length === 1) beginOneFingerPan(pts[0]);
        else {
          pinch = null;
          drag.current = null;
        }
        return;
      }
      if (!mouseDrag || mouseDrag.pointerId !== event.pointerId) return;
      mouseDrag = null;
      drag.current = null;
      try {
        if (stage.hasPointerCapture(event.pointerId)) stage.releasePointerCapture(event.pointerId);
      } catch {
        // Ignore release failures.
      }
    };

    stage.addEventListener("touchstart", onTouchStart, { passive: false });
    stage.addEventListener("touchmove", onTouchMove, { passive: false });
    stage.addEventListener("touchend", onTouchEnd);
    stage.addEventListener("touchcancel", onTouchEnd);
    stage.addEventListener("pointerdown", onPointerDown);
    stage.addEventListener("pointermove", onPointerMove, { passive: false });
    stage.addEventListener("pointerup", endPointer);
    stage.addEventListener("pointercancel", endPointer);
    return () => {
      stage.removeEventListener("touchstart", onTouchStart);
      stage.removeEventListener("touchmove", onTouchMove);
      stage.removeEventListener("touchend", onTouchEnd);
      stage.removeEventListener("touchcancel", onTouchEnd);
      stage.removeEventListener("pointerdown", onPointerDown);
      stage.removeEventListener("pointermove", onPointerMove);
      stage.removeEventListener("pointerup", endPointer);
      stage.removeEventListener("pointercancel", endPointer);
      if (gestureRaf) window.cancelAnimationFrame(gestureRaf);
      if (blitRaf.current) window.cancelAnimationFrame(blitRaf.current);
      pinch = null;
      mouseDrag = null;
      touchOwns = false;
      touchPointers.clear();
      drag.current = null;
    };
  }, []);

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
      apiRef.current.onCameraGesture();
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
    apiRef.current.onCameraGesture();
  }

  function recenter() {
    panRef.current = { x: 0, y: 0 };
    setPan({ x: 0, y: 0 });
    reliefRecenter.current?.();
    apiRef.current.onCameraGesture();
  }

  return (
    <section ref={cardRef} className="radar-card" aria-label="Radar">
      <div
        ref={stageRef}
        className="radar-stage"
        style={{
          touchAction: RADAR_STAGE_GESTURE.touchAction,
          overscrollBehavior: RADAR_STAGE_GESTURE.overscrollBehavior,
        }}
      >
        <canvas
          ref={canvasRef}
          className={`radar-overlay${relief ? " radar-overlay-hidden" : ""}`}
          aria-hidden={relief}
        />
        {relief && ready && ground && (
          <RadarRelief
            echoes={echoes}
            cols={echoCols}
            rows={echoRows}
            ground={ground}
            empty={echoes.length === 0}
            boardTransformRef={boardTransformRef}
            sceneRef={reliefSceneRef}
            onRecenterRef={(fn) => {
              reliefRecenter.current = fn;
            }}
          />
        )}
        {(!ready || (relief && !ground)) && (
          <div className="radar-placeholder">{failed ? "Radar tiles unavailable" : "Loading radar…"}</div>
        )}
        <div className="zoom-controls">
          {glOk && (
            <button
              type="button"
              className="mode-toggle"
              aria-pressed={relief}
              aria-label={relief ? "Switch to flat radar" : "Switch to relief radar"}
              title={relief ? "Flat" : "Relief"}
              onClick={() => setMode((current) => (current === "flat" ? "relief" : "flat"))}
            >
              {relief ? "3D" : "2D"}
            </button>
          )}
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
              recenter();
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
        {frame.label} · MRMS hybrid-scan reflectivity
        {relief ? " · Relief" : ""}
      </p>
      <p className="radar-note">
        {withBasemap
          ? "Basemap © MapTiler © OpenStreetMap contributors"
          : "Radar only. Add a MapTiler key to show streets."}
      </p>
    </section>
  );
}

/** Device-pixel radar mosaic for echo sampling (relief). Tiles stay sharp on Flat via direct blit. */
function mosaicRadar(
  placed: PlacedTile[],
  view: View,
): { canvas: HTMLCanvasElement; image: ImageData } | null {
  const width = Math.max(1, Math.round(view.cssWidth * view.ratio));
  const height = Math.max(1, Math.round(view.cssHeight * view.ratio));
  const radarCanvas = document.createElement("canvas");
  radarCanvas.width = width;
  radarCanvas.height = height;
  const radarContext = radarCanvas.getContext("2d", { willReadFrequently: true });
  if (!radarContext) return null;
  radarContext.setTransform(view.ratio * view.scale, 0, 0, view.ratio * view.scale, 0, 0);
  radarContext.imageSmoothingEnabled = Math.abs(view.scale - 1) > 0.001;
  radarContext.clearRect(0, 0, view.viewWidth, view.viewHeight);
  let drew = false;
  for (const tile of placed) {
    if (tile.kind !== "radar" || !tile.source) continue;
    radarContext.drawImage(tile.source, tile.dx, tile.dy, TILE, TILE);
    drew = true;
  }
  if (!drew) return null;
  radarContext.setTransform(1, 0, 0, 1, 0, 0);
  const image = radarContext.getImageData(0, 0, width, height);
  return { canvas: radarCanvas, image };
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
