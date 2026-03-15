# Test Coverage Design for TokenMonitor

**Date:** 2026-03-15
**Status:** Draft

## Overview

Full unit test coverage for TokenMonitor's Rust backend and TypeScript frontend. Tests target pure business logic (data transformation, formatting, caching) without requiring Tauri runtime or GUI rendering.

## Architecture

```
Rust (cargo test)                     TypeScript (vitest)
─────────────────                     ──────────────────
models.rs      → normalize functions  format.ts    → currency/formatting
commands.rs    → transform functions  stores/usage → fetch/cache logic
hourly.rs      → hourly distribution
ccusage.rs     → 3-tier cache logic
```

## Rust Backend Tests

### 1. `models.rs` — Model Normalization

Tests live in `#[cfg(test)] mod tests` at the bottom of the file. These functions are already `pub` so tests can call them directly from any module in the crate.

**`normalize_claude_model()`**

| Input | Expected |
|---|---|
| `"claude-opus-4-6-20260301"` | `("Opus 4.6", "opus")` |
| `"claude-opus-4-5-20250501"` | `("Opus 4.5", "opus")` |
| `"claude-sonnet-4-6-20260301"` | `("Sonnet 4.6", "sonnet")` |
| `"claude-3-5-sonnet-20241022"` | `("Sonnet", "sonnet")` |
| `"claude-haiku-4-5-20251001"` | `("Haiku 4.5", "haiku")` |
| `"some-unknown-model"` | `("Unknown", "unknown")` |

**`normalize_codex_model()`**

| Input | Expected |
|---|---|
| `"gpt-5.4-turbo"` | `("GPT-5.4", "gpt54")` |
| `"gpt-5.3-codex"` | `("GPT-5.3 Codex", "gpt53")` |
| `"gpt-5.2"` | `("GPT-5.2", "gpt52")` |
| `"o3-mini"` | `("o3-mini", "codex")` |

**Serde deserialization** — 4 tests verifying `ClaudeDailyResponse`, `ClaudeBlocksResponse`, `ClaudeMonthlyResponse`, `CodexDailyResponse` deserialize from realistic JSON fixtures. Tests verify default values for missing optional fields (`#[serde(default)]`).

### 2. `commands.rs` — Data Transformation

Transform functions (`blocks_to_payload`, `claude_daily_to_payload`, `claude_monthly_to_payload`, `codex_daily_to_payload`, `merge_payloads`, `aggregate_claude_models`, `count_claude_tokens`, `mb_to_segment`) are currently private. Tests go in a `#[cfg(test)] mod tests` block at the bottom of the same file, which has access to all private items.

**`blocks_to_payload()`**
- Input: `ClaudeBlocksResponse` with 3 blocks (2 non-gap, 1 gap)
- Verify: total_cost sums non-gap only, gap block excluded from chart_buckets, active_block extracted from first non-gap block, model_breakdown aggregated across blocks, session_count = 2, five_hour_cost uses active block's cost

**`claude_daily_to_payload()`**
- Input: `ClaudeDailyResponse` with 2 days, each having model breakdowns
- Verify: date labels formatted ("2025-03-15" -> "Mar 15"), total_cost/total_tokens summed, input_tokens includes cache_creation + cache_read, model_breakdown deduplicated by key

**`claude_monthly_to_payload()`**
- Input: `ClaudeMonthlyResponse` with 3 months
- Verify: month labels formatted ("2025-03" -> "Mar"), same aggregation as daily

**`codex_daily_to_payload()`**
- Input: `CodexDailyResponse` with 2 days, 2 models each
- Verify: date parsing ("Mar 01, 2026" -> "Mar 1"), proportional cost distribution per model (cost * model_tokens/total_tokens), input/output/reasoning tokens summed correctly

**`merge_payloads()`**
- Input: two `UsagePayload` values with overlapping and unique bucket labels
- Verify: costs added, tokens added, overlapping buckets merged (segments concatenated), unique buckets preserved, from_cache is AND (true+false=false)

**`aggregate_claude_models()`**
- Input: iterator of 4 breakdowns, 2 sharing the same normalized key
- Verify: output has 3 entries, cost/tokens summed for duplicates

**`count_claude_tokens()`**
- Input: breakdowns with cache_creation=100, cache_read=50, input=200, output=300
- Verify: returns (350, 300) — input includes cache tokens

**`mb_to_segment()`**
- Input: single `ClaudeModelBreakdown` with a sonnet model name
- Verify: ChartSegment has normalized name/key, correct cost, tokens = input + output

### 3. `hourly.rs` — Hourly Distribution

Tests go in `#[cfg(test)] mod tests` at the bottom. The private helpers are testable from within the same file's test module.

**`format_hour()`**

| Input | Expected |
|---|---|
| `0` | `"12AM"` |
| `1` | `"1AM"` |
| `11` | `"11AM"` |
| `12` | `"12PM"` |
| `13` | `"1PM"` |
| `23` | `"11PM"` |

**`extract_model_from_line()`**
- `{"model":"claude-sonnet","type":"msg"}` -> `Some("claude-sonnet")`
- `{"type":"msg"}` (no model field) -> `None`
- empty string -> `None`

**`glob_jsonl_files()`**
- Create temp dir with nested structure containing `.jsonl` and `.txt` files
- Verify: only `.jsonl` files returned, recursive traversal works
- Empty/nonexistent directory -> empty Vec

**`modified_today()`**
- Create temp file (modified time = now) -> `true`
- File with old mtime -> `false`

### 4. `ccusage.rs` — Cache Logic

