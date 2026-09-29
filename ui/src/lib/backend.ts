// Thin wrapper over the Tauri backend. Outside Tauri (a plain browser, used
// for UI previews) it falls back to stand-ins so the UI still renders.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { previewSearch, previewSnippetDraft } from "./preview";
import type { Theme } from "./theme";

export interface AppStatus {
  version: string;
  /** The key combination that opens Grandium, like "Alt+Space". */
  hotkey: string;
  /** Set when another app has that combination. */
  hotkeyError: string | null;
  /** Show the setup screen: on first run, or after installing. */
  setupNeeded: boolean;
  theme: Theme;
}

export interface Choice {
  id: string;
  name: string;
}

export interface SetupChoices {
  /** An installed browser's ID, or null for Windows' default browser. */
  browser: string | null;
  searchEngine: string;
  hotkey: string;
  theme: Theme;
  startWithWindows: boolean;
  clipboardMaxItems: number;
  clipboardMaxDays: number;
  /** Searched besides Desktop, Documents and Downloads. */
  extraFolders: string[];
  /** Kept out of search results; `key` is the result's ID. */
  hidden: { key: string; name: string }[];
}

export interface SetupOptions {
  browsers: Choice[];
  searchEngines: Choice[];
  hotkeys: string[];
  themes: Choice[];
  clipboardItems: number[];
  clipboardDays: number[];
  /** The folders file search always looks in, by name. */
  userFolders: string[];
  current: SetupChoices;
}

/** A snippet being made (`id` null) or changed on the editor screen. */
export interface SnippetDraft {
  id: number | null;
  keyword: string;
  text: string;
}

export interface SnippetEditing {
  draft: SnippetDraft;
  /** Like `{date}`, with what each stands for. */
  placeholders: { text: string; meaning: string }[];
}

export interface ResultAction {
  id: string;
  label: string;
  shortcut: string | null;
  /** When set, ask this question before running the action. */
  confirm: string | null;
}

/** A picture served by one of Grandium's URL schemes. */
export interface IconRef {
  scheme: string;
  key: string;
}

export interface Preview {
  text: string | null;
  image: IconRef | null;
}

/** After an action: "done" closed the launcher; "refresh" means search again. */
export type Outcome = "done" | "refresh";

export interface SearchResult {
  id: string;
  title: string;
  /** Smaller text after the title, like the folder `%temp%` stands for. */
  subtitle: string | null;
  kind: string;
  /** A picture to show as the icon: an app's icon or a copied image. */
  icon: IconRef | null;
  /** A built-in icon by name, for results that aren't apps. */
  glyph: string | null;
  /** `[start, end)` ranges of `title` to highlight. */
  highlights: [number, number][];
  /** The first action is what Enter does. */
  actions: ResultAction[];
  /** For slash commands: the text to put in the search box when picked. */
  fill: string | null;
  /** Shown below the list while the result is selected. */
  preview: Preview | null;
}

const inTauri = "__TAURI_INTERNALS__" in window;
let previewSetupDone = false;
let previewTheme: Theme = "system";

function previewStatus(): AppStatus {
  const params = new URLSearchParams(location.search);
  return {
    version: "dev",
    hotkey: "Alt+Space",
    hotkeyError: params.has("hotkeyError") ? "HotKey already registered" : null,
    setupNeeded: params.has("setup") && !previewSetupDone,
    theme: previewTheme,
  };
}

