# Grandium: Plan

Grandium is a keyboard launcher for Windows 11. Press **Alt+Space** anywhere and
a search bar opens. From there you can launch apps, find files, do math, search
the web, run system commands, get back anything you copied, paste saved snippets
and jot down quick notes. It runs in the tray, starts with Windows and keeps all
data on your PC.

*Status: M0–M5 built (launcher, app search, calculator/web/system commands, clipboard history, file search, snippets and quick notes), plus slash commands, Run-box commands, and uninstalling apps. Waiting for testing on Windows. Last updated 2026-09-29.*

---

## 1. Decisions so far

| Topic | Decision |
|---|---|
| Name | **Grandium** |
| Target OS | **Windows 11** (x64) |
| Hotkey | **Alt+Space**. (Opening it with the Windows key alone was tried and dropped: Windows 11 still opened Start.) |
| Mouse | Hovering never changes the selection; only the arrow keys do. Clicking a row still runs it. |
| v1 scope | Apps, calculator, web search, system commands, clipboard history, file search, snippets, quick notes, settings |
| Stack | **Tauri 2**: a Rust backend with a Svelte + TypeScript UI |
| Delivery | GitHub Actions builds the `.exe` on Windows; tagged versions become GitHub Releases |
| Privacy | No accounts and no telemetry. Data stays in `%APPDATA%\Grandium` |

### Why Tauri
- **Small and fast.** The `.exe` is about 10 MB. The popup window is created once
  and kept hidden, so it appears instantly.
- **Looks native.** Windows 11 Mica/Acrylic blur, rounded corners, and the
  system's light or dark theme.
- **Rust core.** Fuzzy search, indexing and the clipboard database are fast and
  easy to test.
- **Previewable.** The UI is web tech, so it can be rendered and screenshotted
  during development without a Windows machine.
- WebView2 is built into Windows 11, so there's nothing extra to install.

**Alternative considered:** C# + WPF, which PowerToys Run and Flow Launcher use.
It's very native, but the output is bigger, and the UI can't be previewed from
a Linux build environment.

---

## 2. What using it looks like

```
              Alt+Space
┌────────────────────────────────────────┐
│ 🔍  budg█                              │
├────────────────────────────────────────┤
│ ▸ 📊 Budget 2026.xlsx      Documents    │   ← file
│   📋 "budget meeting 3pm"  Copied 2h    │   ← clipboard
│   📝 Budget ideas          Note         │   ← note
│   🧮 = 1200*12 → 14,400    Enter copies │   ← calculator
│   🟩 Excel                 App          │   ← app
└────────────────────────────────────────┘
  ↑↓ move · Enter open · Tab actions · Esc close
```

### Keyboard
| Key | What it does |
|---|---|
| `Alt+Space` | Show or hide Grandium |
| `↑` / `↓` | Move through results |
| `Enter` | Main action (open, launch, paste, copy) |
| `Ctrl+Enter` | Second action (open file location, copy only, …) |
| `Tab` | Menu of every action for the selected result |
| `Esc` | Cancel a confirmation, clear the text if there is any, otherwise hide |

### Slash commands (optional; plain typing searches everything)
Type `/` to see them all; Tab or Enter picks one.

| Command | What it does |
|---|---|
| *(none)* | Searches everything, mixed and ranked |
| `/google` `/g`, `/youtube` `/yt`, `/wiki` `/w` | Searches that site |
| `/calc` or `=` | Calculator only (math is also detected without it) |
| `/run` | Runs anything, like Win+R. Paths, `%temp%`-style variables, `shell:` links and program names like `regedit` also work without it. |
| `/system` `/sys` | System commands |
| `/clip` | Clipboard history (M3) |
| `/files` `/f` | Files and folders only. On its own, it lists your most recently changed files. |
| `;addr` or `/snip` | Snippets: `;addr` + Enter pastes the snippet with the keyword `addr`. `/snip` on its own lists them all. |
| `/note` `/n` | `/note buy milk` + Enter saves a note. `/note` on its own lists your notes, newest first. |

---

## 3. Features (v1)

### Apps
- Finds every installed app: Start Menu shortcuts plus Store/UWP apps (via `shell:AppsFolder`).
- Shows real app icons, cached on disk.
- Fuzzy matching, so `vsc` finds *Visual Studio Code*.
- **Frecency:** apps you open often and recently rank higher.
- Actions: launch, run as administrator, open file location.

### Calculator
- The answer shows up live while you type: `1200*12`, `(4+5)^2`, `15% of 80`, `sqrt(2)`.
- Enter copies the result.

### Web search
- Keywords `g`, `yt` and `w` open your default browser. Custom keywords can be added in settings.

### System commands
- Lock, sleep, restart, shut down, sign out, empty the Recycle Bin, open Settings.
- Anything destructive asks for confirmation first.

### Clipboard history
- `/clip` (or `/c`) opens it; typing searches it. The best couple of matches
  also show up in normal searches.
