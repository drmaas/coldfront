import type { Place } from "../lib/domain";
import { coordinatesMatch } from "../lib/location";

const STORAGE_KEY = "coldfront-place";

export function readStoredPlace(latitude: number, longitude: number): Place | null {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const place = JSON.parse(raw) as Place;
    if (!coordinatesMatch(place.latitude, place.longitude, latitude, longitude)) return null;
    return place;
  } catch {
    return null;
  }
}

export function writeStoredPlace(place: Place): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(place));
}