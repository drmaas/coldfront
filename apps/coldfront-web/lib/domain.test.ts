import { describe, expect, it } from "vitest";
import { formatClockTime, parseOpenMeteo } from "./domain";

describe("parseOpenMeteo optional fields", () => {
  it("maps null hourly precipitation to null", () => {
    const forecast = parseOpenMeteo({
      latitude: 0,
      longitude: 0,
      timezone: "UTC",
      current: {
        time: "2026-11-17T12:00",
        temperature_2m: 10,
        apparent_temperature: 10,
        relative_humidity_2m: 50,
        wind_speed_10m: 5,
        weather_code: 1,
        is_day: 1,
      },
      hourly: {
        time: ["2026-11-17T12:00", "2026-11-17T13:00"],
        temperature_2m: [10, 11],
        weather_code: [1, 1],
        precipitation_probability: [25, null],
        is_day: [1, 1],
      },
      daily: {
        time: ["2026-11-17"],
        weather_code: [1],
        temperature_2m_max: [12],
        temperature_2m_min: [5],
        precipitation_probability_max: [10],
      },
    });
    expect(forecast.hourly[0].precipProbability).toBe(25);
    expect(forecast.hourly[1].precipProbability).toBeNull();
  });

  it("leaves daily detail fields null when absent", () => {
    const forecast = parseOpenMeteo({
      latitude: 0,
      longitude: 0,
      timezone: "UTC",
      current: {
        time: "2026-11-17T12:00",
        temperature_2m: 10,
        apparent_temperature: 10,
        relative_humidity_2m: 50,
        wind_speed_10m: 5,
        weather_code: 1,
        is_day: 1,
      },
      hourly: {
        time: ["2026-11-17T12:00"],
        temperature_2m: [10],
        weather_code: [1],
        precipitation_probability: [0],
        is_day: [1],
      },
      daily: {
        time: ["2026-11-17"],
        weather_code: [1],
        temperature_2m_max: [12],
        temperature_2m_min: [5],
        precipitation_probability_max: [10],
        sunrise: ["2026-11-17T07:12"],
      },
    });
    const day = forecast.daily[0];
    expect(day.sunrise).toBe("2026-11-17T07:12");
    expect(formatClockTime(day.sunrise!)).toBe("7:12 AM");
    expect(day.sunset).toBeNull();
    expect(day.precipSumMm).toBeNull();
    expect(day.windMaxKmh).toBeNull();
  });
});