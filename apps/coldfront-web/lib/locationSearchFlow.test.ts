import { describe, expect, it } from "vitest";
import { parseGeocodeSearch, type GeocodeHit } from "./domain";
import { geocodeHitsMatchQuery, resolveLocationSearchFlow } from "./locationSearchFlow";

const GEOCODE = {
  results: [
    {
      name: "Eureka",
      latitude: 40.80207,
      longitude: -124.16367,
      country_code: "US",
      admin1: "California",
      postcodes: ["95501"],
    },
  ],
};

describe("location search flow", () => {
  it("keeps ZIP entry unchanged", () => {
    expect(resolveLocationSearchFlow("95501", [])).toEqual({ kind: "navigate", path: "/95501" });
  });

  it("chooses the ZIP path when Eureka has a postcode", () => {
    const hits = parseGeocodeSearch(GEOCODE);
    expect(resolveLocationSearchFlow("Eureka, CA", hits, "Eureka, CA")).toEqual({
      kind: "navigate",
      path: "/95501",
    });
  });

  it("does not submit stale Springfield hits after the query becomes Eureka", () => {
    const springfieldHits: GeocodeHit[] = [
      {
        name: "Springfield",
        admin1: "Illinois",
        stateAbbreviation: "IL",
        latitude: 39.7817,
        longitude: -89.6501,
        postcodes: ["62701"],
      },
    ];
    expect(geocodeHitsMatchQuery("Eureka", "Springfield", springfieldHits)).toBe(false);
    expect(resolveLocationSearchFlow("Eureka", springfieldHits, "Springfield")).toEqual({
      kind: "error",
      message: "No matching US cities found.",
    });
  });
});