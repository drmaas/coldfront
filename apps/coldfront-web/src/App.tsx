import { useEffect, useMemo, useState } from "react";
import {
  chartPoints,
  dayLabel,
  describe,
  displayIcon,
  displayTemperature,
  displayWindMph,
  formatAsOf,
  formatClockTime,
  formatPrecipAmountMm,
  hourLabel,
  nextHours,
  parseAlerts,
  parseOpenMeteo,
  parseZippopotam,
  rangePlacement,
  roundHalfAwayFromZero,
  type Alert,
  type Day,
  type Forecast,
  type Place,
} from "../lib/domain";
import { parseRoutePath, placeFromCoordinates, type RouteTarget } from "../lib/location";
import { FORECAST_DAY_DETAILS_ID, dayRowButtonA11y } from "./forecastDayA11y";
import { ColdFront, PrecipDrop, WeatherIcon, Wordmark } from "./icons";
import { LocationSearch } from "./LocationSearch";
import { RadarMap } from "./RadarMap";
import { readStoredPlace, writeStoredPlace } from "./locationStorage";
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
  const route = useMemo(() => parseRoutePath(path), [path]);
  return (
    <>
      {route.kind === "home" ? (
        <Landing theme={theme} onTheme={setTheme} />
      ) : (
        <ForecastPage route={route} units={units} onUnits={setUnits} theme={theme} onTheme={setTheme} />
      )}
      <footer className="site-footer">
        Forecast data by Open-Meteo. Radar tiles by Iowa Environmental Mesonet, NOAA NEXRAD. Alerts by the National
        Weather Service. MapTiler basemap when a key is configured.
      </footer>
    </>
  );
}

function Landing({ theme, onTheme }: { theme: ThemePreference; onTheme: (theme: ThemePreference) => void }) {
  const [error, setError] = useState("");
  return (
    <main className="landing">
      <div className="landing-theme">
        <ThemeToggle theme={theme} onTheme={onTheme} />
      </div>
      <Wordmark />
      <LocationSearch
        className="landing-form"
        inputClassName="hero-zip"
        placeholder="ZIP or city"
        submitLabel="Get forecast"
        error={error}
        onNavigate={go}
        onError={setError}
      />
      <p className="subtitle">
        Current conditions, the next 24 hours, and a 10-day forecast from Open-Meteo, with live NEXRAD radar and active
        NWS alerts.
      </p>
    </main>
  );
}

