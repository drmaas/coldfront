/** Domain logic shared in spirit with `coldfront-core`. Fixtures keep them aligned. */

export interface Place {
  zip: string;
  city: string;
  state: string;
  stateAbbreviation: string;
  latitude: number;
  longitude: number;
}

export interface Current {
  time: string;
  temperatureC: number;
  apparentC: number;
  humidity: number;
  windKmh: number;
  weatherCode: number;
  /** Open-Meteo day/night flag for the place timezone. */
  isDay: boolean;
}

export interface Hour {
  time: string;
  temperatureC: number;
  weatherCode: number;
  precipProbability: number | null;
  isDay: boolean;
}

export interface Day {
  date: string;
  weatherCode: number;
  tempMaxC: number;
  tempMinC: number;
  precipProbability: number;
  precipSumMm: number | null;
  windMaxKmh: number | null;
  sunrise: string | null;
  sunset: string | null;
}

export interface Forecast {
  latitude: number;
  longitude: number;
  timezone: string;
  current: Current;
  hourly: Hour[];
  daily: Day[];
}

export interface Alert {
  id: string;
  event: string;
  headline: string;
  severity: string;
  urgency: string;
  description: string;
  instruction: string;
  onset: string;
  ends: string;
  senderName: string;
}

export interface WeatherInfo {
  label: string;
  icon: string;
}

export interface RadarFrame {
  offsetMinutes: number;
  layer: string;
  label: string;
}

export interface TilePoint {
  zoom: number;
  x: number;
  y: number;
  pixelX: number;
  pixelY: number;
}

export interface TileWindow {
  zoom: number;
  originX: number;
  originY: number;
  spanX: number;
  spanY: number;
  pinX: number;
  pinY: number;
}

export const IEM_TILE_ORIGIN = "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0";

export function isUsZip(value: string): boolean {
  return /^\d{5}$/.test(value);
}

export function roundHalfAwayFromZero(value: number): number {
  if (!Number.isFinite(value)) return 0;
  if (value >= 0) return Math.floor(value + 0.5);
  return Math.ceil(value - 0.5);
}

export function celsiusToFahrenheit(celsius: number): number {
  return (celsius * 9) / 5 + 32;
}

export function kmhToMph(kmh: number): number {
  return kmh / 1.609344;
}

export function displayTemperature(celsius: number, fahrenheit: boolean): number {
  return roundHalfAwayFromZero(fahrenheit ? celsiusToFahrenheit(celsius) : celsius);
}

export function displayWindMph(kmh: number): number {
  return roundHalfAwayFromZero(kmhToMph(kmh));
}

export function describe(code: number): WeatherInfo {
  if (code === 0) return { label: "Clear", icon: "clear" };
  if (code === 1) return { label: "Mainly clear", icon: "mainly-clear" };
  if (code === 2) return { label: "Partly cloudy", icon: "partly-cloudy" };
  if (code === 3) return { label: "Overcast", icon: "overcast" };
  if (code === 45 || code === 48) return { label: "Fog", icon: "fog" };
  if (code === 51 || code === 53 || code === 55) return { label: "Drizzle", icon: "drizzle" };
  if (code === 56 || code === 57) return { label: "Freezing drizzle", icon: "freezing-drizzle" };
  if (code === 61 || code === 63 || code === 65) return { label: "Rain", icon: "rain" };
  if (code === 66 || code === 67) return { label: "Freezing rain", icon: "freezing-rain" };
  if (code === 71 || code === 73 || code === 75 || code === 77) return { label: "Snow", icon: "snow" };
  if (code === 80 || code === 81 || code === 82) return { label: "Rain showers", icon: "rain-showers" };
  if (code === 85 || code === 86) return { label: "Snow showers", icon: "snow-showers" };
  if (code === 95 || code === 96 || code === 99) return { label: "Thunderstorm", icon: "thunder" };
  return { label: "Unknown", icon: "unknown" };
}

/** Day or night display icon id from a `describe` icon and Open-Meteo `is_day`. */
export function displayIcon(icon: string, isDay: boolean): string {
  if (!isDay) {
    if (icon === "clear") return "clear-night";
    if (icon === "mainly-clear") return "mainly-clear-night";
    if (icon === "partly-cloudy") return "partly-cloudy-night";
  }
  switch (icon) {
    case "clear":
    case "mainly-clear":
    case "partly-cloudy":
    case "overcast":
    case "fog":
    case "drizzle":
    case "freezing-drizzle":
    case "rain":
    case "freezing-rain":
    case "snow":
    case "rain-showers":
    case "snow-showers":
    case "thunder":
    case "clear-night":
    case "mainly-clear-night":
    case "partly-cloudy-night":
      return icon;
    default:
      return "unknown";
  }
}

