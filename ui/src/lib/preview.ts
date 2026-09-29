// Fake search results for previewing the UI in a browser, without Windows.
// Only roughly like the real search: substring matching, and the
// calculator only knows one sum.
import type { ResultAction, SearchResult, SnippetEditing } from "./backend";

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
  ["snip", "Paste a snippet, or make one (also just type ;) · also /snippet, /snippets", "snippet"],
  ["note", "Save a quick note, or find one · also /n, /notes", "note"],
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
  if (q.startsWith(";")) return previewSnippets(q.slice(1).trim());
  const snip = /^\/(snip|snippets?)(\s|$)/.exec(q);
  if (snip) return previewSnippets(q.slice(snip[0].length).trim());
  const note = /^\/(notes?|n)(\s|$)/.exec(query.trimStart().toLowerCase());
  if (note) return previewNotes(query.trimStart().slice(note[0].length).trim());
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
  if (q.length >= 2) {
    results.push(...previewFiles(q, 3));
    results.push(...previewSnippets(q).filter((r) => r.kind === "Snippet" && r.id.startsWith("snip:")).slice(0, 2));
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

const SNIPPETS: [keyword: string, text: string][] = [
  ["addr", "12 Harbour Street\nWellington 6011\nNew Zealand"],
  ["email", "sam.taylor@example.com"],
  ["sig", "Cheers,\nSam\n\nSent {date} at {time}"],
];

function snippetActions(keyword: string): ResultAction[] {
  return [
    action("paste", "Paste"),
    action("copy", "Copy", "Ctrl+Enter"),
    action("editSnippet", "Edit", "Ctrl+E"),
    action("delete", "Delete", "Ctrl+Delete", `Delete the snippet ;${keyword}?`),
  ];
}

function previewSnippets(q: string): SearchResult[] {
  const words = q.split(/\s+/).filter(Boolean);
  const results = SNIPPETS.filter(([keyword, text]) =>
    words.every((w) => `;${keyword}\n${text}`.toLowerCase().includes(w)),
  ).map(([keyword, text]) =>
    result({
      id: `snip:${keyword}`,
      title: `;${keyword}`,
      subtitle: text.split(/\s+/).join(" "),
      kind: "Snippet",
      glyph: "snippet",
      highlights: keyword === q ? [[0, keyword.length + 1]] : [],
      actions: snippetActions(keyword),
      preview: { text, image: null },
    }),
  );
  results.sort((a, b) => Number(b.title === `;${q}`) - Number(a.title === `;${q}`));
  const exact = SNIPPETS.some(([keyword]) => keyword === q);
  if (!q || (!exact && /^\S+$/.test(q))) {
    results.push(
      result({
        id: `snipcmd:new:${q}`,
        title: q ? `New snippet ;${q}` : "New snippet",
        subtitle: q ? `Save text to paste with ;${q}` : "Save text to paste by typing ; and a keyword",
        kind: "Snippet",
        glyph: "add",
        actions: [action("editSnippet", "Create")],
      }),
    );
  }
  return results;
}

export function previewSnippetDraft(id: string): SnippetEditing {
  const placeholders = [
    { text: "{date}", meaning: "today's date" },
    { text: "{time}", meaning: "the time now" },
    { text: "{clipboard}", meaning: "what's on the clipboard" },
  ];
  const existing = SNIPPETS.find(([keyword]) => id === `snip:${keyword}`);
  const keyword = existing?.[0] ?? (id.startsWith("snipcmd:new:") ? id.slice(12) : "");
  return { draft: { id: existing ? 1 : null, keyword, text: existing?.[1] ?? "" }, placeholders };
}

const NOTES: [title: string, text: string, when: string][] = [
  ["Trip plan", "Trip plan\nBook the ferry to Picton\nPack the tent", "2 h ago"],
  ["buy milk", "buy milk", "yesterday"],
  ["Ideas for Grandium", "Ideas for Grandium\n- emoji picker\n- unit conversion", "3 days ago"],
];

function previewNotes(text: string): SearchResult[] {
  const q = text.toLowerCase();
  const found = NOTES.filter(([, body]) => body.toLowerCase().includes(q)).map(([title, body, when]) =>
    result({
      id: `note:${title}`,
      title,
      subtitle: when,
      kind: "Note",
      glyph: "note",
      highlights: q ? (matching(title, q) ?? []) : [],
      actions: [
        action("open", "Open"),
        action("copy", "Copy text", "Ctrl+Enter"),
        action("openLocation", "Open file location", "Ctrl+Shift+Enter"),
        action("delete", "Delete", "Ctrl+Delete", `Delete the note “${title}”? It goes to the Recycle Bin.`),
      ],
      preview: { text: body, image: null },
    }),
  );
  if (!text) {
    return [
      ...found,
      result({
        id: "notecmd:folder",
        title: "Open notes folder",
        subtitle: "C:\\Users\\you\\Documents\\Grandium\\Notes",
        kind: "Note",
        glyph: "folder",
        actions: [action("open", "Open")],
      }),
    ];
  }
  return [
    result({
      id: `notecmd:save:${text}`,
      title: `Save note “${text}”`,
      subtitle: "As a text file in your notes folder",
      kind: "Note",
      glyph: "add",
      actions: [action("save", "Save"), action("saveOpen", "Save and open", "Ctrl+Enter")],
    }),
    ...found,
  ];
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
