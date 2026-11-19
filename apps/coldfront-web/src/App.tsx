import { useEffect, useMemo, useState } from "react";
import {
  chartPoints,
  dayLabel,
  describe,
  displayTemperature,
  displayWindMph,
  formatAsOf,
  hourLabel,
  isUsZip,
  nextHours,
  parseAlerts,
  parseOpenMeteo,
  parseZippopotam,
  rangePlacement,
  roundHalfAwayFromZero,
  type Alert,
  type Forecast,
  type Place,
} from "../lib/domain";
import { ColdFront, WeatherIcon, Wordmark } from "./icons";
import { RadarMap } from "./RadarMap";
import {
  applyResolvedTheme,
  readThemePreference,
  resolveTheme,
  writeThemePreference,
  type ThemePreference,
} from "./theme";

type Units = "F" | "C";

export function App() {
  const path = usePath();
  const [units, setUnits] = useUnits();
  const [theme, setTheme] = useTheme();
  const zip = path === "/" ? null : path.slice(1);
  return (
    <>
      {zip === null ? (
        <Landing theme={theme} onTheme={setTheme} />
      ) : (
        <ForecastPage
          zip={decodeURIComponent(zip)}
          units={units}
          onUnits={setUnits}
          theme={theme}
          onTheme={setTheme}
        />
      )}
      <footer className="site-footer">
        Forecast data by Open-Meteo. Radar tiles by Iowa Environmental Mesonet, NOAA NEXRAD. Alerts by the National
        Weather Service. MapTiler basemap when a key is configured.
      </footer>
    </>
  );
}

function Landing({ theme, onTheme }: { theme: ThemePreference; onTheme: (theme: ThemePreference) => void }) {
  const [zip, setZip] = useState("");
  const [error, setError] = useState("");
  return (
    <main className="landing">
      <div className="landing-theme">
        <ThemeToggle theme={theme} onTheme={onTheme} />
      </div>
      <Wordmark />
      <form
        className="landing-form"
        onSubmit={(event) => {
          event.preventDefault();
          const next = zip.trim();
          if (!isUsZip(next)) {
            setError("Enter a 5-digit US ZIP code.");
            return;
          }
          go(`/${next}`);
        }}
      >
        <input
          className="hero-zip"
          inputMode="numeric"
          autoComplete="postal-code"
          maxLength={5}
          placeholder="94110"
          aria-label="ZIP code"
          value={zip}
          onChange={(event) => {
            setZip(event.target.value);
            setError("");
          }}
        />
        <button className="primary" type="submit">
          Get forecast
        </button>
        {error && <p className="form-error">{error}</p>}
      </form>
      <p className="subtitle">
        Current conditions, the next 24 hours, and a 10-day forecast from Open-Meteo, with live NEXRAD radar and active
        NWS alerts.
      </p>
    </main>
  );
}

function ForecastPage({
  zip,
  units,
  onUnits,
  theme,
  onTheme,
}: {
  zip: string;
  units: Units;
  onUnits: (units: Units) => void;
  theme: ThemePreference;
  onTheme: (theme: ThemePreference) => void;
}) {
  const [draft, setDraft] = useState(zip);
  const [data, setData] = useState<ForecastData | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setDraft(zip);
    if (!isUsZip(zip)) {
      setLoading(false);
      setData(null);
      setError("Enter a 5-digit US ZIP code.");
      return undefined;
    }
    const controller = new AbortController();
    setLoading(true);
    setError("");
    void loadForecast(zip, controller.signal)
      .then((next) => {
        if (!controller.signal.aborted) {
          setData(next);
          setLoading(false);
        }
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setData(null);
        setLoading(false);
        setError(reason instanceof Error ? reason.message : "Could not load the forecast.");
      });
    return () => controller.abort();
  }, [zip]);

  return (
    <main className="forecast-page">
      <header className="topbar">
        <a className="brand" href="/" onClick={link("/")}>
          <Wordmark />
        </a>
        <form
          className="zip-form"
          onSubmit={(event) => {
            event.preventDefault();
            const next = draft.trim();
            if (isUsZip(next)) go(`/${next}`);
            else setError("Enter a 5-digit US ZIP code.");
          }}
        >
          <input aria-label="ZIP code" value={draft} maxLength={5} onChange={(event) => setDraft(event.target.value)} />
          <button className="primary" type="submit">
            Go
          </button>
        </form>
        <div className="units" role="group" aria-label="Temperature units">
          <button type="button" className={units === "F" ? "on" : ""} onClick={() => onUnits("F")}>
            °F
          </button>
          <button type="button" className={units === "C" ? "on" : ""} onClick={() => onUnits("C")}>
            °C
          </button>
        </div>
        <ThemeToggle theme={theme} onTheme={onTheme} />
      </header>
      {loading && <p className="status">Loading forecast…</p>}
      {error && !loading && <p className="status error">{error}</p>}
      {data && !loading && <ForecastBody data={data} units={units} />}
    </main>
  );
}

