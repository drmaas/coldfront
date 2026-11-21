import { describe, expect, it } from "vitest";
import { parseGeocodeSearch } from "./domain";
import { resolveLocationSearchFlow } from "./locationSearchFlow";

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
    expect(resolveLocationSearchFlow("Eureka, CA", hits)).toEqual({ kind: "navigate", path: "/95501" });
  });
});