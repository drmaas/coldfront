import { isUsZip, placeFromGeocodeHit, type GeocodeHit } from "./domain";
import { locationPath } from "./location";

export type SearchFlowResult =
  | { kind: "navigate"; path: string }
  | { kind: "pick" }
  | { kind: "error"; message: string };

/** Query length accepted by `/api/geocode/search` and Open-Meteo geocoding. */
export const GEOCODE_QUERY_MAX_LEN = 64;

export function geocodeHitsMatchQuery(query: string, hitsQuery: string | null, hits: GeocodeHit[]): boolean {
  const trimmed = query.trim();
  if (!trimmed || isUsZip(trimmed)) return false;
  if (hits.length === 0) return false;
  return hitsQuery === trimmed;
}

export function resolveLocationSearchFlow(
  trimmed: string,
  hits: GeocodeHit[],
  hitsQuery: string | null = null,
): SearchFlowResult {
  if (!trimmed) return { kind: "error", message: "Enter a ZIP code or city name." };
  if (isUsZip(trimmed)) return { kind: "navigate", path: `/${trimmed}` };
  const currentHits =
    hitsQuery === null || geocodeHitsMatchQuery(trimmed, hitsQuery, hits) ? hits : ([] as GeocodeHit[]);
  if (currentHits.length === 1) {
    return { kind: "navigate", path: locationPath(placeFromGeocodeHit(currentHits[0])) };
  }
  if (currentHits.length > 1) return { kind: "pick" };
  return { kind: "error", message: "No matching US cities found." };
}