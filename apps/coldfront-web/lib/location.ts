import { isUsZip, type Place } from "./domain";

const COORD_ROUTE_PREFIX = "/@";

export function isZipLookup(value: string): boolean {
  return isUsZip(value.trim());
}

export function hasResolvedCoordinates(latitude: number, longitude: number): boolean {
  return (
    Number.isFinite(latitude) &&
    Number.isFinite(longitude) &&
    latitude >= -90 &&
    latitude <= 90 &&
    longitude >= -180 &&
    longitude <= 180 &&
    (latitude !== 0 || longitude !== 0)
  );
}

export function locationPath(place: Place): string {
  if (isUsZip(place.zip)) return `/${place.zip}`;
  return `${COORD_ROUTE_PREFIX}${place.latitude.toFixed(4)},${place.longitude.toFixed(4)}`;
}

export type RouteTarget =
  | { kind: "home" }
  | { kind: "zip"; zip: string }
  | { kind: "coordinates"; latitude: number; longitude: number };

export function parseRoutePath(path: string): RouteTarget {
  const trimmed = path.replace(/\/+$/, "").replace(/^\/+/, "");
  if (!trimmed) return { kind: "home" };
  if (trimmed.startsWith("@")) {
    const [lat, lon] = trimmed.slice(1).split(",");
    const latitude = Number(lat);
    const longitude = Number(lon);
    if (hasResolvedCoordinates(latitude, longitude)) {
      return { kind: "coordinates", latitude, longitude };
    }
    return { kind: "home" };
  }
  if (isUsZip(trimmed)) return { kind: "zip", zip: trimmed };
  return { kind: "home" };
}

export function coordinatesMatch(aLat: number, aLon: number, bLat: number, bLon: number): boolean {
  return Math.abs(aLat - bLat) < 0.000_05 && Math.abs(aLon - bLon) < 0.000_05;
}

export function placeFromCoordinates(
  latitude: number,
  longitude: number,
  stored: Place | null,
): Place {
  if (stored && coordinatesMatch(stored.latitude, stored.longitude, latitude, longitude)) {
    return stored;
  }
  return {
    zip: "",
    city: "",
    state: "",
    stateAbbreviation: "",
    latitude,
    longitude,
  };
}