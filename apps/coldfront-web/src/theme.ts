export type ThemePreference = "light" | "dark" | "system";

const STORAGE_KEY = "coldfront-theme";

export function readThemePreference(): ThemePreference {
  const value = window.localStorage.getItem(STORAGE_KEY);
  if (value === "light" || value === "dark" || value === "system") return value;
  return "system";
}

export function writeThemePreference(preference: ThemePreference): void {
  window.localStorage.setItem(STORAGE_KEY, preference);
}

export function resolveTheme(preference: ThemePreference): "light" | "dark" {
  if (preference === "light" || preference === "dark") return preference;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyResolvedTheme(resolved: "light" | "dark"): void {
  document.documentElement.dataset.theme = resolved;
}