Tests go in `#[cfg(test)] mod tests`. Focus on the 3-tier cache hierarchy which is the most intricate logic.

**Cache key generation**
- `run_cached("claude", "daily", &["--since", "20260315"], ...)` -> cache key `"claude-daily---since-20260315"`
- Verify file path: `<cache_dir>/claude-daily---since-20260315.json`

**`clear_cache()`**
- Populate in-memory and disk caches with entries
- Call `clear_cache()`
- Verify: in-memory map is empty, disk cache directory has no files

**In-memory cache hit/miss**
- Store entry, query within TTL -> returns `(data, true)`
- Store entry, wait/use short TTL, query after expiry -> falls through to disk/CLI

Note: `run_cached` requires async + subprocess, so full integration tests of tier-3 (CLI) are out of scope. We test tiers 1-2 and `clear_cache` behavior.

## TypeScript Frontend Tests

### 5. `src/lib/utils/format.ts` — Formatting Utilities

**`formatCost()`**
- USD: `formatCost(1.5)` -> `"$1.50"`
- EUR: set EUR, `formatCost(1.0)` -> `"€0.92"`
- JPY: set JPY, `formatCost(1.0)` -> `"¥150"` (rounded, no decimals)
- Unknown currency: falls back to USD behavior

**`convertCost()`**
- USD: `convertCost(10)` -> `10`
- GBP: `convertCost(10)` -> `7.9`

**`currencySymbol()`**
- USD -> `"$"`, EUR -> `"€"`, GBP -> `"£"`, JPY -> `"¥"`, unknown -> `"$"`

**`setCurrency()`**
- `setCurrency("EUR")` then `formatCost(1)` -> `"€0.92"`

**`formatTokens()`**

| Input | Expected |
|---|---|
| `999` | `"999"` |
| `1000` | `"1K"` |
| `1500` | `"2K"` |
| `999999` | `"1000K"` |
| `1000000` | `"1.0M"` |
| `1500000` | `"1.5M"` |

**`formatTimeAgo()`**
- 2 seconds ago -> `"just now"`
- 30 seconds ago -> `"30s ago"`
- 5 minutes ago -> `"5m ago"`
- 2 hours ago -> `"2h ago"`

**`modelColor()`**
- `"opus"` -> `"var(--opus)"`
- `"sonnet"` -> `"var(--sonnet)"`
- `"unknown_key"` -> `"var(--t3)"`

### 6. `src/lib/stores/usage.ts` — Fetch & Cache Logic

All tests mock `@tauri-apps/api/core` via `vi.mock()`. Svelte stores work fine outside components — `get()` from `svelte/store` reads current values synchronously.

**`fetchData()` — cold path**
- Mock `invoke` to return a payload
- Call `fetchData("claude", "day")`
- Verify: `usageData` store receives the payload, `isLoading` transitions true -> false, internal payloadCache populated

**`fetchData()` — warm cache hit**
- Pre-populate payloadCache, call `fetchData`
- Verify: `usageData` set synchronously (before `invoke` resolves), background refresh fires
- Verify: stale response is replaced when background refresh completes

**`fetchData()` — request deduplication**
- Call `fetchData` twice rapidly with different periods
- First invoke resolves after second
- Verify: `usageData` has the second call's data (monotonic ID check prevents stale overwrite)

**`fetchData()` — error handling**
- Mock `invoke` to reject
- Verify: `isLoading` set back to false, `usageData` not clobbered, error logged

**`warmCache()`**
- Call `warmCache("claude", "week")`
- Verify: `invoke` called, payloadCache populated, `usageData` store NOT updated

**`warmAllPeriods()`**
- Call `warmAllPeriods("claude", "day")`
- Verify: `invoke` called 4 times (all periods except "day")

**`initializeApp()`**
- Success: mock returns `{ready: true, installing: false, error: null}`
- Verify: `setupStatus` transitions installing -> ready
- Error: mock rejects
- Verify: `setupStatus` has error string, ready=false

**`checkSetup()`**
- Success: returns status object
- Failure: mock rejects, returns null

## Test Infrastructure

### Rust
- Framework: built-in `#[cfg(test)]` with `cargo test`
- No new dependencies required
- Test files: inline `mod tests` blocks in each source file
- Run: `cd src-tauri && cargo test`

### TypeScript
- Framework: **Vitest** (zero-config with Vite)
- New dev dependencies: `vitest`
- Test files: colocated as `format.test.ts` and `usage.test.ts` next to source
- Config: `vitest.config.ts` at project root
- Run: `npx vitest run`

### package.json scripts
```json
{
  "test": "vitest run",
  "test:watch": "vitest",
  "test:rust": "cd src-tauri && cargo test",
  "test:all": "npm run test:rust && npm run test"
}
```

## File Changes Summary

| File | Action |
|---|---|
| `src-tauri/src/models.rs` | Add `#[cfg(test)] mod tests` |
| `src-tauri/src/commands.rs` | Add `#[cfg(test)] mod tests` |
| `src-tauri/src/hourly.rs` | Add `#[cfg(test)] mod tests` |
| `src-tauri/src/ccusage.rs` | Add `#[cfg(test)] mod tests` |
| `src/lib/utils/format.test.ts` | New file |
| `src/lib/stores/usage.test.ts` | New file |
| `vitest.config.ts` | New file |
| `package.json` | Add vitest dep + test scripts |

## Test Count Estimate

- Rust: ~35 tests (8 models + 10 commands + 10 hourly + 7 ccusage)
- TypeScript: ~20 tests (12 format + 8 usage store)
- **Total: ~55 tests**
