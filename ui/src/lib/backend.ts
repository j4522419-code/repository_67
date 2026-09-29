// Thin wrapper over the Tauri backend. Outside Tauri (a plain browser, used
// for UI previews) it falls back to stand-ins so the UI still renders.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { previewSearch } from "./preview";

export interface AppStatus {
  version: string;
  hotkey: string;
  hotkeyError: string | null;
}

export interface ResultAction {
  id: string;
  label: string;
  shortcut: string | null;
}

export interface SearchResult {
  id: string;
  title: string;
  kind: string;
  /** Key for the `icon` URL scheme, see `iconUrl`. */
  icon: string | null;
  /** `[start, end)` ranges of `title` to highlight. */
  highlights: [number, number][];
  /** The first action is what Enter does. */
  actions: ResultAction[];
}

const inTauri = "__TAURI_INTERNALS__" in window;

function previewStatus(): AppStatus {
  const params = new URLSearchParams(location.search);
  return {
    version: "dev",
    hotkey: "Alt+Space",
    hotkeyError: params.has("hotkeyError") ? "HotKey already registered" : null,
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
  runAction(id: string, action: string): Promise<void> {
    return inTauri ? invoke("run_action", { id, action }) : Promise.resolve();
  },

  iconUrl(icon: string): string | null {
    return inTauri ? convertFileSrc(icon, "icon") : null;
  },

  /** Called every time the launcher window is shown. */
  onShown(callback: () => void): Promise<UnlistenFn> {
    return inTauri ? listen("launcher-shown", callback) : Promise.resolve(() => {});
  },
};
