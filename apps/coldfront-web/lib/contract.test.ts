import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  appletRadarFrames,
  dayLabel,
  describe as describeCode,
  displayTemperature,
  displayWindMph,
  formatAsOf,
  hourLabel,
  iemTileUrl,
  isUsZip,
  latLonToTile,
  nextHours,
  parseAlerts,
  parseOpenMeteo,
  parseZippopotam,
  roundHalfAwayFromZero,
  tileWindow,
  webRadarFrames,
} from "./domain";

const fixtures = resolve(import.meta.dirname, "../../../fixtures");

function fixture(name: string): unknown {
  return JSON.parse(readFileSync(resolve(fixtures, name), "utf8"));
}

const contract = fixture("contract.json") as {
  place: {
    zip: string;
    city: string;
    state: string;
    state_abbreviation: string;
    latitude: number;
    longitude: number;
  };
  current: {
    time: string;
    temperature_c: number;
    apparent_c: number;
    humidity: number;
    wind_kmh: number;
    weather_code: number;
    label: string;
    icon: string;
  };
  hourly_count: number;
  daily_count: number;
  next_hour_time: string;
  next_hour_labels: string[];
  day_labels: string[];
  daily0: { max_c: number; min_c: number; pop: number; code: number };
  daily4_pop: number;
  display: { temp_f: number; apparent_f: number; temp_c: number; wind_mph: number };
  rounding: { input: number; away_from_zero: number };
  tile: { zoom: number; x: number; y: number; pixel_x: number; pixel_y: number; url: string };
  window: { zoom: number; span: number; origin_x: number; origin_y: number; pin_x: number; pin_y: number };
  web_frames: number;
  web_first: { offset_minutes: number; layer: string; label: string };
  web_last: { offset_minutes: number; layer: string; label: string };
  applet_frames: number;
  applet_first_offset: number;
  codes: Array<{ code: number; label: string; icon: string }>;
  zips: { valid: string[]; invalid: string[] };
};

describe("shared fixtures", () => {
  it("checks ZIP codes", () => {
    for (const zip of contract.zips.valid) expect(isUsZip(zip)).toBe(true);
    for (const zip of contract.zips.invalid) expect(isUsZip(zip)).toBe(false);
  });

  it("parses the Saint Paul forecast the same way as Rust", () => {
    const place = parseZippopotam(fixture("zippopotam-55115.json"));
    expect(place.zip).toBe(contract.place.zip);
    expect(place.city).toBe(contract.place.city);
    expect(place.state).toBe(contract.place.state);
    expect(place.stateAbbreviation).toBe(contract.place.state_abbreviation);
    expect(place.latitude).toBeCloseTo(contract.place.latitude, 3);
    expect(place.longitude).toBeCloseTo(contract.place.longitude, 3);

    const forecast = parseOpenMeteo(fixture("open-meteo-55115.json"));
    expect(forecast.timezone).toBe("America/Chicago");
    expect(forecast.current.time).toBe(contract.current.time);
    expect(forecast.current.temperatureC).toBeCloseTo(contract.current.temperature_c, 3);
    expect(forecast.current.apparentC).toBeCloseTo(contract.current.apparent_c, 3);
    expect(forecast.current.humidity).toBe(contract.current.humidity);
    expect(forecast.current.windKmh).toBeCloseTo(contract.current.wind_kmh, 3);
    expect(forecast.current.weatherCode).toBe(contract.current.weather_code);
    expect(forecast.hourly).toHaveLength(contract.hourly_count);
    expect(forecast.daily).toHaveLength(contract.daily_count);
    expect(describeCode(forecast.current.weatherCode)).toEqual({
      label: contract.current.label,
      icon: contract.current.icon,
    });

    const hours = nextHours(forecast, 24);
    expect(hours).toHaveLength(24);
    expect(hours[0].time).toBe(contract.next_hour_time);
    expect(hours.slice(0, 9).map((hour, index) => hourLabel(hour.time, index === 0))).toEqual(
      contract.next_hour_labels,
    );
    expect(formatAsOf(forecast.current.time)).toBe("4:45 PM");
    const today = forecast.current.time.slice(0, 10);
    expect(forecast.daily.map((day) => dayLabel(day.date, today))).toEqual(contract.day_labels);
    expect(forecast.daily[0].tempMaxC).toBeCloseTo(contract.daily0.max_c, 3);
    expect(forecast.daily[0].tempMinC).toBeCloseTo(contract.daily0.min_c, 3);
    expect(forecast.daily[0].precipProbability).toBe(contract.daily0.pop);
    expect(forecast.daily[0].weatherCode).toBe(contract.daily0.code);
    expect(forecast.daily[4].precipProbability).toBe(contract.daily4_pop);
    expect(displayTemperature(forecast.current.temperatureC, true)).toBe(contract.display.temp_f);
    expect(displayTemperature(forecast.current.apparentC, true)).toBe(contract.display.apparent_f);
    expect(displayTemperature(forecast.current.temperatureC, false)).toBe(contract.display.temp_c);
    expect(displayWindMph(forecast.current.windKmh)).toBe(contract.display.wind_mph);
    expect(roundHalfAwayFromZero(contract.rounding.input)).toBe(contract.rounding.away_from_zero);
  });

  it("maps weather codes", () => {
    for (const row of contract.codes) {
      expect(describeCode(row.code)).toEqual({ label: row.label, icon: row.icon });
    }
  });

  it("parses alerts", () => {
    expect(parseAlerts(fixture("nws-alerts-empty.json"))).toEqual([]);
    const alerts = parseAlerts(fixture("nws-alerts-wind.json"));
    expect(alerts).toHaveLength(1);
    expect(alerts[0].event).toBe("Wind Advisory");
    expect(alerts[0].severity).toBe("Moderate");
    expect(alerts[0].senderName).toBe("NWS Twin Cities MN");
    expect(alerts[0].instruction).toContain("high profile");
  });

  it("builds the same radar tiles as Rust", () => {
    const point = latLonToTile(contract.place.latitude, contract.place.longitude, contract.tile.zoom);
    expect(point.x).toBe(contract.tile.x);
    expect(point.y).toBe(contract.tile.y);
    expect(point.pixelX).toBe(contract.tile.pixel_x);
    expect(point.pixelY).toBe(contract.tile.pixel_y);
    expect(iemTileUrl("nexrad-n0q-900913", point.zoom, point.x, point.y)).toBe(contract.tile.url);
    const grid = tileWindow(
      contract.place.latitude,
      contract.place.longitude,
      contract.window.zoom,
      contract.window.span,
      contract.window.span,
    );
    expect(grid.originX).toBe(contract.window.origin_x);
    expect(grid.originY).toBe(contract.window.origin_y);
    expect(grid.pinX).toBe(contract.window.pin_x);
    expect(grid.pinY).toBe(contract.window.pin_y);
    const web = webRadarFrames();
    expect(web).toHaveLength(contract.web_frames);
    expect(web[0]).toMatchObject({
      offsetMinutes: contract.web_first.offset_minutes,
      layer: contract.web_first.layer,
      label: contract.web_first.label,
    });
    expect(web.at(-1)).toMatchObject({
      offsetMinutes: contract.web_last.offset_minutes,
      layer: contract.web_last.layer,
      label: contract.web_last.label,
    });
    const applet = appletRadarFrames();
    expect(applet).toHaveLength(contract.applet_frames);
    expect(applet[0].offsetMinutes).toBe(contract.applet_first_offset);
    expect(applet.at(-1)?.label).toBe("Latest scan");
  });
});