function ForecastPage({
  route,
  units,
  onUnits,
  theme,
  onTheme,
}: {
  route: Exclude<RouteTarget, { kind: "home" }>;
  units: Units;
  onUnits: (units: Units) => void;
  theme: ThemePreference;
  onTheme: (theme: ThemePreference) => void;
}) {
  const initialDraft = route.kind === "zip" ? route.zip : "";
  const [draft, setDraft] = useState(initialDraft);
  const [data, setData] = useState<ForecastData | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setDraft(route.kind === "zip" ? route.zip : "");
    const controller = new AbortController();
    setLoading(true);
    setError("");
    void resolveForecastRoute(route, controller.signal)
      .then((next) => {
        if (!controller.signal.aborted) {
          writeStoredPlace(next.place);
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
  }, [route]);

  return (
    <main className="forecast-page">
      <header className="topbar">
        <a className="brand" href="/" onClick={link("/")}>
          <Wordmark />
        </a>
        <LocationSearch
          className="zip-form location-topbar"
          initialValue={draft}
          submitLabel="Go"
          onNavigate={go}
          onError={setError}
        />
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
  const [selectedDay, setSelectedDay] = useState<number | null>(null);
  const { forecast, place } = data;
  useEffect(() => setSelectedDay(null), [forecast.current.time]);
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
  const currentIcon = displayIcon(condition.icon, current.isDay);
  const placeLine = formatPlaceLine(place);

  return (
    <div className="forecast-grid">
      <section className="conditions">
        <p className="place">{placeLine}</p>
        <div className="now">
          <WeatherIcon name={currentIcon} />
          <p className="temp">
            {displayTemperature(current.temperatureC, fahrenheit)}
            <span>°</span>
          </p>
          <p className="condition">{condition.label}</p>
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
                const pop = hour.precipProbability;
                return (
                  <div className="hour" key={hour.time}>
                    <WeatherIcon name={displayIcon(info.icon, hour.isDay)} />
                    <span className="hour-temp">{temps[index]}°</span>
                    {pop !== null ? (
                      <span className={`hour-pop${pop <= 5 ? " hour-pop-low" : ""}`}>
                        <PrecipDrop />
                        {pop}%
                      </span>
                    ) : (
                      <span className="hour-pop hour-pop-empty" aria-hidden="true" />
                    )}
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
        <ol className="days" aria-label="10-day forecast">
          {forecast.daily.map((day, index) => {
            const info = describe(day.weatherCode);
            const low = displayTemperature(day.tempMinC, fahrenheit);
            const high = displayTemperature(day.tempMaxC, fahrenheit);
            const bar = rangePlacement(low, high, scaleMin, scaleMax);
            const selected = selectedDay === index;
            return (
              <li key={day.date}>
                <button
                  type="button"
                  className={`day-row${selected ? " selected" : ""}`}
                  onClick={() => setSelectedDay((current) => (current === index ? null : index))}
                  {...dayRowButtonA11y(selected)}
                >
                  <span className="day-name">{dayLabel(day.date, today)}</span>
                  <WeatherIcon name={info.icon} />
                  <span className="day-desc">{info.label}</span>
                  <span className="pop">
                    <PrecipDrop />
                    {day.precipProbability}%
                  </span>
                  <span className="day-temp">{low}°</span>
                  <span className="range">
                    <span style={{ left: `${bar.left}%`, width: `${bar.width}%` }} />
                  </span>
                  <span className="day-temp high">{high}°</span>
                </button>
                {selected ? (
                  <DayDetails
                    day={day}
                    dayTitle={dayLabel(day.date, today)}
                    fahrenheit={fahrenheit}
                  />
                ) : null}
              </li>
            );
          })}
        </ol>
      </section>
      <RadarMap latitude={place.latitude} longitude={place.longitude} basemap={data.basemap} />
    </div>
  );
}

function formatPlaceLine(place: Place): string {
  const name = [place.city, place.stateAbbreviation].filter(Boolean).join(", ");
  if (name && place.zip) return `${name} ${place.zip}`;
  if (name) return name;
  if (place.zip) return place.zip;
  return `${place.latitude.toFixed(4)}, ${place.longitude.toFixed(4)}`;
}

function DayDetails({ day, dayTitle, fahrenheit }: { day: Day; dayTitle: string; fahrenheit: boolean }) {
  const info = describe(day.weatherCode);
  const rows: Array<{ label: string; value: string }> = [
    { label: "Conditions", value: info.label },
    { label: "Precip chance", value: `${day.precipProbability}%` },
  ];
  if (day.precipSumMm !== null && day.precipSumMm > 0) {
    rows.push({ label: "Precip amount", value: formatPrecipAmountMm(day.precipSumMm, fahrenheit) });
  }
  if (day.windMaxKmh !== null) {
    rows.push({
      label: "Max wind",
      value: fahrenheit
        ? `${displayWindMph(day.windMaxKmh)} mph`
        : `${roundHalfAwayFromZero(day.windMaxKmh)} km/h`,
    });
  }
  if (day.sunrise) {
    const formatted = formatClockTime(day.sunrise);
    if (formatted) rows.push({ label: "Sunrise", value: formatted });
  }
  if (day.sunset) {
    const formatted = formatClockTime(day.sunset);
    if (formatted) rows.push({ label: "Sunset", value: formatted });
  }
  if (day.humidityMean !== null) {
    rows.push({ label: "Humidity", value: `${day.humidityMean}%` });
  }
  return (
    <section className="day-details" id={FORECAST_DAY_DETAILS_ID} aria-live="polite">
      <h3>{dayTitle}</h3>
      <dl className="day-detail-grid">
        {rows.map((row) => (
          <div key={row.label}>
            <dt>{row.label}</dt>
            <dd>{row.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

async function resolveForecastRoute(
  route: Exclude<RouteTarget, { kind: "home" }>,
  signal: AbortSignal,
): Promise<ForecastData> {
  if (route.kind === "zip") {
    return loadForecastByZip(route.zip, signal);
  }
  const stored = readStoredPlace(route.latitude, route.longitude);
  const place = placeFromCoordinates(route.latitude, route.longitude, stored);
  return loadForecastForPlace(place, signal);
}

async function loadForecastByZip(zip: string, signal: AbortSignal): Promise<ForecastData> {
  const [placeResponse, configResponse] = await Promise.all([
    fetch(`/api/zip/${zip}`, { signal }),
    fetch("/api/config", { signal }),
  ]);
  if (placeResponse.status === 404) throw new Error("No place found for that ZIP code.");
  if (!placeResponse.ok) throw new Error("Could not look up that ZIP code.");
  const place = parseZippopotam(await placeResponse.json());
  return loadForecastForPlace(place, signal, await configResponse.json());
}

async function loadForecastForPlace(
  place: Place,
  signal: AbortSignal,
  configBody?: unknown,
): Promise<ForecastData> {
  const configResponse =
    configBody === undefined ? await fetch("/api/config", { signal }) : ({ ok: true, json: async () => configBody });
  if (!configResponse.ok) throw new Error("Could not load configuration.");
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