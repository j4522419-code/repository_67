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

function action(
  id: string,
  label: string,
  shortcut: string | null = "Enter",
  confirm: string | null = null,
): ResultAction {
  return { id, label, shortcut, confirm };
}

function result(fields: Partial<SearchResult> & Pick<SearchResult, "id" | "title" | "kind">): SearchResult {
  return { subtitle: null, icon: null, glyph: null, highlights: [], actions: [], fill: null, ...fields };
}

function matching(title: string, q: string): [number, number][] | null {
  const start = title.toLowerCase().indexOf(q);
  return start < 0 ? null : [[start, start + q.length]];
}

const COMMANDS: [name: string, description: string, glyph: string][] = [
  ["google", "Search Google · also /g", "web"],
  ["youtube", "Search YouTube · also /yt", "web"],
  ["wiki", "Search Wikipedia · also /w, /wikipedia", "web"],
  ["calc", "Calculator · also /=", "calculator"],
  ["run", "Run a command or open a path, like Win+R", "run"],
  ["system", "Lock, sleep, restart, shut down… · also /sys", "shutdown"],
];

export function previewSearch(query: string): SearchResult[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  if (q.startsWith("/") && !/\s/.test(query.trim())) {
    return COMMANDS.filter(([name]) => name.startsWith(q.slice(1))).map(([name, subtitle, glyph]) =>
      result({
        id: `cmd:${name}`,
        title: `/${name}`,
        subtitle,
        kind: "Command",
        glyph,
        actions: [action("fill", "Choose")],
        fill: `/${name} `,
      }),
    );
  }
  const results: SearchResult[] = [];
  if (q === "%temp%") {
    results.push(
      result({
        id: "run:%temp%",
        title: "%temp%",
        subtitle: "C:\\Users\\you\\AppData\\Local\\Temp",
        kind: "Run",
        glyph: "folder",
        actions: [action("open", "Open"), action("runAsAdmin", "Run as administrator", "Ctrl+Shift+Enter")],
      }),
    );
  }

  if (q === "1200*12") {
    results.push(result({
      id: "calc:14400",
      title: "= 14,400",
      kind: "Calculator",
      icon: null,
      glyph: "calculator",
      highlights: [],
      actions: [action("copy", "Copy result")],
    }));
  }
  for (const title of APPS) {
    const highlights = matching(title, q);
    if (!highlights) continue;
    results.push(result({
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
        action("uninstall", "Uninstall", null, `Uninstall ${title}? This opens its uninstaller.`),
      ],
    }));
  }
  for (const [id, name, confirm] of SYSTEM) {
    const highlights = matching(name, q);
    if (!highlights) continue;
    results.push(result({
      id: `sys:${id}`,
      title: name,
      kind: "System",
      icon: null,
      glyph: id,
      highlights,
      actions: [action("run", name, "Enter", confirm)],
    }));
  }
  results.push(result({
    id: `web:g:${query.trim()}`,
    title: `Search Google for “${query.trim()}”`,
    kind: "Web",
    icon: null,
    glyph: "web",
    highlights: [],
    actions: [action("open", "Search")],
  }));
  return results.slice(0, 8);
}
