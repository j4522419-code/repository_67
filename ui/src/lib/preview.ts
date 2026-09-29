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
  return {
    subtitle: null,
    icon: null,
    glyph: null,
    highlights: [],
    actions: [],
    fill: null,
    preview: null,
    ...fields,
  };
}

function matching(title: string, q: string): [number, number][] | null {
  const start = title.toLowerCase().indexOf(q);
  return start < 0 ? null : [[start, start + q.length]];
}

const COMMANDS: [name: string, description: string, glyph: string][] = [
  ["google", "Search Google · also /g", "web"],
  ["youtube", "Search YouTube · also /yt", "web"],
  ["wiki", "Search Wikipedia · also /w, /wikipedia", "web"],
  ["files", "Find files and folders · also /f, /file", "file"],
  ["calc", "Calculator · also /=", "calculator"],
  ["clip", "Clipboard history · also /clipboard, /c", "clipboard"],
  ["run", "Run a command or open a path, like Win+R", "run"],
  ["system", "Lock, sleep, restart, shut down… · also /sys", "shutdown"],
];

export function previewSearch(query: string): SearchResult[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  // Like the real search: a slash command is still being picked until a space.
  if (q.startsWith("/") && !/\s/.test(query.trimStart())) {
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
  if (q.startsWith("/clip")) return previewClipboard(q.slice(5).trim());
  const files = /^\/(files?|f)(\s|$)/.exec(q);
  if (files) return previewFiles(q.slice(files[0].length).trim(), 50);
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
  if (q.length >= 2) results.push(...previewFiles(q, 3));
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

// Newest first, like `/files` on its own lists them.
const FILES: [name: string, location: string, folder: boolean][] = [
  ["Budget 2026.xlsx", "Documents", false],
  ["Flight confirmation.pdf", "Downloads", false],
  ["Taxes", "Documents", true],
  ["2025 tax return.pdf", "Documents › Taxes", false],
  ["Old budget notes.docx", "Documents › Archive › 2024", false],
  ["setup-vscode-x64.exe", "Downloads", false],
  ["Screenshot 2026-09-28 101512.png", "Desktop", false],
];

function previewFiles(q: string, limit: number): SearchResult[] {
  const words = q.split(/\s+/).filter(Boolean);
  return FILES.filter(([name]) => words.every((w) => name.toLowerCase().includes(w)))
    .slice(0, limit)
    .map(([name, location, folder]) =>
      result({
        id: `file:${name}`,
        title: name,
        subtitle: location,
        kind: folder ? "Folder" : "File",
        glyph: folder ? "folder" : "file",
        highlights: words.length === 1 ? (matching(name, words[0]) ?? []) : [],
        actions: [
          action("open", "Open"),
          action("openLocation", "Open file location", "Ctrl+Enter"),
          action("copyPath", "Copy path", "Ctrl+Shift+C"),
        ],
      }),
    );
}

const COPIES: [text: string, when: string, pinned: boolean][] = [
  ["Home address: 12 Harbour Street, Wellington 6011", "yesterday", true],
  ["https://github.com/j4522419-code/repository_67", "just now", false],
  [
    "Meeting notes: budget review moved to Thursday 3pm. Bring the Q3 numbers and the new hiring plan.",
    "2 h ago",
    false,
  ],
  ["npm run tauri build", "5 min ago", false],
];

function previewClipboard(q: string): SearchResult[] {
  const results = COPIES.filter(([text]) => text.toLowerCase().includes(q)).map(
    ([text, when, pinned], i) =>
      result({
        id: `clip:${i}`,
        title: text,
        subtitle: pinned ? `Pinned · ${when}` : when,
        kind: "Clipboard",
        glyph: "clipboard",
        highlights: matching(text, q) ?? [],
        actions: [
          action("paste", "Paste"),
          action("copy", "Copy", "Ctrl+Enter"),
          action(pinned ? "unpin" : "pin", pinned ? "Unpin" : "Pin", "Ctrl+P"),
          action("delete", "Delete", "Ctrl+Delete"),
        ],
        preview: { text, image: null },
      }),
  );
  if (!q) {
    results.push(
      result({
        id: "clipcmd:pause",
        title: "Pause clipboard history",
        subtitle: "Stop saving new copies for now",
        kind: "Clipboard",
        glyph: "pause",
        actions: [action("run", "Pause")],
      }),
      result({
        id: "clipcmd:clear",
        title: "Clear clipboard history",
        subtitle: "Pinned items stay",
        kind: "Clipboard",
        glyph: "emptybin",
        actions: [action("run", "Clear", "Enter", "Delete everything in your clipboard history except pinned items?")],
      }),
    );
  }
  return results;
}