interface ForecastData {
  place: Place;
  forecast: Forecast;
  alerts: Alert[];
  basemap: "maptiler" | "none";
}

function ForecastBody({ data, units }: { data: ForecastData; units: Units }) {
  const fahrenheit = units === "F";
  const { forecast, place } = data;
  const hours = useMemo(() => nextHours(forecast, 24), [forecast]);
  const today = forecast.current.time.slice(0, 10);
  const temps = hours.map((hour) => displayTemperature(hour.temperatureC, fahrenheit));
  const points = chartPoints(temps, 64, 64);
  const chartWidth = Math.max(hours.length, 1) * 64;
  const path = points.map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(1)} ${point.y.toFixed(1)}`).join(" ");
  const lows = data.forecast.daily.map((day) => displayTemperature(day.tempMinC, fahrenheit));
  const highs = data.forecast.daily.map((day) => displayTemperature(day.tempMaxC, fahrenheit));
  const scaleMin = Math.min(...lows);
  const scaleMax = Math.max(...highs);
  const current = forecast.current;
  const todayDay = forecast.daily[0];
  const wind = fahrenheit
    ? `${displayWindMph(current.windKmh)} mph`
    : `${roundHalfAwayFromZero(current.windKmh)} km/h`;
  const condition = describe(current.weatherCode);

  return (
    <div className="forecast-grid">
      <section className="conditions">
        <p className="place">
          {place.city}, {place.stateAbbreviation} {place.zip}
        </p>
        <div className="now">
          <WeatherIcon name={condition.icon} />
          <div>
            <p className="temp">
              {displayTemperature(current.temperatureC, fahrenheit)}
              <span>°</span>
            </p>
            <p className="condition">{condition.label}</p>
          </div>
        </div>
        <ul className="facts">
          <li>Feels like {displayTemperature(current.apparentC, fahrenheit)}°</li>
          <li>
            High {todayDay ? displayTemperature(todayDay.tempMaxC, fahrenheit) : "–"}° · Low{" "}
            {todayDay ? displayTemperature(todayDay.tempMinC, fahrenheit) : "–"}°
          </li>
          <li>Wind {wind}</li>
          <li>Humidity {current.humidity}%</li>
        </ul>
        <p className="as-of">As of {formatAsOf(current.time)}</p>
        {data.alerts.length === 0 ? (
          <p className="alerts-empty">No active alerts</p>
        ) : (
          <ul className="alerts">
            {data.alerts.slice(0, 3).map((alert) => (
              <li key={alert.id || alert.headline}>
                <strong>{alert.event}</strong>
                <span>{alert.headline}</span>
              </li>
            ))}
          </ul>
        )}
        <ColdFront className="divider" />
        <h2>Next 24 hours</h2>
        <div className="hour-scroll">
          <div className="hour-strip" style={{ width: chartWidth }}>
            <div className="hour-row">
              {hours.map((hour, index) => {
                const info = describe(hour.weatherCode);
                return (
                  <div className="hour" key={hour.time}>
                    <WeatherIcon name={info.icon} />
                    <span className="hour-temp">{temps[index]}°</span>
                  </div>
                );
              })}
            </div>
            <svg className="hour-chart" viewBox={`0 0 ${chartWidth} 64`} role="img" aria-label="Hourly temperature">
              <path d={path} fill="none" stroke="var(--chart)" strokeWidth="2" />
              {points.map((point) => (
                <circle key={`${point.x}-${point.y}`} cx={point.x} cy={point.y} r="2.5" fill="var(--chart)" />
              ))}
            </svg>
            <div className="hour-row labels">
              {hours.map((hour, index) => (
                <span key={hour.time}>{hourLabel(hour.time, index === 0)}</span>
              ))}
            </div>
          </div>
        </div>
        <ColdFront className="divider" />
        <h2>10-day forecast</h2>
        <ol className="days">
          {forecast.daily.map((day) => {
            const info = describe(day.weatherCode);
            const low = displayTemperature(day.tempMinC, fahrenheit);
            const high = displayTemperature(day.tempMaxC, fahrenheit);
            const bar = rangePlacement(low, high, scaleMin, scaleMax);
            return (
              <li key={day.date}>
                <span className="day-name">{dayLabel(day.date, today)}</span>
                <WeatherIcon name={info.icon} />
                <span className="pop">{day.precipProbability}%</span>
                <span className="day-temp">{low}°</span>
                <span className="range">
                  <span style={{ left: `${bar.left}%`, width: `${bar.width}%` }} />
                </span>
                <span className="day-temp high">{high}°</span>
              </li>
            );
          })}
        </ol>
      </section>
      <RadarMap latitude={place.latitude} longitude={place.longitude} basemap={data.basemap} />
    </div>
  );
}

async function loadForecast(zip: string, signal: AbortSignal): Promise<ForecastData> {
  const [placeResponse, configResponse] = await Promise.all([
    fetch(`/api/zip/${zip}`, { signal }),
    fetch("/api/config", { signal }),
  ]);
  if (placeResponse.status === 404) throw new Error("No place found for that ZIP code.");
  if (!placeResponse.ok) throw new Error("Could not look up that ZIP code.");
  const place = parseZippopotam(await placeResponse.json());
  const config = (await configResponse.json()) as { basemap?: string };
  const query = `latitude=${place.latitude}&longitude=${place.longitude}`;
  const [forecastResponse, alertsResponse] = await Promise.all([
    fetch(`/api/forecast?${query}`, { signal }),
    fetch(`/api/alerts?${query}`, { signal }),
  ]);
  if (!forecastResponse.ok) throw new Error("The forecast service did not respond.");
  const forecast = parseOpenMeteo(await forecastResponse.json());
  const alerts = alertsResponse.ok ? parseAlerts(await alertsResponse.json()) : [];
  return { place, forecast, alerts, basemap: config.basemap === "maptiler" ? "maptiler" : "none" };
}

function usePath(): string {
  const [path, setPath] = useState(() => window.location.pathname);
  useEffect(() => {
    const onPop = () => setPath(window.location.pathname);
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);
  return path.length > 1 && path.endsWith("/") ? path.slice(0, -1) : path;
}

function useUnits(): [Units, (units: Units) => void] {
  const [units, setUnits] = useState<Units>(() => (window.localStorage.getItem("coldfront-units") === "C" ? "C" : "F"));
  useEffect(() => {
    window.localStorage.setItem("coldfront-units", units);
  }, [units]);
  return [units, setUnits];
}

function useTheme(): [ThemePreference, (theme: ThemePreference) => void] {
  const [theme, setTheme] = useState<ThemePreference>(() => readThemePreference());
  useEffect(() => {
    writeThemePreference(theme);
    applyResolvedTheme(resolveTheme(theme));
    if (theme !== "system") return undefined;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyResolvedTheme(resolveTheme("system"));
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [theme]);
  return [theme, setTheme];
}

function ThemeToggle({ theme, onTheme }: { theme: ThemePreference; onTheme: (theme: ThemePreference) => void }) {
  return (
    <div className="segmented" role="group" aria-label="Color theme">
      <button type="button" className={theme === "light" ? "on" : ""} onClick={() => onTheme("light")}>
        Light
      </button>
      <button type="button" className={theme === "dark" ? "on" : ""} onClick={() => onTheme("dark")}>
        Dark
      </button>
      <button type="button" className={theme === "system" ? "on" : ""} onClick={() => onTheme("system")}>
        Auto
      </button>
    </div>
  );
}

function go(path: string) {
  window.history.pushState(null, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

function link(path: string) {
  return (event: { preventDefault: () => void }) => {
    event.preventDefault();
    go(path);
  };
}