- Records copied text and images, removes duplicates and keeps them searchable.
- Stored in `%APPDATA%\Grandium\clipboard`, encrypted for your Windows account (DPAPI).
- Ctrl+P pins, Ctrl+Delete deletes; the selected entry shows a preview.
- **Enter** pastes into the app you were just using. **Ctrl+Enter** only copies.
- Pinned items are never deleted.
- **Safety:**
  - Skips anything a password manager marks as private.
  - Has a *Pause* toggle and a *Clear history* button.
  - Default limits are 500 items or 30 days, whichever comes first. Both can be changed.

### File search
- Indexes the names of files and folders in Desktop, Documents and Downloads,
  wherever Windows keeps them (OneDrive included). Adding more folders comes
  with settings (M6).
- Skips what Explorer hides and what nobody searches for: hidden and system
  files, `.git` and other dot-folders, `node_modules`, Office lock files
  (`~$…`) and unfinished downloads.
- Watches those folders (ReadDirectoryChangesW), so new, renamed and deleted
  files show up within a couple of seconds.
- Every word typed must appear in the name: `tax 2025` finds
  *2025 tax return.pdf*. Recently changed files, and files you open often,
  rank higher.
- Up to 3 files show up among everything else once you type 2 letters; `/files`
  shows up to 50.
- Actions: **Open** (Enter), **Open file location** (Ctrl+Enter), **Copy path**
  (Ctrl+Shift+C).
- Icons are Explorer's, shared per file type; thumbnails are never read, so
  OneDrive files that live only online aren't downloaded.

### Snippets
- Saved text pasted from the launcher: `;addr` + Enter pastes your address into
  the app you were using. Typing words from a snippet's text finds it too, and
  the best couple of matches show up in normal searches.
