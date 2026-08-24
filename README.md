# PasteMaxx

A lightweight, liquid-glass clipboard manager for macOS. Press **⌘⇧V** and a glass bar fades up from the bottom of your screen with everything you've copied — text, rich text, screenshots, images, and Finder files. Select one and it's pasted straight into the app you were using.

Built with Tauri v2 (Rust) + Svelte 5. macOS only. All history stays on your machine.

## Install

**Prerequisites** (skip any you already have):

```sh
xcode-select --install                                            # Xcode Command Line Tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh    # Rust
brew install node                                                 # Node.js (or nodejs.org)
```

**Clone, build, and install the app:**

```sh
git clone https://github.com/alexqise/pastemaxx.git
cd pastemaxx
npm install
npm run tauri build -- --bundles app

cp -R src-tauri/target/release/bundle/macos/PasteMaxx.app /Applications/
open /Applications/PasteMaxx.app
```

**First run:**

1. macOS will ask for **Accessibility** permission (System Settings → Privacy & Security → Accessibility). Grant it to PasteMaxx — it's only used to synthesize ⌘V so selecting an item can auto-paste. Without it, selecting an item still copies it; you just press ⌘V yourself.
2. That's it. PasteMaxx lives in your **menu bar** (no Dock icon) and starts at login automatically — both are controlled from the menu bar icon.

## Usage

| Keys | Action |
| --- | --- |
| **⌘⇧V** | Toggle the clipboard bar |
| **← →** | Navigate items |
| **Enter** | Paste selected item into the app you were in |
| **⌘1–9** | Paste the nth item directly |
| *just type* | Filter by text content / file name |
| **⌘P** or right-click | Pin / unpin item |
| **⌘⌫** | Delete item |
| **Esc** / click outside | Hide the bar |

The bar stays open after a paste so you can paste several items in a row. It appears on whichever display and Space you're on — including over fullscreen apps.

## Features

- **Captures everything** — plain text, rich text (RTF/HTML preserved and re-pasted with formatting), images/screenshots, and Finder file copies, each tagged with the source app's icon.
- **Pinning** — pinned items sit at the front of the bar and survive eviction and Clear History.
- **Persistent, deduplicated history** — SQLite in `~/Library/Application Support/com.alexqi.pastemaxx/`; re-copying something bumps the existing entry instead of duplicating it. Oldest unpinned items are evicted past a 500 MB disk budget.
- **Native liquid glass** — real `NSVisualEffectView` vibrancy under a transparent webview, with the whole window sliding up from below as one piece.

## Development

```sh
npm install
npm run tauri dev      # dev app with hot reload
npm run tauri build    # release .app (+ .dmg) in src-tauri/target/release/bundle/
```

Dev/debug and release builds are separate binaries, so each needs its own Accessibility grant — and because builds are ad-hoc signed, macOS may ask you to re-grant after rebuilding.

### Things to know

- **Hotkey conflict**: ⌘⇧V is "paste without formatting" in some apps (Google Docs, Slack). The global registration takes precedence. Change the default in one place: `hotkey` in `src-tauri/src/lib.rs`.
- No sensitive-content filtering by design: everything copied is recorded, including passwords copied from password managers. History is local-only.
- The bar is a non-activating `NSPanel` (via `tauri-nspanel`): it takes keyboard input without activating the app, so it appears over fullscreen apps and never switches Spaces, and pastes are posted directly to the target process (`CGEventPostToPid`) — the bar keeps keyboard focus, so Enter-Enter-Enter multi-paste works.

## Architecture

```
src/                     Svelte 5 UI (the bar)
  App.svelte             layout, keyboard nav, search, events
  lib/ItemCard.svelte    one clipboard item
  lib/api.ts             typed invoke/event wrappers
src-tauri/src/
  lib.rs                 app setup: panel, tray, hotkey, vibrancy, watcher
  clipboard.rs           NSPasteboard poller (200 ms) + capture
  macos.rs               objc2 glue: pasteboard I/O, CGEvent ⌘V, AX, app icons
  db.rs                  SQLite schema, dedup, disk-budget eviction
  paste.rs               write-back + synth ⌘V to the target process
  window.rs              positioning, native slide/fade, blur handling
  tray.rs                menu bar icon (drawn in code) + menu
```

The clipboard watcher polls `NSPasteboard.changeCount` every 200 ms (the standard low-overhead approach — there is no clipboard-change notification API on macOS).

## License

MIT
