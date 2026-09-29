# Grandium

A keyboard launcher for Windows 11. Press **Alt+Space** and start typing to
launch apps, find files, do math, search the web, bring back anything you
copied, paste saved snippets and jot down quick notes.

🚧 **Early development.** See [PLAN.md](PLAN.md) for features, architecture and milestones.

## Download

Every push is built on Windows by GitHub Actions:

1. Open the **Actions** tab and click the latest **Build** run.
2. Under **Artifacts**, download **grandium-windows**.
3. Unzip it. It holds `Grandium-portable.exe` (just run it) and
   `Grandium_<version>_x64-setup.exe` (installs for your user).

The builds aren't code-signed yet, so the first time you run one, Windows
shows "Windows protected your PC". Click **More info → Run anyway**.

## Development

You need Node.js 22+, Rust (stable), and on Windows, the
[Tauri prerequisites](https://tauri.app/start/prerequisites/).

```sh
npm install
npm run tauri dev      # run the app with live reload (Windows)
npm run tauri build    # build the .exe and installer
npm run dev            # UI only, in a browser
npm run check          # type-check the UI
cargo test -p grandium-core
```

| Folder | What's in it |
|---|---|
| `crates/grandium-core` | Platform-independent logic, tested on any OS |
| `src-tauri` | The Windows app: window, hotkey, tray |
| `ui` | The interface (Svelte + TypeScript) |
