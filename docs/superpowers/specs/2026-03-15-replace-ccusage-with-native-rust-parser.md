# Replace ccusage with Native Rust Parser

**Date:** 2026-03-15
**Status:** Draft
**Goal:** Eliminate the Node.js/ccusage subprocess dependency by replacing it with a pure Rust JSONL parser and pricing engine, so the bundled `.app` works on any Mac without requiring Node.js or npm.

---

## Problem

TokenMonitor currently shells out to `ccusage` (a Node.js CLI tool) to read Claude/Codex usage data. This means:

1. The bundled `.app` requires Node.js and npm installed on the host machine
2. First launch runs `npm install ccusage` — fails silently if npm is missing or network is down
3. Users who have Claude Code usage data but no Node.js see an empty dashboard with no error message
4. The 3-tier cache (memory → disk → subprocess) exists only because subprocess spawning is slow

## Solution

Replace `ccusage.rs` and `hourly.rs` with two new Rust modules:

- **`pricing.rs`** — hardcoded pricing table with fuzzy model matching
- **`parser.rs`** — JSONL file reader with daily/monthly/blocks/hourly aggregation

## Architecture

### New module: `pricing.rs`

A single public function:

```rust
pub fn calculate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> f64
```

Internally uses a lookup table matching model name patterns to per-token rates (all prices in $/MTok). The table is matched in order, most-specific patterns first (e.g., `opus-4-6` before `opus-4`).

**Version constant:** `pub const PRICING_VERSION: &str = "2026-03-15";` — identifies which pricing data is baked into this app version for debugging cost discrepancies.

**OpenAI cache token mapping:** OpenAI uses a single "Cached Input" discount rather than separate write/read rates. For OpenAI models: `cache_creation_tokens` are charged at the standard input rate, `cache_read_tokens` are charged at the "Cached Input" rate.

**Reasoning tokens:** For OpenAI o-series models, `reasoning_output_tokens` are billed at the output token rate. During parsing, reasoning tokens are folded into `output_tokens` in the `ParsedEntry`.

#### Claude Models (source: [Anthropic official pricing](https://platform.claude.com/docs/en/docs/about-claude/pricing))

| Model Pattern | Input | Output | Cache Write | Cache Read |
|---------------|-------|--------|-------------|------------|
| `opus-4-6` | 5.00 | 25.00 | 6.25 | 0.50 |
| `opus-4-5` | 5.00 | 25.00 | 6.25 | 0.50 |
| `opus-4-1` | 15.00 | 75.00 | 18.75 | 1.50 |
| `opus-4` (exact, no minor ver) | 15.00 | 75.00 | 18.75 | 1.50 |
| `sonnet-4-6` | 3.00 | 15.00 | 3.75 | 0.30 |
| `sonnet-4-5` | 3.00 | 15.00 | 3.75 | 0.30 |
| `sonnet-4` (includes 3.7) | 3.00 | 15.00 | 3.75 | 0.30 |
| `haiku-4-5` | 1.00 | 5.00 | 1.25 | 0.10 |
| `haiku-3-5` | 0.80 | 4.00 | 1.00 | 0.08 |
| `haiku-3` | 0.25 | 1.25 | 0.30 | 0.03 |

#### OpenAI/Codex Models (source: [OpenAI official pricing](https://platform.openai.com/docs/pricing))

| Model Pattern | Input | Output | Cached Input |
|---------------|-------|--------|--------------|
| `gpt-5.4` | 2.50 | 15.00 | 0.25 |
| `gpt-5.3-codex` | 1.75 | 14.00 | 0.175 |
| `gpt-5.2-codex` | 1.75 | 14.00 | 0.175 |
| `gpt-5.2` | 1.75 | 14.00 | 0.175 |
| `gpt-5.1-codex-max` | 1.25 | 10.00 | 0.125 |
| `gpt-5.1-codex` | 1.25 | 10.00 | 0.125 |
| `gpt-5-codex` | 1.25 | 10.00 | 0.125 |
| `gpt-5.1-codex-mini` | 0.25 | 2.00 | 0.025 |
| `codex-mini-latest` | 1.50 | 6.00 | 0.375 |
| `gpt-5.1` | 1.25 | 10.00 | 0.125 |
| `gpt-5` | 1.25 | 10.00 | 0.125 |
| `gpt-5-mini` | 0.25 | 2.00 | 0.025 |
| `gpt-5-nano` | 0.05 | 0.40 | 0.005 |
| `o4-mini` | 1.10 | 4.40 | 0.275 |
| `o3` (not o3-mini) | 2.00 | 8.00 | 0.50 |
| `o3-mini` | 1.10 | 4.40 | 0.55 |
| `o1` (not o1-mini) | 15.00 | 60.00 | 7.50 |
| `o1-mini` | 1.10 | 4.40 | 0.55 |

#### Fuzzy Fallback for Unknown Models

When a model name doesn't match any known pattern, extract the family and use the latest pricing:

