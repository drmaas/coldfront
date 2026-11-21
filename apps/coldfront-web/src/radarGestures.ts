/** CSS and behavior for the radar stage — shared across 2D, 3D, and fullscreen. */
export const RADAR_STAGE_GESTURE = {
  touchAction: "none",
  overscrollBehavior: "contain",
  /** Two-finger pinch on the stage adjusts map zoom (not browser/page zoom). */
  pinchZoomEnabled: true,
  /** One-finger drag on the stage pans the map. */
  panEnabled: true,
  /** Wheel without ctrl/meta pans; wheel with ctrl/meta or classic wheel zooms. */
  wheelZoomUsesCtrlOrMeta: true,
} as const;

export type RadarViewMode = "flat" | "relief" | "fullscreen-flat" | "fullscreen-relief";

/** Gesture flags are identical in every radar view mode; pinch is handled on the stage. */
export function radarGestureConfig(mode: RadarViewMode): typeof RADAR_STAGE_GESTURE {
  void mode;
  return RADAR_STAGE_GESTURE;
}

export function pointerSpan(
  a: { x: number; y: number },
  b: { x: number; y: number },
): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

export function pointerCenter(
  a: { x: number; y: number },
  b: { x: number; y: number },
): { x: number; y: number } {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

/** Snapshot live touches — preferred over PointerEvents for multi-finger pinch. */
export function touchPoints(touches: TouchList | ArrayLike<Touch>): { x: number; y: number }[] {
  const points: { x: number; y: number }[] = [];
  for (let i = 0; i < touches.length; i += 1) {
    const touch = touches[i];
    if (!touch) continue;
    points.push({ x: touch.clientX, y: touch.clientY });
  }
  return points;
}

/** Continuous zoom from an initial pinch span; `log2` keeps pinch speed even. */
export function pinchTargetZoom(
  startZoom: number,
  startSpan: number,
  currentSpan: number,
  minZoom: number,
  maxZoom: number,
): number {
  if (startSpan <= 0 || currentSpan <= 0 || !Number.isFinite(startZoom)) return startZoom;
  const target = startZoom + Math.log2(currentSpan / startSpan);
  return Math.min(maxZoom, Math.max(minZoom, target));
}