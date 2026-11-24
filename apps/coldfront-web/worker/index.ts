import { createInIsolateMapTileLimiter } from "../lib/server/mapTileGuard";
import { proxyApi, type ProxyContext } from "../lib/server/proxy";

/** Must stay unique within the Cloudflare account (see wrangler `ratelimits`). */
const MAP_TILE_RATE_LIMIT = 600;
const MAP_TILE_RATE_PERIOD_SEC = 60;

const inIsolateMapTileLimit = createInIsolateMapTileLimiter(
  MAP_TILE_RATE_LIMIT,
  MAP_TILE_RATE_PERIOD_SEC * 1000,
);

export default {
  async fetch(request, env): Promise<Response> {
    const url = new URL(request.url);
    if (!url.pathname.startsWith("/api/")) {
      return new Response(null, { status: 404 });
    }
    try {
      return await proxyApi(request, maptilerKey(env), proxyContext(env));
    } catch (error) {
      console.error(
        JSON.stringify({
          message: "api request failed",
          path: url.pathname,
          error: error instanceof Error ? error.message : "unknown",
        }),
      );
      return new Response("upstream failed", { status: 502 });
    }
  },
} satisfies ExportedHandler<Env>;

function maptilerKey(env: Env): string | undefined {
  // Optional secret, deliberately absent from wrangler.jsonc so a deploy works
  // with no key. Generated Env therefore does not list it.
  const value = (env as Env & { MAPTILER_API_KEY?: string }).MAPTILER_API_KEY;
  return value && value.length > 0 ? value : undefined;
}

function proxyContext(env: Env): ProxyContext {
  const limiter = (env as Env & { MAP_TILE_RATE_LIMITER?: RateLimit }).MAP_TILE_RATE_LIMITER;
  return {
    mapTileRateLimit: limiter
      ? async (key) => {
          const { success } = await limiter.limit({ key });
          return { allowed: success };
        }
      : inIsolateMapTileLimit,
  };
}
