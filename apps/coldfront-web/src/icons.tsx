interface IconProps {
  name: string;
}

export function WeatherIcon({ name }: IconProps) {
  const rainy = name.includes("rain") || name === "drizzle" || name.includes("freezing");
  const snowy = name.includes("snow");
  const storm = name === "thunder";
  const cloudy = name === "overcast" || name === "fog" || name === "partly-cloudy" || rainy || snowy || storm;
  const sun = name === "clear" || name === "mainly-clear" || name === "partly-cloudy";
  return (
    <svg className="weather-icon" viewBox="0 0 32 32" aria-hidden="true">
      {sun && <circle cx={cloudy ? 11 : 16} cy={cloudy ? 12 : 16} r={6} fill="#f5b942" />}
      {sun && !cloudy && (
        <g stroke="#f5b942" strokeWidth="1.6" strokeLinecap="round">
          <path d="M16 4 v3 M16 25 v3 M4 16 h3 M25 16 h3 M7.5 7.5 l2 2 M22.5 22.5 l2 2 M24.5 7.5 l-2 2 M9.5 22.5 l-2 2" />
        </g>
      )}
      {cloudy && <path d="M10 22h13a5 5 0 0 0 .4-10 6.5 6.5 0 0 0-12.4-1.2A4.5 4.5 0 0 0 10 22z" fill={name === "fog" ? "#c5ccd4" : "#8aa0b8"} />}
      {rainy && <path d="M12 24 l-1.4 3 M17 24 l-1.4 3 M22 24 l-1.4 3" stroke="#5b8def" strokeWidth="1.4" strokeLinecap="round" />}
      {snowy && <path d="M13 24 v3 M17.5 24 v3 M22 24 v3 M11.8 25.2 h2.4 M16.3 25.2 h2.4 M20.8 25.2 h2.4" stroke="#d7e4f5" strokeWidth="1.2" strokeLinecap="round" />}
      {storm && <path d="M15 22 l-2 4 h3 l-2 4" fill="#f5b942" />}
    </svg>
  );
}

export function ColdFront({ className = "front" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 72 16" aria-hidden="true">
      <path d="M0 5 H72" stroke="#2563eb" strokeWidth="1.5" />
      <path d="M6 5 L12 14 L18 5 Z" fill="#2563eb" />
      <path d="M24 5 L30 14 L36 5 Z" fill="#2563eb" />
      <path d="M42 5 L48 14 L54 5 Z" fill="#2563eb" />
      <path d="M60 5 L66 14 L72 5 Z" fill="#2563eb" />
    </svg>
  );
}

export function Wordmark() {
  return (
    <div className="wordmark">
      <span>Coldfront</span>
      <ColdFront />
    </div>
  );
}
