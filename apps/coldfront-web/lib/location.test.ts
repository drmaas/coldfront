import { describe, expect, it } from "vitest";
import { formatGeocodeLabel, parseGeocodeSearch, placeFromGeocodeHit } from "./domain";
import { isZipLookup, locationPath, parseRoutePath } from "./location";

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

describe("location routing", () => {
  it("treats five digits as a ZIP lookup", () => {
    expect(isZipLookup("95501")).toBe(true);
    expect(isZipLookup("Eureka, CA")).toBe(false);
  });

  it("maps geocoder hits to ZIP paths when known", () => {
    const hit = parseGeocodeSearch(GEOCODE)[0];
    const place = placeFromGeocodeHit(hit);
    expect(place.zip).toBe("95501");
    expect(locationPath(place)).toBe("/95501");
    expect(formatGeocodeLabel(hit)).toContain("95501");
  });

  it("parses coordinate routes", () => {
    expect(parseRoutePath("/95501")).toEqual({ kind: "zip", zip: "95501" });
    expect(parseRoutePath("/@40.8021,-124.1637")).toEqual({
      kind: "coordinates",
      latitude: 40.8021,
      longitude: -124.1637,
    });
  });
});