export function parseZippopotam(body: unknown): Place {
  const value = record(body, "zip response");
  const places = value.places;
  if (!Array.isArray(places) || places.length === 0) {
    throw new Error("no place found for that ZIP code");
  }
  const place = record(places[0], "place");
  return {
    zip: requiredString(value, "post code"),
    city: requiredString(place, "place name"),
    state: requiredString(place, "state"),
    stateAbbreviation: requiredString(place, "state abbreviation"),
    latitude: Number(requiredString(place, "latitude")),
    longitude: Number(requiredString(place, "longitude")),
  };
}

export function parseOpenMeteo(body: unknown): Forecast {
  const value = record(body, "forecast");
  const current = record(value.current, "current");
  const hourly = record(value.hourly, "hourly");
  const daily = record(value.daily, "daily");
  const hourTimes = stringArray(hourly, "time");
  const hourTemps = numberArray(hourly, "temperature_2m");
  const hourCodes = numberArray(hourly, "weather_code");
  const hourPops = optionalPrecipArray(hourly, "precipitation_probability", hourTimes.length);
  const hourIsDay = numberArray(hourly, "is_day");
  if (
    hourTimes.length !== hourTemps.length ||
    hourTimes.length !== hourCodes.length ||
    hourTimes.length !== hourPops.length ||
    hourTimes.length !== hourIsDay.length
  ) {
    throw new Error("hourly length mismatch");
  }
  const dayTimes = stringArray(daily, "time");
  const dayCodes = numberArray(daily, "weather_code");
  const dayMax = numberArray(daily, "temperature_2m_max");
  const dayMin = numberArray(daily, "temperature_2m_min");
  const dayPops = numberArray(daily, "precipitation_probability_max");
  const daySums = optionalNumberArray(daily, "precipitation_sum", dayTimes.length);
  const dayWinds = optionalNumberArray(daily, "wind_speed_10m_max", dayTimes.length);
  const daySunrise = optionalStringArray(daily, "sunrise", dayTimes.length);
  const daySunset = optionalStringArray(daily, "sunset", dayTimes.length);
  if (
    dayTimes.length !== dayCodes.length ||
    dayTimes.length !== dayMax.length ||
    dayTimes.length !== dayMin.length ||
    dayTimes.length !== dayPops.length
  ) {
    throw new Error("daily length mismatch");
  }
  return {
    latitude: requiredNumber(value, "latitude"),
    longitude: requiredNumber(value, "longitude"),
    timezone: requiredString(value, "timezone"),
    current: {
      time: requiredString(current, "time"),
      temperatureC: requiredNumber(current, "temperature_2m"),
      apparentC: requiredNumber(current, "apparent_temperature"),
      humidity: roundHalfAwayFromZero(requiredNumber(current, "relative_humidity_2m")),
      windKmh: requiredNumber(current, "wind_speed_10m"),
      weatherCode: roundHalfAwayFromZero(requiredNumber(current, "weather_code")),
      isDay: requiredNumber(current, "is_day") !== 0,
    },
    hourly: hourTimes.map((time, index) => ({
      time,
      temperatureC: hourTemps[index],
      weatherCode: roundHalfAwayFromZero(hourCodes[index]),
      precipProbability: hourPops[index],
      isDay: hourIsDay[index] !== 0,
    })),
    daily: dayTimes.map((date, index) => ({
      date,
      weatherCode: roundHalfAwayFromZero(dayCodes[index]),
      tempMaxC: dayMax[index],
      tempMinC: dayMin[index],
      precipProbability: roundHalfAwayFromZero(dayPops[index]),
      precipSumMm: daySums[index],
      windMaxKmh: dayWinds[index],
      sunrise: daySunrise[index],
      sunset: daySunset[index],
    })),
  };
}

export function parseAlerts(body: unknown): Alert[] {
  const value = record(body, "alerts");
  if (!Array.isArray(value.features)) throw new Error("missing features");
  const alerts: Alert[] = [];
  for (const feature of value.features) {
    if (!isRecord(feature)) continue;
    const properties = feature.properties;
    if (!isRecord(properties)) continue;
    const event = text(properties, "event");
    if (!event) continue;
    alerts.push({
      id: typeof feature.id === "string" ? feature.id : "",
      event,
      headline: text(properties, "headline"),
      severity: text(properties, "severity"),
      urgency: text(properties, "urgency"),
      description: text(properties, "description"),
      instruction: text(properties, "instruction"),
      onset: text(properties, "onset"),
      ends: text(properties, "ends"),
      senderName: text(properties, "senderName"),
    });
  }
  return alerts;
}