| Family detected | Fallback pricing (input/output) |
|----------------|-------------------------------|
| Contains `opus` | $5.00 / $25.00 (latest Opus) |
| Contains `sonnet` | $3.00 / $15.00 (latest Sonnet) |
| Contains `haiku` | $1.00 / $5.00 (latest Haiku) |
| Contains `codex-mini` | $0.25 / $2.00 |
| Contains `codex` or `gpt-5` | $1.25 / $10.00 |
| Starts with `o` + digit | $1.10 / $4.40 (o4-mini) |
| Completely unknown | $3.00 / $15.00 (Sonnet, conservative middle) |

### New module: `parser.rs`

Replaces both `ccusage.rs` and `hourly.rs`. Reads JSONL files directly and exposes aggregation functions.

#### Data Sources

- **Claude:** `~/.claude/projects/**/*.jsonl` (recursive glob)
- **Codex:** `~/.codex/sessions/YYYY/MM/DD/*.jsonl` (date-structured directories)

#### Internal Normalized Entry

Every JSONL entry from either provider is parsed into:

```rust
struct ParsedEntry {
    timestamp: DateTime<Local>,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
}
```

Cost is calculated per-entry using `pricing::calculate_cost()`.

#### Public API

All functions return `UsagePayload` (the existing struct the frontend expects — no changes).

The `since` parameter uses `YYYYMMDD` format (e.g., `"20260315"`) to match the existing codebase convention. The parser filters entries with `timestamp >= since`.

`UsageParser` is `Send + Sync` — it is shared across Tauri command threads via `Arc` with internal mutability via `Mutex` for the cache. All public methods take `&self`.

```rust
impl UsageParser {
    /// Daily aggregation: group entries by date, sum tokens, apply pricing.
    pub fn get_daily(&self, provider: &str, since: &str) -> UsagePayload;

    /// Monthly aggregation: group entries by YYYY-MM.
    pub fn get_monthly(&self, provider: &str, since: &str) -> UsagePayload;

    /// 5-hour billing window detection with gap analysis.
    /// Scans entries, groups into contiguous activity windows separated by
    /// gaps (>30 min of no activity). Calculates burn_rate = cost / elapsed_hours,
    /// projection = burn_rate * 5.
    pub fn get_blocks(&self, provider: &str, since: &str) -> UsagePayload;

    /// Hourly distribution for today. Groups entries by hour.
    pub fn get_hourly(&self, provider: &str, since: &str) -> UsagePayload;

    /// Clear the in-memory cache.
    pub fn clear_cache(&self);
}
```

#### Period → Parser Method Dispatch

`commands.rs` translates the frontend's `period` parameter to the correct parser method and date range:

| Frontend period | Parser method | `since` value |
|----------------|---------------|---------------|
| `"5h"` | `get_blocks(provider, today)` | Today's date |
| `"day"` | `get_hourly(provider, today)` | Today's date |
| `"week"` | `get_daily(provider, week_start)` | Monday of current week |
| `"month"` | `get_daily(provider, month_start)` | 1st of current month |
| `"year"` | `get_monthly(provider, year_start)` | Jan 1st of current year |

#### Provider `"all"` Handling

When `provider = "all"`, `commands.rs` calls the parser twice (once for `"claude"`, once for `"codex"`) and merges the results using the existing `merge_payloads` helper (which deduplicates chart buckets by label, combines model breakdowns, and AND-s `from_cache`). The parser itself only handles single-provider queries.

#### Caching

Simple in-memory cache: `Mutex<HashMap<String, (UsagePayload, Instant)>>` with a 2-minute TTL (matching current `CACHE_MAX_AGE`). No disk cache needed — reading local JSONL files is milliseconds, not seconds like subprocess spawning.

The `from_cache` field in `UsagePayload` now means "served from the in-memory TTL cache" (no longer "from disk/subprocess cache"). The frontend refresh indicator logic is unchanged.

#### Performance: File Scanning

For short periods (day/5h), only files modified today are scanned (using `fs::metadata().modified()` to skip old files without reading contents — porting the `modified_today` optimization from `hourly.rs` into a generalized `modified_since` check).

For longer periods (week/month/year), all files in the provider directory are scanned. The `~/.claude/projects/` directory can contain hundreds of files; each JSONL file is read and lines are filtered by timestamp. This is expected to complete in <100ms for typical usage histories.

#### Claude JSONL Format

Each line is a JSON object. We parse entries where `type == "assistant"`:

```json
{
  "type": "assistant",
  "timestamp": "2026-03-15T10:30:00Z",
  "message": {
    "model": "claude-sonnet-4-6-20260301",
    "usage": {
      "input_tokens": 3000,
      "output_tokens": 500,
      "cache_creation_input_tokens": 100,
      "cache_read_input_tokens": 200
    }
  }
}
```

#### Codex JSONL Format

Each line is a JSON object. We parse entries where `type == "event_msg"` and `payload.type == "token_count"`:

```json
{
  "type": "event_msg",
  "timestamp": "2026-03-15T10:30:00Z",
  "payload": {
    "type": "token_count",
    "info": {
      "last_token_usage": {
        "input_tokens": 1000,
        "output_tokens": 500,
        "reasoning_output_tokens": 200
      }
    }
  }
}
```

