<p align="center">
  <img src="docs/avatar.svg" width="128" height="128" alt="TokenMonitor icon" />
</p>

<h1 align="center">TokenMonitor</h1>

<p align="center">
  <strong>Native macOS menu bar monitor for Claude Code and Codex usage</strong>
</p>

<p align="center">
  Tracks spend, tokens, models, and session activity directly from local usage logs.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%2013%2B-black?style=flat-square&logo=apple&logoColor=white" alt="macOS" />
  <img src="https://img.shields.io/badge/Tauri-v2-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2" />
  <img src="https://img.shields.io/badge/Svelte-5-FF3E00?style=flat-square&logo=svelte&logoColor=white" alt="Svelte 5" />
  <img src="https://img.shields.io/badge/Rust-native-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/local--first-no%20cloud-2F855A?style=flat-square" alt="Local first" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue?style=flat-square" alt="License" />
</p>

---

TokenMonitor is a local macOS menu bar app for monitoring Claude Code and Codex usage.

It reads JSONL session logs from disk, applies provider-specific pricing rules locally, and shows cost, token, model, burn-rate, and historical usage data in a compact desktop UI.

No API keys. No cloud sync. No external usage CLI dependency.

## Installation

### Build From Source

```bash
git clone https://github.com/Michael-OvO/TokenMonitor.git
cd TokenMonitor
npm install
npx tauri build
```

Bundle output:

```text
src-tauri/target/release/bundle/
```

### Development

```bash
npm install
npx tauri dev
```

The app runs as a menu bar utility. Click the tray icon to open the popover.

## Requirements

- macOS 13 or newer
- Existing Claude Code and/or Codex usage logs on disk
- Node.js 18+ and Rust toolchain only if you are building from source

## Features

- Native macOS menu bar app with popover UI
- Claude, Codex, and combined provider views
- Historical navigation for day, week, month, and year periods
- Real-time tray title showing today's spend, with an option to hide it
- Monthly calendar heatmap with provider-specific plan tracking
- Active-session footer with live burn-rate and 5-hour session cost context
- Per-model cost and token breakdowns with model hiding/filtering
- Stacked bar and line chart modes for cost-by-model exploration
- Background refresh with configurable cadence
- Launch-at-login support
- Currency display options for `USD`, `EUR`, `GBP`, `JPY`, and `CNY`
- Local parsing and in-memory caching for near-instant repeat views
- No runtime dependency on `ccusage` or any external usage CLI

## Data Sources

| Provider | Default path | Discovery behavior |
|---|---|---|
| Claude Code | `~/.claude/projects/**/*.jsonl` | Also checks `$CLAUDE_CONFIG_DIR/projects` when set |
| Codex CLI | `~/.codex/sessions/YYYY/MM/DD/*.jsonl` | Also respects `$CODEX_HOME/sessions` when set |

TokenMonitor works from usage data you already have on disk. If no logs are present yet, the app stays idle until Claude Code or Codex generates them.

## Pricing Accuracy

TokenMonitor is designed to answer the question people actually care about: "what did this usage really cost?"

For Anthropic models, cache traffic is not treated as a single bucket. TokenMonitor reads the cache-creation breakdown from Claude logs and distinguishes between 5-minute and 1-hour cache writes before pricing them.

For OpenAI and Codex models, cached-input traffic is accounted for separately from standard input and output usage. Reasoning output is folded into output billing where applicable.

That gives the app materially better pricing fidelity than dashboards that only total tokens and apply a flat per-model rate.

### Claude cache-write tiers

| Model | 5m Cache Write | 1h Cache Write | Difference |
|---|---:|---:|---:|
| Opus 4.6 | $6.25 / MTok | $10.00 / MTok | +60% |
| Sonnet 4.6 | $3.75 / MTok | $6.00 / MTok | +60% |
| Haiku 4.5 | $1.25 / MTok | $2.00 / MTok | +60% |

## Performance

- **Native Rust parser** caches parsed file entries by file stamp, so unchanged log files are not reparsed unnecessarily
- **Frontend payload cache** keys by provider, period, and offset for immediate repeat loads
- **Stale-while-revalidate fetch path** shows cached data instantly and refreshes silently in the background
- **Cache warming** preloads adjacent history windows and common periods so navigation feels immediate
- **Selective scanning strategy** reduces unnecessary file work for short-lived views

## Architecture

```mermaid
graph LR
    A["Claude logs<br/><sub>~/.claude/projects/**/*.jsonl</sub>"] --> B["Rust parser + pricing engine"]
    D["Codex logs<br/><sub>~/.codex/sessions/YYYY/MM/DD/*.jsonl</sub>"] --> B
    B --> C["Tauri IPC layer"]
    C --> E["Svelte 5 desktop UI"]
    C --> F["Menu bar title updater"]
    B --> G["In-memory query + file caches"]
```

### Runtime flow

1. The UI requests a provider, period, and optional historical offset through Tauri IPC.
2. The Rust backend scans relevant JSONL logs, normalizes provider-specific usage events, and prices each entry locally.
3. Aggregated payloads are cached in memory for fast repeat requests.
4. The frontend renders metrics, charts, model summaries, calendar views, and live footer state.
5. A background loop refreshes the tray title and emits update events on the configured interval.

### Parsing behavior

- Claude parsing skips non-assistant entries and intermediate streaming chunks
- Codex parsing uses the final cumulative `token_count` event per session file
- Cross-provider merge mode preserves period semantics and combines provider totals into a single payload
- Historical navigation is offset-based, which keeps the UI simple while letting the backend stay date-aware

## Validation

```bash
./node_modules/.bin/tsc --noEmit
npm test -- --run
npm run build
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Convenience command:

```bash
npm run test:all
```

## Project Structure

```text
TokenMonitor/
├── src/
│   ├── App.svelte
│   └── lib/
│       ├── bootstrap.ts
│       ├── components/
│       ├── stores/
│       ├── types/
│       └── utils/
├── src-tauri/
│   └── src/
│       ├── commands.rs
│       ├── lib.rs
│       ├── models.rs
│       ├── parser.rs
│       └── pricing.rs
├── docs/
├── DEVELOPMENT.md
├── package.json
└── README.md
```

## Tech Stack

| Layer | Technology |
|---|---|
| Desktop shell | [Tauri v2](https://v2.tauri.app/) |
| Frontend | [Svelte 5](https://svelte.dev/) + TypeScript |
| Backend | Rust |
| Build tool | [Vite 6](https://vitejs.dev/) |
| State path | Local JSONL parsing + Tauri IPC + Svelte stores |

## Contributing

Issues and pull requests are welcome, especially around:

- new provider support
- pricing-model accuracy
- performance on large local histories
- UI polish and visualization improvements
- packaging and distribution

If you use Claude Code or Codex heavily, this repo is intended to be a practical, inspectable foundation for local usage observability.

## License

Licensed under the [GNU General Public License v3.0](LICENSE).
