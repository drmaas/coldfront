/**
 * Same-origin API plans for the Worker and for tests.
 * The MapTiler key stays on the server. Callers must not echo upstream URLs.
 */

export const USER_AGENT = "Coldfront/0.1 (https://github.com/drmaas/coldfront)";

const OPEN_METEO =
  "https://api.open-meteo.com/v1/forecast?current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,is_day&hourly=temperature_2m,weather_code,precipitation_probability,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,precipitation_sum,wind_speed_10m_max,sunrise,sunset&temperature_unit=celsius&wind_speed_unit=kmh&timezone=auto&forecast_days=10";

export interface Upstream {
  url: string;
  headers: Record<string, string>;
  cacheControl: string;
}

export type ApiPlan =
  | { type: "ignore" }
  | { type: "response"; status: number; body: string; contentType: string }
  | { type: "upstream"; upstream: Upstream; kind: "json" | "png" };

export function planApi(url: URL, maptilerKey: string | undefined): ApiPlan {
  const path = url.pathname;
  if (!path.startsWith("/api/")) return { type: "ignore" };
  if (path === "/api/config") {
    return json(200, { basemap: maptilerKey ? "maptiler" : "none" });
  }
  const zip = /^\/api\/zip\/(\d{5})$/.exec(path);
  if (zip) {
    return upstream(`https://api.zippopotam.us/us/${zip[1]}`, "public, max-age=86400", "json");
  }
  if (path === "/api/zip" || path.startsWith("/api/zip/")) {
    return text(400, "enter a 5-digit US ZIP code");
  }
  if (path === "/api/forecast") {
    const latitude = numberParam(url, "latitude", -90, 90);
    const longitude = numberParam(url, "longitude", -180, 180);
    if (latitude === null || longitude === null) return text(400, "latitude and longitude are required");
    const target = new URL(OPEN_METEO);
    target.searchParams.set("latitude", String(latitude));
    target.searchParams.set("longitude", String(longitude));
    return upstream(target.toString(), "public, max-age=300", "json");
  }
  if (path === "/api/alerts") {
    const latitude = numberParam(url, "latitude", -90, 90);
    const longitude = numberParam(url, "longitude", -180, 180);
    if (latitude === null || longitude === null) return text(400, "latitude and longitude are required");
    return upstream(
      `https://api.weather.gov/alerts/active?point=${latitude},${longitude}`,
      "public, max-age=60",
      "json",
    );
  }
  const tile = /^\/api\/map\/(\d+)\/(\d+)\/(\d+)\.png$/.exec(path);
  if (tile) {
    const zoom = Number(tile[1]);
    const x = Number(tile[2]);
    const y = Number(tile[3]);
    const limit = 2 ** zoom;
    if (zoom > 18 || x >= limit || y >= limit) return text(400, "tile is out of range");
    if (!maptilerKey) return text(404, "basemap unavailable");
    const target = new URL(`https://api.maptiler.com/maps/basic-v2/${zoom}/${x}/${y}.png`);
    target.searchParams.set("key", maptilerKey);
    return upstream(target.toString(), "public, max-age=86400", "png");
  }
  return text(404, "not found");
}

export async function proxyApi(request: Request, maptilerKey: string | undefined): Promise<Response> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    return new Response("method not allowed", { status: 405 });
  }
  const plan = planApi(new URL(request.url), maptilerKey);
  if (plan.type === "ignore" || plan.type === "response") {
    if (plan.type === "ignore") return new Response("not found", { status: 404 });
    return new Response(plan.body, {
      status: plan.status,
      headers: { "content-type": plan.contentType, "cache-control": "no-store" },
    });
  }
  let upstreamResponse: Response;
  try {
    upstreamResponse = await fetch(plan.upstream.url, { headers: plan.upstream.headers });
  } catch {
    return new Response("upstream failed", { status: 502 });
  }
  const headers = new Headers();
  headers.set(
    "content-type",
    upstreamResponse.headers.get("content-type") ?? (plan.kind === "png" ? "image/png" : "application/json"),
  );
  headers.set("cache-control", plan.upstream.cacheControl);
  return new Response(upstreamResponse.body, { status: upstreamResponse.status, headers });
}

function upstream(url: string, cacheControl: string, kind: "json" | "png"): ApiPlan {
  return {
    type: "upstream",
    kind,
    upstream: { url, cacheControl, headers: { "user-agent": USER_AGENT, accept: kind === "png" ? "image/png" : "application/json" } },
  };
}

function json(status: number, body: unknown): ApiPlan {
  return { type: "response", status, body: JSON.stringify(body), contentType: "application/json" };
}

function text(status: number, body: string): ApiPlan {
  return { type: "response", status, body, contentType: "text/plain; charset=utf-8" };
}

function numberParam(url: URL, name: string, min: number, max: number): number | null {
  const raw = url.searchParams.get(name);
  if (raw === null || raw.trim() === "") return null;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < min || value > max) return null;
  return value;
}
