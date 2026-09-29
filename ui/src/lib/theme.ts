export type Theme = "system" | "light" | "dark";

/** Colors come from app.css; "system" follows Windows' light or dark mode. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}
