// Fake search results for previewing the UI in a browser, without Windows.
import type { SearchResult } from "./backend";

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

export function previewSearch(query: string): SearchResult[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return APPS.flatMap((title) => {
    const start = title.toLowerCase().indexOf(q);
    if (start < 0) return [];
    return [
      {
        id: `app:${title}`,
        title,
        kind: "App",
        icon: null,
        highlights: [[start, start + q.length] as [number, number]],
        actions: [
          { id: "open", label: "Open", shortcut: "Enter" },
          { id: "openLocation", label: "Open file location", shortcut: "Ctrl+Enter" },
          { id: "runAsAdmin", label: "Run as administrator", shortcut: "Ctrl+Shift+Enter" },
        ],
      },
    ];
  });
}
