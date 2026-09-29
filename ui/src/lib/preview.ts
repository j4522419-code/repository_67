// Fake search results for previewing the UI in a browser, without Windows.
// Only roughly like the real search: substring matching, and the
// calculator only knows one sum.
import type { ResultAction, SearchResult } from "./backend";

const APPS = [
  "Google Chrome",
  "Chrome Remote Desktop",
  "Calculator",
  "Visual Studio Code",
  "Microsoft Edge",
  "Notepad",
  "Notepad++",
  "Windows Terminal",
  "Spotify",
  "File Explorer",
];

const SYSTEM: [id: string, name: string, confirm: string | null][] = [
  ["lock", "Lock", null],
  ["sleep", "Sleep", null],
  ["restart", "Restart", "Restart your PC now? Unsaved work in open apps may be lost."],
  ["shutdown", "Shut down", "Shut down your PC now? Unsaved work in open apps may be lost."],
];

function action(id: string, label: string, shortcut = "Enter", confirm: string | null = null): ResultAction {
  return { id, label, shortcut, confirm };
}

function matching(title: string, q: string): [number, number][] | null {
  const start = title.toLowerCase().indexOf(q);
  return start < 0 ? null : [[start, start + q.length]];
}

export function previewSearch(query: string): SearchResult[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const results: SearchResult[] = [];

  if (q === "1200*12") {
    results.push({
      id: "calc:14400",
      title: "= 14,400",
      kind: "Calculator",
      icon: null,
      glyph: "calculator",
      highlights: [],
      actions: [action("copy", "Copy result")],
    });
  }
  for (const title of APPS) {
    const highlights = matching(title, q);
    if (!highlights) continue;
    results.push({
      id: `app:${title}`,
      title,
      kind: "App",
      icon: null,
      glyph: null,
      highlights,
      actions: [
        action("open", "Open"),
        action("openLocation", "Open file location", "Ctrl+Enter"),
        action("runAsAdmin", "Run as administrator", "Ctrl+Shift+Enter"),
      ],
    });
  }
  for (const [id, name, confirm] of SYSTEM) {
    const highlights = matching(name, q);
    if (!highlights) continue;
    results.push({
      id: `sys:${id}`,
      title: name,
      kind: "System",
      icon: null,
      glyph: id,
      highlights,
      actions: [action("run", name, "Enter", confirm)],
    });
  }
  results.push({
    id: `web:g:${query.trim()}`,
    title: `Search Google for “${query.trim()}”`,
    kind: "Web",
    icon: null,
    glyph: "web",
    highlights: [],
    actions: [action("open", "Search")],
  });
  return results.slice(0, 8);
}