- Made and changed on an editor screen: pick **New snippet** (at the end of
  `/snip`, or offered when you type a `;keyword` that isn't taken), **Edit**
  (Ctrl+E) on a snippet, or **Save as snippet** (Ctrl+S) on copied text in
  clipboard history. Ctrl+Enter saves.
- Placeholders filled in when pasted: `{date}` and `{time}` (in your region's
  format) and `{clipboard}`.
- Actions: **Paste** (Enter), **Copy** (Ctrl+Enter), **Edit** (Ctrl+E),
  **Delete** (Ctrl+Delete, asks first).
- Kept in `%APPDATA%\Grandium\snippets.json`.
- *Note:* expanding snippets as you type in other apps needs a keyboard hook,
  which is planned for **after** v1 (see §8).

### Quick notes
- `/note buy milk` + Enter saves a note instantly; Ctrl+Enter saves and opens it.
- Notes are plain text files in `Documents\Grandium\Notes`, named after their
  first line, so you own them (and they sync if Documents is in OneDrive).
  `.md` files you put there count too.
- Searchable by title and text from the main bar; the selected note shows a
  preview. Edits made in any editor show up within a couple of seconds.
- Actions: **Open** in your default editor (Enter), **Copy text** (Ctrl+Enter),
  **Open file location** (Ctrl+Shift+Enter), **Delete** to the Recycle Bin
  (Ctrl+Delete, asks first).

### First-run setup
- On first run, after every install or update (the installer asks Grandium
  to), and later from the tray's **Settings…**, you choose:
  - the **browser** web searches and links open in (any installed browser,
    or Windows' default),
  - the **search engine** for everyday web searches (Google, Bing,
    DuckDuckGo, Brave Search or Ecosia).

### App shell and settings
- Tray icon (Open, Settings, Pause clipboard, Quit), start with Windows, and a
  single running instance.
- The settings window covers:
  - Hotkey
  - Theme (system, light or dark)
  - Indexed folders
  - Web keywords
  - Clipboard limits
  - Snippets
  - Startup
- A first-run welcome screen explains Alt+Space and the prefixes.

---

## 4. Architecture

```
┌──────────────── Svelte UI (WebView2) ────────────────┐
│ search box · result list · preview · settings window │
└───────────────▲──────────────────────┬───────────────┘
                │ results (events)     │ query / action (IPC)
┌───────────────┴──────────────────────▼───────────────┐
│ Query engine: parse prefix → fan out to providers →  │
│ fuzzy score + frecency → merge → top N               │
├──────────────────────────────────────────────────────┤
│ Providers: apps · files · calc · web · system ·      │
│            clipboard · snippets · notes              │
├──────────────────────────────────────────────────────┤
│ Storage: SQLite (history, usage, snippets, index) +  │
│          settings.json                               │
├──────────────────────────────────────────────────────┤
│ Windows layer: hotkey · tray · clipboard listener ·  │
│ app/icon discovery · launch · paste (SendInput)      │
└──────────────────────────────────────────────────────┘
```

- Each **provider** implements the same small interface:
  `query(text) → results` and `run(action)`.
- Each result has a title, subtitle, icon, score and a list of actions.
- Slow providers (files) send their results a little later. Every query has an
  id, so results for text you've already changed are discarded.
- **Platform-independent logic lives in its own crate** (`grandium-core`). That
  covers query parsing, ranking, the calculator, frecency and storage, and it
  can be tested on any OS. Windows-specific code is kept in one module.

### Repo layout
```
grandium/
├─ crates/grandium-core/     # pure Rust: engine, ranking, calc, storage (+ tests)
├─ src-tauri/                # the app: Tauri setup, providers, Windows layer
│  └─ src/platform/windows/  # hotkey, tray, clipboard, apps, icons, paste
├─ ui/                       # Svelte + TypeScript frontend
├─ .github/workflows/        # build.yml (every push), release.yml (tags)
├─ PLAN.md
└─ README.md
```

### Main libraries (subject to change)
| Need | Library |
|---|---|
| Hotkey, tray, autostart, single instance | Tauri 2 plugins (`global-shortcut`, `autostart`, `single-instance`) |
| Win32 APIs (clipboard listener, icons, SendInput, shell) | `windows` crate |
| Fuzzy matching | `nucleo-matcher` |
| Database | `rusqlite` (bundled SQLite) |
| Folder walking and watching | Rust's standard library + `ReadDirectoryChangesW` |
| Blur effect | Tauri window effects (Mica/Acrylic) |

### Performance targets
| What | Target |
|---|---|
| Popup visible after the hotkey | < 50 ms |
| Results after a keystroke (apps, calc, clipboard) | < 30 ms |
| Idle CPU | ~0% |
| Idle memory (including WebView2) | < 150 MB |

---

## 5. Building, testing, shipping

- **Every push:**
  - GitHub Actions on `windows-latest` runs the tests and builds the app.
  - It uploads the portable `grandium.exe` and the installer as downloadable artifacts.
  - A faster Linux job runs the core tests and the UI type checks.
- **Releases:** pushing a `v*` tag, like `v0.1.0`, publishes a GitHub Release
  with the installer and portable `.exe` attached.
- **Automated tests:** ranking, calculator, prefix parsing, frecency, clipboard
  de-duplication and snippet placeholders.
- **UI previews:** screenshots of the UI with fake data, rendered in a headless browser.
- **Manual check:** each milestone comes with a short checklist for you to try on your PC.

---

## 6. Milestones

Each milestone ends with a downloadable `.exe` that you try before we move on.

| # | Milestone | Done when |
|---|---|---|
| **M0** | **Foundation:** project skeleton, CI builds the `.exe`, tray icon, Alt+Space shows and hides a blurred search bar | You press Alt+Space, the bar appears with the cursor in it, and Esc hides it |
| **M1** | **Apps:** discovery, icons, fuzzy search, frecency, launch and run-as-admin | Typing `chr` puts Chrome on top and Enter opens it |
| **M2** | **Calculator, web search, system commands** | `=2^10` shows 1024, `yt lofi` opens YouTube, `lock` locks the PC |
| **M3** | **Clipboard history** | Copy 3 things, type `c`, and paste the first one into Notepad |
| **M4** | **File search** | A file saved to Downloads is findable within seconds |
| **M5** | **Snippets and quick notes** | `;addr` pastes your address, and `/note test` creates a note file |
| **M6** | **Settings and polish → v1.0** | Every setting works, the installer works, the first-run screen shows, and the release is published |

---

## 7. Risks and how we handle them

| Risk | Plan |
|---|---|
| Alt+Space is Windows' own shortcut for the window menu, and PowerToys Run / Command Palette use it too | Grandium takes it over. If the hotkey can't be registered, a tray notification says so and you can pick another one. Checked in M0. |
| Windows blocks the popup from getting keyboard focus (focus-stealing protection) | Use the standard workarounds launchers rely on. Checked in M0. |
| SmartScreen warns about an unsigned `.exe` | For now: *More info → Run anyway*. Code signing can be added later. |
| Antivirus false positives | v1 uses only standard APIs (RegisterHotKey, clipboard listener) and **no keyboard hooks**. |
| Passwords ending up in clipboard history | Honor the private-content flags password managers set; Pause and Clear buttons; local storage only. |
| Can't paste into apps running as administrator (a Windows security rule) | Copy to the clipboard instead and show a hint. |

---

## 8. After v1 (ideas backlog)

- **Snippets in any app:** type `;addr` anywhere and it expands. Needs a keyboard hook.
- **Window switcher:** search your open windows.
- **Whole-drive instant file search**, like *Everything*. Uses the NTFS index.
- **More conversions and pickers:**
  - Unit and currency conversion
  - Emoji picker
  - Color picker
- **Plugin system** for community extensions.
- **AI actions:** rewrite, summarize or translate selected text. Needs your API key.
- **Updates:** auto-update from GitHub Releases, plus code signing.