export function nextHours(forecast: Forecast, count: number): Hour[] {
  let start = 0;
  for (let index = 0; index < forecast.hourly.length; index += 1) {
    if (forecast.hourly[index].time <= forecast.current.time) start = index;
  }
  return forecast.hourly.slice(start, start + count);
}

export function formatAsOf(time: string): string {
  return formatClockTime(time) ?? time;
}

export function formatClockTime(time: string): string | null {
  const hour = clockHour(time);
  if (hour === null) return null;
  const minute = time.slice(14, 16) || "00";
  const suffix = hour >= 12 ? "PM" : "AM";
  const hour12 = hour % 12 === 0 ? 12 : hour % 12;
  return `${hour12}:${minute} ${suffix}`;
}

export function formatPrecipAmountMm(mm: number, fahrenheit: boolean): string {
  if (fahrenheit) return `${roundHalfAwayFromZero(mm / 25.4 * 100) / 100} in`;
  return `${roundHalfAwayFromZero(mm * 10) / 10} mm`;
}

export function hourLabel(time: string, isNow: boolean): string {
  if (isNow) return "Now";
  const hour = clockHour(time);
  if (hour === null) return time;
  if (hour === 0) return "12a";
  if (hour === 12) return "12p";
  if (hour < 12) return `${hour}a`;
  return `${hour - 12}p`;
}

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

export function dayLabel(date: string, today: string): string {
  if (date === today) return "Today";
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!match) return date;
  const utc = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return WEEKDAYS[utc.getUTCDay()];
}

export function webRadarFrames(now: Date = new Date()): RadarFrame[] {
  const frames: RadarFrame[] = [];
  for (let step = 11; step >= 0; step -= 1) frames.push(radarFrame(step * 5, now));
  return frames;
}

export function appletRadarFrames(now: Date = new Date()): RadarFrame[] {
  return [50, 40, 30, 20, 10, 0].map((offset) => radarFrame(offset, now));
}

/** IEM tile layer for one animation offset. See Rust `radar_layer`. */
export function radarLayer(offsetMinutes: number, now: Date = new Date()): string {
  if (offsetMinutes === 0) return "q2-hsr-900913";
  const ms = now.getTime() - offsetMinutes * 60_000;
  const floored = ms - (((ms % 120_000) + 120_000) % 120_000);
  const stamp = new Date(floored);
  const y = stamp.getUTCFullYear();
  const month = String(stamp.getUTCMonth() + 1).padStart(2, "0");
  const day = String(stamp.getUTCDate()).padStart(2, "0");
  const hour = String(stamp.getUTCHours()).padStart(2, "0");
  const minute = String(stamp.getUTCMinutes()).padStart(2, "0");
  return `mrms::lcref-${y}${month}${day}${hour}${minute}`;
}

export function iemTileUrl(layer: string, zoom: number, x: number, y: number): string {
  return `${IEM_TILE_ORIGIN}/${layer}/${zoom}/${x}/${y}.png`;
}

export function iemTileTemplate(layer: string): string {
  return `${IEM_TILE_ORIGIN}/${layer}/{z}/{x}/{y}.png`;
}

export function latLonToTile(latitude: number, longitude: number, zoom: number): TilePoint {
  const [x, y] = world(latitude, longitude, zoom);
  const tileX = Math.max(0, Math.floor(x));
  const tileY = Math.max(0, Math.floor(y));
  return {
    zoom,
    x: tileX,
    y: tileY,
    pixelX: clampPixel((x - tileX) * 256),
    pixelY: clampPixel((y - tileY) * 256),
  };
}

export function tileWindow(
  latitude: number,
  longitude: number,
  zoom: number,
  spanX: number,
  spanY: number,
): TileWindow {
  const [x, y] = world(latitude, longitude, zoom);
  const limitX = Math.max(0, 2 ** zoom - spanX);
  const limitY = Math.max(0, 2 ** zoom - spanY);
  const originX = clamp(roundHalfAwayFromZero(x - spanX / 2), 0, limitX);
  const originY = clamp(roundHalfAwayFromZero(y - spanY / 2), 0, limitY);
  return {
    zoom,
    originX,
    originY,
    spanX,
    spanY,
    pinX: Math.max(0, roundHalfAwayFromZero((x - originX) * 256)),
    pinY: Math.max(0, roundHalfAwayFromZero((y - originY) * 256)),
  };
}

