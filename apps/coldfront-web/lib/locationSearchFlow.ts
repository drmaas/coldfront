import { isUsZip, placeFromGeocodeHit, type GeocodeHit } from "./domain";
import { locationPath } from "./location";

export type SearchFlowResult =
  | { kind: "navigate"; path: string }
  | { kind: "pick" }
  | { kind: "error"; message: string };

export function resolveLocationSearchFlow(trimmed: string, hits: GeocodeHit[]): SearchFlowResult {
  if (!trimmed) return { kind: "error", message: "Enter a ZIP code or city name." };
  if (isUsZip(trimmed)) return { kind: "navigate", path: `/${trimmed}` };
  if (hits.length === 1) return { kind: "navigate", path: locationPath(placeFromGeocodeHit(hits[0])) };
  if (hits.length > 1) return { kind: "pick" };
  return { kind: "error", message: "No matching US cities found." };
}