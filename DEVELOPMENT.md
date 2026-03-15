# TokenMonitor — Development Guide

## Prerequisites

- **Node.js** ≥ 18 (for Svelte frontend + ccusage)
- **Rust** toolchain via [rustup](https://rustup.rs/) (for Tauri backend)
- **Xcode Command Line Tools** — `xcode-select --install`

## Quick Start

```bash
# Install frontend dependencies
npm install

# Run in development mode (hot-reload frontend + debug Rust backend)
npx tauri dev
```

The app appears as a **menu bar icon** (no dock icon). Click it to open the popover.

## Project Structure

```
TokenMonitor/
├── src/                          # Svelte frontend (WebView)
│   ├── App.svelte                # Root component — layout orchestration
│   ├── app.css                   # Global styles, CSS variables, keyframes
│   ├── main.ts                   # Svelte mount point
│   └── lib/
│       ├── components/
│       │   ├── Toggle.svelte     # Claude/Codex provider switch
│       │   ├── TimeTabs.svelte   # 5H | Day | Week | Month tabs
│       │   ├── MetricsRow.svelte # Cost / Tokens / Sessions cards
│       │   ├── Chart.svelte      # Stacked bar chart + inline detail panel
│       │   ├── UsageBars.svelte  # Horizontal progress bars (5H view)
│       │   ├── ModelList.svelte  # Per-model cost/token rows
│       │   ├── Footer.svelte     # Live session indicator, timestamps
│       │   └── SetupScreen.svelte# First-launch install indicator
│       ├── stores/
│       │   └── usage.ts          # Svelte stores + IPC fetch logic
│       ├── types/
│       │   └── index.ts          # TypeScript interfaces (mirrors Rust structs)
│       └── utils/
│           └── format.ts         # Cost/token/time formatting + model colors
├── src-tauri/                    # Rust backend (Tauri)
│   ├── Cargo.toml
│   ├── tauri.conf.json           # Window, bundle, and app config
│   ├── Info.plist                # LSUIElement (no dock icon)
│   ├── capabilities/default.json # Permission grants
│   ├── icons/                    # Tray + app icons
│   └── src/
│       ├── main.rs               # Entry point
│       ├── lib.rs                # Tauri setup, tray icon, background polling
│       ├── ccusage.rs            # Auto-install, subprocess exec, 3-tier cache
│       ├── commands.rs           # IPC handlers + data transformation
│       └── models.rs             # Serde structs for ccusage JSON + frontend payload
├── package.json
├── vite.config.ts
└── svelte.config.js
```

## Development Workflow

### Running

```bash
# Full app (frontend + backend)
npx tauri dev

# Frontend only (for CSS/layout iteration without Rust rebuild)
npm run dev
# Then open http://localhost:1420 in a browser
```

### Rebuilding

Tauri dev mode hot-reloads the frontend automatically. Rust changes require a recompile (~2-4s incremental).

If the port is already in use from a previous run:

```bash
# Kill stale processes
pkill -f "token-monitor"; lsof -ti:1420 | xargs kill -9
```

### Checking Rust compilation without running

```bash
cd src-tauri && cargo check
```

### Building for production

```bash
npx tauri build
```

Output: `src-tauri/target/release/bundle/dmg/TokenMonitor_0.1.0_aarch64.dmg`

## Data Flow

```
ccusage CLI (Node.js subprocess)
    ↓ JSON stdout
Rust backend (ccusage.rs — 3-tier cache: memory → disk → subprocess)
    ↓ IPC invoke
Svelte frontend (stores/usage.ts → components)
```

### Cache Tiers

1. **In-memory HashMap** — nanoseconds, cleared on poll cycle
2. **Disk JSON** — `~/Library/Application Support/com.tokenmonitor.app/cache/*.json`
3. **CLI subprocess** — `node .../node_modules/.bin/ccusage daily --json` (~6-7s)

Cache TTL: 120 seconds. Background polling refreshes every 120s.

### ccusage Auto-Install

On first launch, the app runs `npm install --prefix ~/Library/Application Support/com.tokenmonitor.app/ ccusage @ccusage/codex`. Subsequent launches use the local install directly (no npx).

## ccusage Commands Used

| Tab   | Claude                                    | Codex                                      |
|-------|-------------------------------------------|--------------------------------------------|
| 5H    | `ccusage blocks --json --since {today}`   | `@ccusage/codex daily --json --since ...`  |
| Day   | `ccusage daily --json --since {-6d}`      | `@ccusage/codex daily --json --since ...`  |
| Week  | `ccusage weekly --json --since {-4w}`     | `@ccusage/codex daily --json --since ...`  |
| Month | `ccusage daily --json --since {-30d}`     | `@ccusage/codex daily --json --since ...`  |

Test manually:

```bash
npx ccusage@latest daily --json --since 20260301 | head -40
npx @ccusage/codex@latest daily --json --since 20260301 | head -40
```

## Troubleshooting

**Two tray icons appearing** — Kill all processes and restart: `pkill -f token-monitor`

**"Node.js not found" error** — Ensure `node` is on your PATH. The app checks `/usr/local/bin/node`, `/opt/homebrew/bin/node`, and `which node`.

**Blank popover / no data** — Check if ccusage installed correctly:
```bash
ls ~/Library/Application\ Support/com.tokenmonitor.app/node_modules/.bin/ccusage
```

**Stale data** — Clear the cache:
```bash
rm -rf ~/Library/Application\ Support/com.tokenmonitor.app/cache/
```
