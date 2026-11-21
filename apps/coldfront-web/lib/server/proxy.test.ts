import { afterEach, describe, expect, it, vi } from "vitest";
import { GEOCODE_UPSTREAM_TIMEOUT_MS, planApi, proxyApi, USER_AGENT } from "./proxy";

describe("api proxy", () => {
  it("refuses to publish a MapTiler key and degrades without one", () => {
    const config = planApi(new URL("https://coldfront.local/api/config"), undefined);
    expect(config).toMatchObject({ type: "response", body: JSON.stringify({ basemap: "none" }) });
    const missing = planApi(new URL("https://coldfront.local/api/map/7/30/46.png"), undefined);
    expect(missing).toMatchObject({ type: "response", status: 404 });
    const keyed = planApi(new URL("https://coldfront.local/api/map/7/30/46.png"), "secret-key");
    expect(keyed.type).toBe("upstream");
    if (keyed.type !== "upstream") return;
    expect(keyed.upstream.url).toBe("https://api.maptiler.com/maps/basic-v2/7/30/46.png?key=secret-key");
    expect(keyed.upstream.headers["user-agent"]).toBe(USER_AGENT);
    const bad = planApi(new URL("https://coldfront.local/api/map/30/1/1.png"), "secret-key");
    expect(bad).toMatchObject({ type: "response", status: 400 });
  });

  it("builds the weather upstreams and rejects bad input", () => {
    const zip = planApi(new URL("https://coldfront.local/api/zip/55115"), undefined);
    expect(zip.type).toBe("upstream");
    if (zip.type === "upstream") expect(zip.upstream.url).toBe("https://api.zippopotam.us/us/55115");
    expect(planApi(new URL("https://coldfront.local/api/zip/nope"), undefined)).toMatchObject({ status: 400 });
    const geocode = planApi(new URL("https://coldfront.local/api/geocode/search?name=Eureka"), undefined);
    expect(geocode.type).toBe("upstream");
    if (geocode.type === "upstream") {
      expect(geocode.upstream.url).toContain("geocoding-api.open-meteo.com");
      expect(geocode.upstream.url).toContain("countryCode=US");
    }

    const forecast = planApi(
      new URL("https://coldfront.local/api/forecast?latitude=45.071&longitude=-92.9391"),
      undefined,
    );
    expect(forecast.type).toBe("upstream");
    if (forecast.type === "upstream") {
      const target = new URL(forecast.upstream.url);
      expect(target.origin).toBe("https://api.open-meteo.com");
      expect(target.searchParams.get("latitude")).toBe("45.071");
      expect(target.searchParams.get("longitude")).toBe("-92.9391");
      expect(target.searchParams.get("temperature_unit")).toBe("celsius");
      expect(forecast.upstream.headers["user-agent"]).toBe(USER_AGENT);
    }
    expect(planApi(new URL("https://coldfront.local/api/forecast?latitude=999&longitude=0"), undefined)).toMatchObject({
      status: 400,
    });

    const alerts = planApi(
      new URL("https://coldfront.local/api/alerts?latitude=45.071&longitude=-92.9391"),
      undefined,
    );
    expect(alerts.type).toBe("upstream");
    if (alerts.type === "upstream") {
      expect(alerts.upstream.url).toBe("https://api.weather.gov/alerts/active?point=45.071,-92.9391");
    }
    expect(planApi(new URL("https://coldfront.local/api/other"), undefined)).toMatchObject({ status: 404 });
  });

  it("times out slow geocode upstreams with 504", async () => {
    const fetchMock = vi.fn((_url: string, init?: RequestInit) => {
      expect(init?.signal).toBeDefined();
      const error = new Error("The operation was aborted");
      error.name = "TimeoutError";
      return Promise.reject(error);
    });
    vi.stubGlobal("fetch", fetchMock);
    const response = await proxyApi(
      new Request("https://coldfront.local/api/geocode/search?name=Eureka"),
      undefined,
    );
    expect(response.status).toBe(504);
    expect(await response.text()).toBe("upstream timed out");
  });

  it("passes the geocode upstream timeout to fetch", () => {
    expect(GEOCODE_UPSTREAM_TIMEOUT_MS).toBeGreaterThanOrEqual(10_000);
    expect(GEOCODE_UPSTREAM_TIMEOUT_MS).toBeLessThanOrEqual(12_000);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });
});
