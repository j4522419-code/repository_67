// Thin wrapper over the Tauri backend. Outside Tauri (a plain browser, used
// for UI previews) it falls back to harmless stand-ins so the UI still renders.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface AppStatus {
  version: string;
  hotkey: string;
  hotkeyError: string | null;
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

  /** Called every time the launcher window is shown. */
  onShown(callback: () => void): Promise<UnlistenFn> {
    return inTauri ? listen("launcher-shown", callback) : Promise.resolve(() => {});
  },
};
