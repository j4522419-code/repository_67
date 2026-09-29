// Thin wrapper over the Tauri backend. Outside Tauri (a plain browser, used
// for UI previews) it falls back to stand-ins so the UI still renders.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { previewSearch } from "./preview";

export interface AppStatus {
  version: string;
  /** The keys that open Grandium right now, e.g. ["Alt+Space"]. */
  keys: string[];
  /** Set when Alt+Space was chosen but another app has it. */
  hotkeyError: string | null;
  /** Set when the Windows key was chosen but couldn't be used. */
  windowsKeyError: string | null;
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

function previewStatus(): AppStatus {
  const params = new URLSearchParams(location.search);
  const hotkeyError = params.has("hotkeyError") ? "HotKey already registered" : null;
  const both = params.has("both");
  return {
    version: "dev",
    keys: [...(hotkeyError ? [] : ["Alt+Space"]), ...(both ? ["Windows key"] : [])],
    hotkeyError,
    windowsKeyError: null,
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

  /** Called every time the launcher window is shown. */
  onShown(callback: () => void): Promise<UnlistenFn> {
    return inTauri ? listen("launcher-shown", callback) : Promise.resolve(() => {});
  },
};