**Important: `last_token_usage` contains cumulative totals, not deltas.** The parser must take only the **final** `token_count` event per session file to get accurate totals. Do not sum all `token_count` events — that would massively overcount.

Model name is extracted from lines in the same session file that contain a `"model"` field. These appear in turn-start events:

```json
{
  "type": "event_msg",
  "timestamp": "2026-03-15T10:29:00Z",
  "payload": {
    "type": "turn_start",
    "model": "gpt-5.3-codex"
  }
}
```

The parser tracks the most recent model seen per session file via raw string search for `"model":"` (same approach as the existing `extract_model_from_line` in `hourly.rs`).

### Changes to `commands.rs`

**`AppState` simplification:**

```rust
pub struct AppState {
    pub parser: Arc<parser::UsageParser>,
    pub refresh_interval: Arc<RwLock<u64>>,
}
```

- `runner: Arc<RwLock<CcusageRunner>>` → removed
- `setup_status: Arc<RwLock<SetupStatus>>` → removed (app is always ready)

**Command changes:**

| Command | Before | After |
|---------|--------|-------|
| `get_usage_data` | Calls `runner.run_cached()` → subprocess | Calls `parser.get_daily/monthly/blocks/hourly()` |
| `initialize_app` | Runs `npm install ccusage` | Removed |
| `get_setup_status` | Returns install status | Removed |
| `clear_cache` | Clears 3-tier cache | Calls `parser.clear_cache()` |
| `set_refresh_interval` | Unchanged | Unchanged |

### Changes to `lib.rs`

**`background_loop` simplification:**

- Remove `ensure_installed()` call
- Remove 12-hour `update_packages()` check
- `update_tray_title` calls `parser.get_daily()` directly
- Emit `setup-complete` immediately (no install wait)
- Still emits `data-updated` on polling interval

### Frontend Changes

Minimal — the `UsagePayload` shape is unchanged.

- Repurpose `SetupScreen` into an **empty state screen**: when `~/.claude/projects/` and `~/.codex/sessions/` do not exist (or contain no JSONL files), show a message like "No usage data found. Use Claude Code or Codex CLI to start tracking." This replaces the current "installing ccusage" state.
- Remove `checkSetup()` / `initializeApp()` calls from `App.svelte` onMount
- `setupStatus` store can be removed or simplified to a `hasData` boolean
- The `setup-complete` event listener can be removed

### Files Deleted

| File | Reason |
|------|--------|
| `src-tauri/src/ccusage.rs` | Replaced by `parser.rs` |
| `src-tauri/src/hourly.rs` | Absorbed into `parser.rs` |

### Files Created

| File | Purpose |
|------|---------|
| `src-tauri/src/pricing.rs` | Pricing table + cost calculation |
| `src-tauri/src/parser.rs` | JSONL reading + aggregation |

### Files Modified

| File | Changes |
|------|---------|
| `src-tauri/src/lib.rs` | Remove ccusage module, add parser/pricing modules, simplify background_loop |
| `src-tauri/src/commands.rs` | Replace CcusageRunner with UsageParser, remove setup commands |
| `src-tauri/src/models.rs` | Remove ccusage response types (ClaudeDailyResponse etc.) — parser produces UsagePayload directly |
| `src/App.svelte` | Remove checkSetup/initializeApp, remove SetupScreen |
| `src/lib/stores/usage.ts` | Remove initializeApp/checkSetup exports |

### Dependencies

**Removed:** None (ccusage was npm-installed at runtime, not a Cargo dep)

**No new Cargo dependencies needed.** Existing `chrono`, `serde`, `serde_json`, `dirs` cover everything. Recursive file discovery uses manual `fs::read_dir` traversal (porting the existing `glob_jsonl_files` helper from `hourly.rs`), not a glob crate.

## Testing Strategy

- Unit tests for `pricing.rs`: verify each model pattern returns correct rates, test fuzzy fallback
- Unit tests for `parser.rs`: create temp JSONL files, verify daily/monthly/blocks/hourly aggregation
- Port relevant existing tests from `ccusage.rs` (cache behavior) and `hourly.rs` (parsing) and `commands.rs` (payload construction)
- Integration: verify `UsagePayload` shape is identical to what the frontend expects

## Migration

This is a clean break — no backward compatibility needed. The ccusage install directory (`~/Library/Application Support/com.tokenmonitor.app/node_modules/`) can be left as-is; it will simply be unused.

**Cost calculation differences:** The new pricing table is locally maintained rather than using ccusage's built-in pricing. Costs may differ slightly from what ccusage previously reported due to rounding differences or pricing version mismatches. This is expected and acceptable.

## Risks

1. **JSONL format changes:** If Claude Code or Codex CLI change their log format, the Rust parser needs updating (same risk ccusage had, but now we own the code).
2. **Pricing staleness:** Model prices change a few times per year. Requires an app update to refresh the pricing table (same as ccusage — it shipped new npm versions for price changes).
3. **Blocks detection:** The 5-hour billing window / gap detection logic needs to match Claude's actual billing behavior. The 30-minute gap threshold is an approximation that should be validated.