export function chartPoints(
  values: number[],
  columnWidth: number,
  height: number,
  pad = 8,
): Array<{ x: number; y: number }> {
  if (values.length === 0) return [];
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  return values.map((value, index) => ({
    x: columnWidth * index + columnWidth / 2,
    y: pad + (1 - (value - min) / span) * (height - pad * 2),
  }));
}

export function rangePlacement(
  low: number,
  high: number,
  scaleMin: number,
  scaleMax: number,
): { left: number; width: number } {
  const span = scaleMax - scaleMin || 1;
  const left = clamp(((low - scaleMin) / span) * 100, 0, 100);
  const right = clamp(((high - scaleMin) / span) * 100, 0, 100);
  return { left, width: Math.max(right - left, high === low ? 2 : 0) };
}

function radarFrame(offsetMinutes: number, now: Date): RadarFrame {
  return {
    offsetMinutes,
    layer: radarLayer(offsetMinutes, now),
    label: offsetMinutes === 0 ? "Latest scan" : `${offsetMinutes} min ago`,
  };
}

function world(latitude: number, longitude: number, zoom: number): [number, number] {
  const tiles = 2 ** zoom;
  const x = ((longitude + 180) / 360) * tiles;
  const latRad = (latitude * Math.PI) / 180;
  const y = ((1 - Math.log(Math.tan(latRad) + 1 / Math.cos(latRad)) / Math.PI) / 2) * tiles;
  return [x, y];
}

function clampPixel(value: number): number {
  return clamp(roundHalfAwayFromZero(value), 0, 255);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function clockHour(time: string): number | null {
  const hour = Number(time.slice(11, 13));
  return Number.isInteger(hour) ? hour : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function record(value: unknown, name: string): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`missing ${name}`);
  return value;
}

function requiredString(value: Record<string, unknown>, key: string): string {
  const item = value[key];
  if (typeof item !== "string" || item.length === 0) throw new Error(`missing ${key}`);
  return item;
}

function requiredNumber(value: Record<string, unknown>, key: string): number {
  const item = value[key];
  if (typeof item !== "number" || !Number.isFinite(item)) throw new Error(`missing ${key}`);
  return item;
}

function stringArray(value: Record<string, unknown>, key: string): string[] {
  const item = value[key];
  if (!Array.isArray(item)) throw new Error(`missing ${key}`);
  return item.map((entry) => {
    if (typeof entry !== "string") throw new Error(`bad ${key}`);
    return entry;
  });
}

function numberArray(value: Record<string, unknown>, key: string): number[] {
  const item = value[key];
  if (!Array.isArray(item)) throw new Error(`missing ${key}`);
  return item.map((entry) => {
    if (entry === null) return 0;
    if (typeof entry !== "number" || !Number.isFinite(entry)) throw new Error(`bad ${key}`);
    return entry;
  });
}

function optionalPrecipArray(
  value: Record<string, unknown>,
  key: string,
  length: number,
): Array<number | null> {
  const item = value[key];
  if (!Array.isArray(item)) return Array.from({ length }, () => null);
  if (item.length !== length) throw new Error(`bad ${key}`);
  return item.map((entry) => {
    if (entry === null) return null;
    if (typeof entry !== "number" || !Number.isFinite(entry)) throw new Error(`bad ${key}`);
    return roundHalfAwayFromZero(entry);
  });
}

function optionalNumberArray(
  value: Record<string, unknown>,
  key: string,
  length: number,
): Array<number | null> {
  const item = value[key];
  if (!Array.isArray(item) || item.length !== length) return Array.from({ length }, () => null);
  return item.map((entry) => {
    if (entry === null) return null;
    if (typeof entry !== "number" || !Number.isFinite(entry)) return null;
    return entry;
  });
}

function optionalStringArray(
  value: Record<string, unknown>,
  key: string,
  length: number,
): Array<string | null> {
  const item = value[key];
  if (!Array.isArray(item) || item.length !== length) return Array.from({ length }, () => null);
  return item.map((entry) => (typeof entry === "string" && entry.length > 0 ? entry : null));
}

function text(value: Record<string, unknown>, key: string): string {
  const item = value[key];
  return typeof item === "string" ? item : "";
}
