import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  appletRadarFrames,
  chartPoints,
  dayLabel,
  describe as describeCode,
  displayIcon,
  displayTemperature,
  displayWindMph,
  formatAsOf,
  formatClockTime,
  hourLabel,
  iemTileUrl,
  isUsZip,
  latLonToTile,
  nextHours,
  parseAlerts,
  parseOpenMeteo,
  parseZippopotam,
  rangePlacement,
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
    is_day: boolean;
  };
  display_icons: {
    mainly_clear_day: string;
    clear_night: string;
    partly_cloudy_night: string;
    rain_night: string;
  };
  hourly_count: number;
  daily_count: number;
  next_hour_time: string;
  next_hour_labels: string[];
  day_labels: string[];
  daily0: {
    max_c: number;
    min_c: number;
    pop: number;
    code: number;
    precip_sum_mm: number;
    wind_max_kmh: number;
    sunrise: string;
    sunset: string;
    humidity_mean: number;
  };
  next_hour0_pop: number;
  daily4_pop: number;
  display: { temp_f: number; apparent_f: number; temp_c: number; wind_mph: number };
  rounding: { input: number; away_from_zero: number };
  chart: {
    values: number[];
    column_width: number;
    height: number;
    pad: number;
    points: Array<{ x: number; y: number }>;
    range: { low: number; high: number; scale_min: number; scale_max: number; left: number; width: number };
    equal_range: { low: number; high: number; scale_min: number; scale_max: number; left: number; width: number };
  };
  tile: { zoom: number; x: number; y: number; pixel_x: number; pixel_y: number; url: string };
  window: { zoom: number; span: number; origin_x: number; origin_y: number; pin_x: number; pin_y: number };
  radar_now: string;
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
    expect(forecast.current.isDay).toBe(contract.current.is_day);
    expect(displayIcon("mainly-clear", true)).toBe(contract.display_icons.mainly_clear_day);
    expect(displayIcon("clear", false)).toBe(contract.display_icons.clear_night);
    expect(displayIcon("partly-cloudy", false)).toBe(contract.display_icons.partly_cloudy_night);
    expect(displayIcon("rain", false)).toBe(contract.display_icons.rain_night);

    const hours = nextHours(forecast, 24);
    expect(hours).toHaveLength(24);
    expect(hours[0].time).toBe(contract.next_hour_time);
    expect(hours[0].precipProbability).toBe(contract.next_hour0_pop);
    expect(hours[0].isDay).toBe(true);
    expect(hours[1].isDay).toBe(false);
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
    expect(forecast.daily[0].precipSumMm).toBeCloseTo(contract.daily0.precip_sum_mm, 3);
    expect(forecast.daily[0].windMaxKmh).toBeCloseTo(contract.daily0.wind_max_kmh, 3);
    expect(formatClockTime(forecast.daily[0].sunrise!)).toBe(contract.daily0.sunrise);
    expect(formatClockTime(forecast.daily[0].sunset!)).toBe(contract.daily0.sunset);
    expect(forecast.daily[0].humidityMean).toBe(contract.daily0.humidity_mean);
    expect(forecast.daily[4].precipProbability).toBe(contract.daily4_pop);
    expect(displayTemperature(forecast.current.temperatureC, true)).toBe(contract.display.temp_f);
    expect(displayTemperature(forecast.current.apparentC, true)).toBe(contract.display.apparent_f);
    expect(displayTemperature(forecast.current.temperatureC, false)).toBe(contract.display.temp_c);
    expect(displayWindMph(forecast.current.windKmh)).toBe(contract.display.wind_mph);
    expect(roundHalfAwayFromZero(contract.rounding.input)).toBe(contract.rounding.away_from_zero);
    const points = chartPoints(
      contract.chart.values,
      contract.chart.column_width,
      contract.chart.height,
      contract.chart.pad,
    );
    expect(points).toHaveLength(contract.chart.points.length);
    for (const [index, point] of points.entries()) {
      expect(point.x).toBeCloseTo(contract.chart.points[index].x, 3);
      expect(point.y).toBeCloseTo(contract.chart.points[index].y, 3);
    }
    const bar = rangePlacement(
      contract.chart.range.low,
      contract.chart.range.high,
      contract.chart.range.scale_min,
      contract.chart.range.scale_max,
    );
    expect(bar.left).toBeCloseTo(contract.chart.range.left, 3);
    expect(bar.width).toBeCloseTo(contract.chart.range.width, 3);
    const equal = rangePlacement(
      contract.chart.equal_range.low,
      contract.chart.equal_range.high,
      contract.chart.equal_range.scale_min,
      contract.chart.equal_range.scale_max,
    );
    expect(equal.left).toBeCloseTo(contract.chart.equal_range.left, 3);
    expect(equal.width).toBeCloseTo(contract.chart.equal_range.width, 3);
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
    expect(iemTileUrl("q2-hsr-900913", point.zoom, point.x, point.y)).toBe(contract.tile.url);
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
    const now = new Date(contract.radar_now);
    const web = webRadarFrames(now);
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
    const applet = appletRadarFrames(now);
    expect(applet).toHaveLength(contract.applet_frames);
    expect(applet[0].offsetMinutes).toBe(contract.applet_first_offset);
    expect(applet.at(-1)?.label).toBe("Latest scan");
  });
});
