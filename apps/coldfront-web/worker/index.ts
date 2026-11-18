import { proxyApi } from "../lib/server/proxy";

export default {
  async fetch(request, env): Promise<Response> {
    const url = new URL(request.url);
    if (!url.pathname.startsWith("/api/")) {
      return new Response(null, { status: 404 });
    }
    try {
      return await proxyApi(request, maptilerKey(env));
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
