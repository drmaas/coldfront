export const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

interface MotionQuery {
  matches: boolean;
}

/** Autoplay only when the reader has not asked the OS to reduce motion. */
export function radarAutoplays(query: MotionQuery | null): boolean {
  return !query?.matches;
}

/** The OS reduce-motion setting, or null where matchMedia is unavailable. */
export function reducedMotionQuery(): MediaQueryList | null {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return null;
  return window.matchMedia(REDUCED_MOTION_QUERY);
}
