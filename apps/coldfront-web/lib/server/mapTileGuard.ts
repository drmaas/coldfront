/**
 * Same-site gate for /api/map before MapTiler is contacted.
 *
 * Browser <img> loads for same-origin tiles usually omit `Origin` but send
 * `Referer` (this site sets Referrer-Policy: strict-origin-when-cross-origin).
 * We validate `Origin` when present, otherwise `Referer`. We do not mint
 * HMAC query tokens: Referer has been reliable for our relative tile URLs.
 */

export const SITE_HOST = "coldfront.fyi";

export type MapTileRateLimit = (clientKey: string) => Promise<{ allowed: boolean }>;

export function allowedMapTileHosts(requestUrl: URL): Set<string> {
  const hosts = new Set<string>([SITE_HOST, `www.${SITE_HOST}`, requestUrl.hostname]);
  if (requestUrl.hostname === "localhost" || requestUrl.hostname === "127.0.0.1") {
    hosts.add("localhost");
    hosts.add("127.0.0.1");
  }
  return hosts;
}

/** True when the request may load a map tile from this Worker. */
export function mapTileSameSiteAllowed(request: Request, requestUrl: URL): boolean {
  const allowed = allowedMapTileHosts(requestUrl);
  const origin = request.headers.get("Origin");
  if (origin) {
    try {
      return allowed.has(new URL(origin).hostname);
    } catch {
      return false;
    }
  }
  const referer = request.headers.get("Referer");
  if (referer) {
    try {
      return allowed.has(new URL(referer).hostname);
    } catch {
      return false;
    }
  }
  return false;
}

/** Per-IP key for rate limiting (Cloudflare provides the client IP at the edge). */
export function mapTileClientKey(request: Request): string {
  const cf = request.headers.get("CF-Connecting-IP")?.trim();
  if (cf) return cf;
  const forwarded = request.headers.get("X-Forwarded-For")?.split(",")[0]?.trim();
  if (forwarded) return forwarded;
  return "unknown";
}

/**
 * In-isolate token bucket used when the Workers rate-limit binding is absent
 * (unit tests, vitest, some local dev). Counts are not shared across isolates
 * or PoPs — production should use the `ratelimits` binding in wrangler.jsonc.
 */
export function createInIsolateMapTileLimiter(limit: number, periodMs: number): MapTileRateLimit {
  const buckets = new Map<string, { count: number; windowStart: number }>();
  return async (clientKey: string) => {
    const now = Date.now();
    let bucket = buckets.get(clientKey);
    if (!bucket || now - bucket.windowStart >= periodMs) {
      bucket = { count: 0, windowStart: now };
      buckets.set(clientKey, bucket);
    }
    bucket.count += 1;
    return { allowed: bucket.count <= limit };
  };
}