export const backend = {
  status(): Promise<AppStatus> {
    return inTauri ? invoke<AppStatus>("app_status") : Promise.resolve(previewStatus());
  },

  hide(): Promise<void> {
    return inTauri ? invoke("hide_launcher") : Promise.resolve();
  },

  setHeight(height: number): Promise<void> {
    return inTauri ? invoke("set_launcher_height", { height }) : Promise.resolve();
  },

  search(query: string): Promise<SearchResult[]> {
    return inTauri
      ? invoke<SearchResult[]>("search", { query })
      : Promise.resolve(previewSearch(query));
  },

  /** Runs one of a result's actions. Rejects with a message on failure. */
  runAction(id: string, action: string): Promise<Outcome> {
    return inTauri
      ? invoke<Outcome>("run_action", { id, action })
      : Promise.resolve(["pin", "unpin", "delete", "run"].includes(action) ? "refresh" : "done");
  },

  iconUrl(icon: IconRef): string | null {
    return inTauri ? convertFileSrc(icon.key, icon.scheme) : null;
  },

  setupOptions(): Promise<SetupOptions> {
    return inTauri
      ? invoke<SetupOptions>("setup_options")
      : Promise.resolve({
          browsers: [
            { id: "Brave", name: "Brave" },
            { id: "Google Chrome", name: "Google Chrome" },
            { id: "Microsoft Edge", name: "Microsoft Edge" },
            { id: "Firefox-308046B0AF4A39CB", name: "Mozilla Firefox" },
          ],
          searchEngines: [
            { id: "g", name: "Google" },
            { id: "b", name: "Bing" },
            { id: "ddg", name: "DuckDuckGo" },
            { id: "brave", name: "Brave Search" },
            { id: "ecosia", name: "Ecosia" },
          ],
          hotkeys: ["Alt+Space", "Ctrl+Space", "Ctrl+Alt+Space", "Alt+Shift+Space", "Ctrl+Shift+Space"],
          themes: [
            { id: "system", name: "Like Windows" },
            { id: "light", name: "Light" },
            { id: "dark", name: "Dark" },
          ],
          clipboardItems: [100, 500, 1000, 2000],
          clipboardDays: [1, 7, 30, 90, 365],
          userFolders: ["Desktop", "Documents", "Downloads"],
          current: {
            browser: null,
            searchEngine: "g",
            hotkey: "Alt+Space",
            theme: previewTheme,
            startWithWindows: true,
            clipboardMaxItems: 500,
            clipboardMaxDays: 30,
            extraFolders: new URLSearchParams(location.search).has("folders") ? ["D:\\Projects", "E:\\Music"] : [],
            hidden: new URLSearchParams(location.search).has("folders")
              ? [
                  { key: "app:Chrome Remote Desktop", name: "Chrome Remote Desktop" },
                  { key: "sys:signout", name: "Sign out" },
                ]
              : [],
          },
        });
  },

  /** Rejects with a message if a setting couldn't be put into effect. */
  saveSetup(choices: SetupChoices): Promise<void> {
    if (inTauri) return invoke("save_setup", { choices });
    previewSetupDone = true;
    previewTheme = choices.theme;
    return Promise.resolve();
  },

  /** Asks for a folder to add to file search; null if none was picked. */
  pickFolder(): Promise<string | null> {
    return inTauri ? invoke<string | null>("pick_folder") : Promise.resolve("C:\\Users\\you\\Pictures");
  },

  /** What the snippet editor starts with for a result (see `editSnippet` actions). */
  snippetDraft(id: string): Promise<SnippetEditing> {
    return inTauri
      ? invoke<SnippetEditing>("snippet_draft", { id })
      : Promise.resolve(previewSnippetDraft(id));
  },

  /** Saves a snippet and returns its keyword. Rejects with what's wrong with it. */
  saveSnippet(draft: SnippetDraft): Promise<string> {
    if (inTauri) return invoke<string>("save_snippet", { draft });
    if (!draft.keyword.replace(/^;/, "").trim()) return Promise.reject("Type a keyword, like “addr” to paste it with ;addr.");
    return Promise.resolve(draft.keyword.replace(/^;/, "").trim().toLowerCase());
  },

  /** Called when the tray's "Settings…" asks for the setup screen. */
  onShowSetup(callback: () => void): Promise<UnlistenFn> {
    if (inTauri) return listen("show-setup", callback);
    // Previews open settings with ?settings.
    if (new URLSearchParams(location.search).has("settings")) queueMicrotask(callback);
    return Promise.resolve(() => {});
  },

  /** Called every time the launcher window is shown. */
  onShown(callback: () => void): Promise<UnlistenFn> {
    return inTauri ? listen("launcher-shown", callback) : Promise.resolve(() => {});
  },
};
