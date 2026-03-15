# Replace ccusage with Native Rust Parser — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the Node.js/ccusage dependency by replacing it with pure Rust JSONL parsing and a hardcoded pricing table.

**Architecture:** Two new modules (`pricing.rs` for cost calculation, `parser.rs` for JSONL reading + aggregation) replace `ccusage.rs` and `hourly.rs`. `commands.rs` and `lib.rs` are simplified to call the parser directly. Frontend changes are minimal — `UsagePayload` shape is unchanged.

**Tech Stack:** Rust (chrono, serde, serde_json, dirs), Svelte/TypeScript frontend

**Spec:** `docs/superpowers/specs/2026-03-15-replace-ccusage-with-native-rust-parser.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src-tauri/src/pricing.rs` | Create | Pricing lookup table + `calculate_cost()` |
| `src-tauri/src/parser.rs` | Create | JSONL reading, parsing, aggregation (daily/monthly/blocks/hourly), caching |
| `src-tauri/src/commands.rs` | Modify | Replace CcusageRunner with UsageParser, remove setup commands, simplify dispatch |
| `src-tauri/src/models.rs` | Modify | Remove ccusage response types (ClaudeDailyResponse, etc.), keep UsagePayload and helpers |
| `src-tauri/src/lib.rs` | Modify | Swap modules, simplify background_loop, remove npm/node logic |
| `src-tauri/src/ccusage.rs` | Delete | Replaced by parser.rs |
| `src-tauri/src/hourly.rs` | Delete | Absorbed into parser.rs |
| `src/App.svelte` | Modify | Remove setup flow, simplify onMount |
| `src/lib/stores/usage.ts` | Modify | Remove initializeApp/checkSetup |
| `src/lib/components/SetupScreen.svelte` | Modify | Repurpose as empty-state screen |

---

## Chunk 1: pricing.rs — Pricing Table and Cost Calculation

### Task 1: Create pricing.rs with Claude model tests

**Files:**
- Create: `src-tauri/src/pricing.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 0: Register pricing module in lib.rs**

In `src-tauri/src/lib.rs`, add `mod pricing;` alongside the existing module declarations:

```rust
mod ccusage;
mod commands;
mod hourly;
mod models;
mod pricing;
```

This is needed so the Rust compiler discovers `pricing.rs`. The existing modules still compile at this point.

- [ ] **Step 1: Write failing tests for Claude model pricing**

Add to the bottom of a new `src-tauri/src/pricing.rs` file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_4_6_pricing() {
        // claude-opus-4-6-20260301: $5/MTok input, $25/MTok output
        let cost = calculate_cost("claude-opus-4-6-20260301", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 30.0).abs() < 0.001, "expected $30.00, got ${:.3}", cost);
    }

    #[test]
    fn sonnet_4_6_pricing() {
        // claude-sonnet-4-6-20260301: $3/MTok input, $15/MTok output
        let cost = calculate_cost("claude-sonnet-4-6-20260301", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 18.0).abs() < 0.001, "expected $18.00, got ${:.3}", cost);
    }

    #[test]
    fn haiku_4_5_pricing() {
        // claude-haiku-4-5-20251001: $1/MTok input, $5/MTok output
        let cost = calculate_cost("claude-haiku-4-5-20251001", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 6.0).abs() < 0.001, "expected $6.00, got ${:.3}", cost);
    }

    #[test]
    fn claude_cache_tokens() {
        // Sonnet 4.6: cache_write=$3.75/MTok, cache_read=$0.30/MTok
        let cost = calculate_cost("claude-sonnet-4-6-20260301", 0, 0, 1_000_000, 1_000_000);
        assert!((cost - 4.05).abs() < 0.001, "expected $4.05, got ${:.3}", cost);
    }

    #[test]
    fn opus_4_1_higher_pricing() {
        // claude-opus-4-1: $15/MTok input, $75/MTok output (older, more expensive)
        let cost = calculate_cost("claude-opus-4-1-20250501", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 90.0).abs() < 0.001, "expected $90.00, got ${:.3}", cost);
    }

    #[test]
    fn opus_4_no_minor_version() {
        // claude-opus-4-20250401: matches opus-4 (not opus-4-5 or opus-4-6)
        let cost = calculate_cost("claude-opus-4-20250401", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 90.0).abs() < 0.001, "expected $90.00, got ${:.3}", cost);
    }

    #[test]
    fn sonnet_3_7_hits_sonnet_catchall() {
        // claude-3-7-sonnet-20250219: matches "sonnet" catchall → $3/$15
        let cost = calculate_cost("claude-3-7-sonnet-20250219", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 18.0).abs() < 0.001, "expected $18.00, got ${:.3}", cost);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib pricing -- --nocapture 2>&1 | head -30`
Expected: compilation error — `calculate_cost` not defined

- [ ] **Step 3: Implement pricing table with Claude models**

Add at the top of `src-tauri/src/pricing.rs` (above the `#[cfg(test)]` block):

```rust
/// Pricing data version — update when prices change.
pub const PRICING_VERSION: &str = "2026-03-15";

/// Per-token rates in dollars. All prices are $/MTok from the pricing tables.
struct ModelRates {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

/// Calculate the dollar cost for a set of token counts given a model name.
///
/// Matches model name patterns against a hardcoded pricing table.
/// Unknown models fall back to the closest known family pricing.
pub fn calculate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> f64 {
    let rates = get_rates(model);
    let mtok = 1_000_000.0;

    (input_tokens as f64 / mtok) * rates.input
        + (output_tokens as f64 / mtok) * rates.output
        + (cache_creation_tokens as f64 / mtok) * rates.cache_write
        + (cache_read_tokens as f64 / mtok) * rates.cache_read
}

fn get_rates(model: &str) -> ModelRates {
    // Claude models — matched most-specific first
    if model.contains("opus-4-6") {
        return ModelRates { input: 5.0, output: 25.0, cache_write: 6.25, cache_read: 0.50 };
    }
    if model.contains("opus-4-5") {
        return ModelRates { input: 5.0, output: 25.0, cache_write: 6.25, cache_read: 0.50 };
    }
    if model.contains("opus-4-1") {
        return ModelRates { input: 15.0, output: 75.0, cache_write: 18.75, cache_read: 1.50 };
    }
    if model.contains("opus-4") {
        return ModelRates { input: 15.0, output: 75.0, cache_write: 18.75, cache_read: 1.50 };
    }
    if model.contains("sonnet-4-6") {
        return ModelRates { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 };
    }
    if model.contains("sonnet-4-5") {
        return ModelRates { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 };
    }
    if model.contains("sonnet") {
        return ModelRates { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 };
    }
    if model.contains("haiku-4-5") {
        return ModelRates { input: 1.0, output: 5.0, cache_write: 1.25, cache_read: 0.10 };
    }
    if model.contains("haiku-3-5") {
        return ModelRates { input: 0.80, output: 4.0, cache_write: 1.0, cache_read: 0.08 };
    }
    if model.contains("haiku") {
        return ModelRates { input: 0.25, output: 1.25, cache_write: 0.30, cache_read: 0.03 };
    }

    // OpenAI/Codex models — see Task 2

    // Fuzzy fallback — see Task 3
    get_fallback_rates(model)
}

fn get_fallback_rates(model: &str) -> ModelRates {
    // Placeholder — will be implemented in Task 3
    ModelRates { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib pricing -- --nocapture`
Expected: all 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pricing.rs src-tauri/src/lib.rs
git commit -m "feat(pricing): add pricing module with Claude model rates and tests"
```

---

### Task 2: Add OpenAI/Codex model pricing

**Files:**
- Modify: `src-tauri/src/pricing.rs`

- [ ] **Step 1: Write failing tests for OpenAI/Codex models**

Add to the `tests` module in `pricing.rs`:

```rust
    #[test]
    fn gpt_5_4_pricing() {
        // gpt-5.4: $2.50/MTok input, $15.00/MTok output
        let cost = calculate_cost("gpt-5.4", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 17.5).abs() < 0.001, "expected $17.50, got ${:.3}", cost);
    }

    #[test]
    fn gpt_5_3_codex_pricing() {
        // gpt-5.3-codex: $1.75/MTok input, $14.00/MTok output
        let cost = calculate_cost("gpt-5.3-codex", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 15.75).abs() < 0.001, "expected $15.75, got ${:.3}", cost);
    }

    #[test]
    fn gpt_5_1_codex_mini_pricing() {
        // gpt-5.1-codex-mini: $0.25/MTok input, $2.00/MTok output
        let cost = calculate_cost("gpt-5.1-codex-mini", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 2.25).abs() < 0.001, "expected $2.25, got ${:.3}", cost);
    }

    #[test]
    fn o4_mini_pricing() {
        // o4-mini: $1.10/MTok input, $4.40/MTok output
        let cost = calculate_cost("o4-mini-2025-04-16", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 5.5).abs() < 0.001, "expected $5.50, got ${:.3}", cost);
    }

    #[test]
    fn o3_pricing() {
        // o3: $2.00/MTok input, $8.00/MTok output
        let cost = calculate_cost("o3-2025-04-16", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 10.0).abs() < 0.001, "expected $10.00, got ${:.3}", cost);
    }

    #[test]
    fn o3_mini_pricing() {
        // o3-mini: $1.10/MTok input, $4.40/MTok output
        let cost = calculate_cost("o3-mini-2025-01-31", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 5.5).abs() < 0.001, "expected $5.50, got ${:.3}", cost);
    }

    #[test]
    fn o1_pricing() {
        // o1: $15.00/MTok input, $60.00/MTok output
        let cost = calculate_cost("o1-2024-12-17", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 75.0).abs() < 0.001, "expected $75.00, got ${:.3}", cost);
    }

    #[test]
    fn o1_mini_pricing() {
        // o1-mini: $1.10/MTok input, $4.40/MTok output
        let cost = calculate_cost("o1-mini-2024-09-12", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 5.5).abs() < 0.001, "expected $5.50, got ${:.3}", cost);
    }

    #[test]
    fn openai_cached_input_tokens() {
        // gpt-5.4: cached input = $0.25/MTok (cache_read), standard input = $2.50/MTok (cache_write)
        let cost = calculate_cost("gpt-5.4", 0, 0, 1_000_000, 1_000_000);
        assert!((cost - 2.75).abs() < 0.001, "expected $2.75 (2.50 write + 0.25 read), got ${:.3}", cost);
    }

    #[test]
    fn codex_mini_latest_pricing() {
        // codex-mini-latest: $1.50/MTok input, $6.00/MTok output
        let cost = calculate_cost("codex-mini-latest", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 7.5).abs() < 0.001, "expected $7.50, got ${:.3}", cost);
    }

    // Ordering-sensitive tests: verify specific models don't match broader patterns
    #[test]
    fn gpt_5_base_pricing() {
        // "gpt-5" must not match "gpt-5.4" or "gpt-5-mini"
        let cost = calculate_cost("gpt-5", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 11.25).abs() < 0.001, "expected $11.25 (1.25+10.00), got ${:.3}", cost);
    }

    #[test]
    fn gpt_5_1_codex_not_mini() {
        // "gpt-5.1-codex" must NOT match codex-mini rate ($0.25/$2.00)
        let cost = calculate_cost("gpt-5.1-codex", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 11.25).abs() < 0.001, "expected $11.25 (1.25+10.00), got ${:.3}", cost);
    }

    #[test]
    fn gpt_5_mini_pricing() {
        let cost = calculate_cost("gpt-5-mini", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 2.25).abs() < 0.001, "expected $2.25 (0.25+2.00), got ${:.3}", cost);
    }

    #[test]
    fn o3_cache_rates() {
        // o3: cache_write=$2.00/MTok (standard input), cache_read=$0.50/MTok
        let cost = calculate_cost("o3-2025-04-16", 0, 0, 1_000_000, 1_000_000);
        assert!((cost - 2.5).abs() < 0.001, "expected $2.50 (2.00 write + 0.50 read), got ${:.3}", cost);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib pricing -- --nocapture 2>&1 | head -40`
Expected: new tests FAIL (hitting fallback Sonnet pricing)

- [ ] **Step 3: Add OpenAI/Codex rates to `get_rates()`**

In `pricing.rs`, insert the OpenAI block in `get_rates()` between the Claude `haiku` match and the fallback call:

```rust
    // OpenAI/Codex models — most-specific first
    // For OpenAI: cache_write = standard input rate, cache_read = cached input rate
    if model.contains("gpt-5.4") {
        return ModelRates { input: 2.50, output: 15.0, cache_write: 2.50, cache_read: 0.25 };
    }
    if model.contains("gpt-5.3-codex") {
        return ModelRates { input: 1.75, output: 14.0, cache_write: 1.75, cache_read: 0.175 };
    }
    if model.contains("gpt-5.2-codex") {
        return ModelRates { input: 1.75, output: 14.0, cache_write: 1.75, cache_read: 0.175 };
    }
    if model.contains("gpt-5.2") {
        return ModelRates { input: 1.75, output: 14.0, cache_write: 1.75, cache_read: 0.175 };
    }
    if model.contains("gpt-5.1-codex-max") {
        return ModelRates { input: 1.25, output: 10.0, cache_write: 1.25, cache_read: 0.125 };
    }
    if model.contains("gpt-5.1-codex-mini") {
        return ModelRates { input: 0.25, output: 2.0, cache_write: 0.25, cache_read: 0.025 };
    }
    if model.contains("gpt-5.1-codex") {
        return ModelRates { input: 1.25, output: 10.0, cache_write: 1.25, cache_read: 0.125 };
    }
    if model.contains("codex-mini-latest") {
        return ModelRates { input: 1.50, output: 6.0, cache_write: 1.50, cache_read: 0.375 };
    }
    if model.contains("gpt-5-codex") {
        return ModelRates { input: 1.25, output: 10.0, cache_write: 1.25, cache_read: 0.125 };
    }
    if model.contains("gpt-5-mini") {
        return ModelRates { input: 0.25, output: 2.0, cache_write: 0.25, cache_read: 0.025 };
    }
    if model.contains("gpt-5-nano") {
        return ModelRates { input: 0.05, output: 0.40, cache_write: 0.05, cache_read: 0.005 };
    }
    if model.contains("gpt-5.1") {
        return ModelRates { input: 1.25, output: 10.0, cache_write: 1.25, cache_read: 0.125 };
    }
    if model.contains("gpt-5") {
        return ModelRates { input: 1.25, output: 10.0, cache_write: 1.25, cache_read: 0.125 };
    }
    if model.starts_with("o4-mini") {
        return ModelRates { input: 1.10, output: 4.40, cache_write: 1.10, cache_read: 0.275 };
    }
    if model.starts_with("o3-mini") {
        return ModelRates { input: 1.10, output: 4.40, cache_write: 1.10, cache_read: 0.55 };
    }
    if model.starts_with("o3") {
        return ModelRates { input: 2.0, output: 8.0, cache_write: 2.0, cache_read: 0.50 };
    }
    if model.starts_with("o1-mini") {
        return ModelRates { input: 1.10, output: 4.40, cache_write: 1.10, cache_read: 0.55 };
    }
    if model.starts_with("o1") {
        return ModelRates { input: 15.0, output: 60.0, cache_write: 15.0, cache_read: 7.50 };
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib pricing -- --nocapture`
Expected: all 21 tests PASS (7 Claude + 14 OpenAI/Codex)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pricing.rs
git commit -m "feat(pricing): add OpenAI/Codex model rates"
```

---

### Task 3: Add fuzzy fallback for unknown models

**Files:**
- Modify: `src-tauri/src/pricing.rs`

- [ ] **Step 1: Write failing tests for fuzzy fallback**

Add to the `tests` module:

```rust
    #[test]
    fn unknown_opus_falls_back_to_latest() {
        // "claude-opus-5-0-20270101" → fallback opus pricing ($5/$25)
        let cost = calculate_cost("claude-opus-5-0-20270101", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 30.0).abs() < 0.001, "expected $30.00 (opus fallback), got ${:.3}", cost);
    }

    #[test]
    fn unknown_sonnet_falls_back() {
        let cost = calculate_cost("claude-sonnet-5-0", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 18.0).abs() < 0.001, "expected $18.00 (sonnet fallback), got ${:.3}", cost);
    }

    #[test]
    fn unknown_haiku_falls_back() {
        let cost = calculate_cost("claude-haiku-5-0", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 6.0).abs() < 0.001, "expected $6.00 (haiku fallback), got ${:.3}", cost);
    }

    #[test]
    fn unknown_codex_mini_falls_back() {
        let cost = calculate_cost("gpt-6-codex-mini", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 2.25).abs() < 0.001, "expected $2.25 (codex-mini fallback), got ${:.3}", cost);
    }

    #[test]
    fn unknown_codex_falls_back() {
        let cost = calculate_cost("gpt-6-codex", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 11.25).abs() < 0.001, "expected $11.25 (codex fallback), got ${:.3}", cost);
    }

    #[test]
    fn unknown_o_series_falls_back() {
        let cost = calculate_cost("o5-mini-2026-01-01", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 5.5).abs() < 0.001, "expected $5.50 (o-series fallback), got ${:.3}", cost);
    }

    #[test]
    fn completely_unknown_falls_back_to_sonnet() {
        let cost = calculate_cost("totally-unknown-model", 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 18.0).abs() < 0.001, "expected $18.00 (sonnet fallback), got ${:.3}", cost);
    }

    #[test]
    fn zero_tokens_zero_cost() {
        let cost = calculate_cost("claude-sonnet-4-6", 0, 0, 0, 0);
        assert!((cost - 0.0).abs() < 0.001);
    }

    #[test]
    fn pricing_version_is_set() {
        assert_eq!(PRICING_VERSION, "2026-03-15");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib pricing -- --nocapture 2>&1 | tail -20`
Expected: `unknown_opus_falls_back_to_latest`, `unknown_haiku_falls_back`, `unknown_codex_mini_falls_back`, `unknown_codex_falls_back`, `unknown_o_series_falls_back` FAIL (all returning Sonnet pricing from the placeholder)

- [ ] **Step 3: Implement fuzzy fallback in `get_fallback_rates()`**

Replace the `get_fallback_rates` placeholder:

```rust
fn get_fallback_rates(model: &str) -> ModelRates {
    if model.contains("opus") {
        return ModelRates { input: 5.0, output: 25.0, cache_write: 6.25, cache_read: 0.50 };
    }
    if model.contains("sonnet") {
        return ModelRates { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 };
    }
    if model.contains("haiku") {
        return ModelRates { input: 1.0, output: 5.0, cache_write: 1.25, cache_read: 0.10 };
    }
    if model.contains("codex-mini") {
        return ModelRates { input: 0.25, output: 2.0, cache_write: 0.25, cache_read: 0.025 };
    }
    if model.contains("codex") || model.contains("gpt-5") {
        return ModelRates { input: 1.25, output: 10.0, cache_write: 1.25, cache_read: 0.125 };
    }
    // o-series: starts with 'o' followed by a digit
    if model.starts_with('o') && model.chars().nth(1).map_or(false, |c| c.is_ascii_digit()) {
        return ModelRates { input: 1.10, output: 4.40, cache_write: 1.10, cache_read: 0.275 };
    }
    // Completely unknown → conservative Sonnet pricing
    ModelRates { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib pricing -- --nocapture`
Expected: all 30 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pricing.rs
git commit -m "feat(pricing): add fuzzy fallback for unknown models"
```

---

## Chunk 2: parser.rs — JSONL Reading and Aggregation

### Task 4: Create parser.rs with file scanning helpers and Claude JSONL parsing

**Files:**
- Create: `src-tauri/src/parser.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 0: Register parser module in lib.rs**

In `src-tauri/src/lib.rs`, add `mod parser;` (alongside the `mod pricing;` added in Task 1):

```rust
mod ccusage;
mod commands;
mod hourly;
mod models;
mod parser;
mod pricing;
```

- [ ] **Step 1: Write failing tests for Claude JSONL parsing**

Create `src-tauri/src/parser.rs` with tests at the bottom.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_claude_jsonl(dir: &std::path::Path, filename: &str, lines: &[&str]) {
        let path = dir.join(filename);
        fs::write(&path, lines.join("\n")).unwrap();
    }

    #[test]
    fn parse_claude_entries_from_jsonl() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test-project");
        fs::create_dir_all(&project_dir).unwrap();

        write_claude_jsonl(&project_dir, "session.jsonl", &[
            r#"{"type":"user","timestamp":"2026-03-15T10:00:00Z","message":{"text":"hello"}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20260301","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":5}}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-15T10:30:00Z","message":{"model":"claude-sonnet-4-6-20260301","usage":{"input_tokens":200,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);

        let entries = read_claude_entries(&project_dir, "20260315");
        assert_eq!(entries.len(), 2, "should parse 2 assistant entries, skipping user entry");
        assert_eq!(entries[0].input_tokens, 100);
        assert_eq!(entries[0].output_tokens, 50);
        assert_eq!(entries[0].cache_creation_tokens, 10);
        assert_eq!(entries[0].cache_read_tokens, 5);
        assert_eq!(entries[0].model, "claude-sonnet-4-6-20260301");
        assert_eq!(entries[1].input_tokens, 200);
    }

    #[test]
    fn parse_claude_filters_by_date() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test-project");
        fs::create_dir_all(&project_dir).unwrap();

        // Use timestamps with enough margin to be unambiguous across timezones
        write_claude_jsonl(&project_dir, "session.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-03-14T12:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":50}}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-15T12:00:01Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":200,"output_tokens":100}}}"#,
        ]);

        let entries = read_claude_entries(&project_dir, "20260315");
        assert_eq!(entries.len(), 1, "should only include entries on/after since date");
        assert_eq!(entries[0].input_tokens, 200);
    }

    #[test]
    fn parse_claude_recursive_glob() {
        let dir = TempDir::new().unwrap();
        let sub1 = dir.path().join("projects").join("proj-a");
        let sub2 = dir.path().join("projects").join("proj-b").join("nested");
        fs::create_dir_all(&sub1).unwrap();
        fs::create_dir_all(&sub2).unwrap();

        write_claude_jsonl(&sub1, "a.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-03-15T10:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":50}}}"#,
        ]);
        write_claude_jsonl(&sub2, "b.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-03-15T11:00:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":300,"output_tokens":150}}}"#,
        ]);

        let entries = read_claude_entries(&dir.path().join("projects"), "20260315");
        assert_eq!(entries.len(), 2, "should find JSONL files recursively");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture 2>&1 | head -20`
Expected: compilation error — `read_claude_entries` not defined

- [ ] **Step 3: Implement ParsedEntry, file scanning, and Claude parsing**

Add at the top of `src-tauri/src/parser.rs` (above `#[cfg(test)]`):

```rust
use crate::models::*;
use crate::pricing;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

// ── Parsed entry (normalized from either provider) ──

pub struct ParsedEntry {
    pub timestamp: DateTime<Local>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

// ── Claude JSONL structs ──

#[derive(Deserialize)]
struct ClaudeJsonlEntry {
    #[serde(rename = "type", default)]
    entry_type: String,
    #[serde(default)]
    timestamp: String,
    message: Option<ClaudeJsonlMessage>,
}

#[derive(Deserialize)]
struct ClaudeJsonlMessage {
    model: Option<String>,
    usage: Option<ClaudeJsonlUsage>,
}

#[derive(Deserialize)]
struct ClaudeJsonlUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

// ── File scanning helpers ──

fn glob_jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if !dir.exists() {
        return results;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.append(&mut glob_jsonl_files(&path));
            } else if path.extension().map_or(false, |e| e == "jsonl") {
                results.push(path);
            }
        }
    }
    results
}

fn parse_since_date(since: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(since, "%Y%m%d").ok()
}

// ── Claude parsing ──

fn read_claude_entries(projects_dir: &Path, since: &str) -> Vec<ParsedEntry> {
    let since_date = match parse_since_date(since) {
        Some(d) => d,
        None => return vec![],
    };

    let files = glob_jsonl_files(projects_dir);
    let mut entries = Vec::new();

    for path in files {
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in contents.lines() {
            let entry: ClaudeJsonlEntry = match serde_json::from_str(line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.entry_type != "assistant" {
                continue;
            }

            let msg = match &entry.message {
                Some(m) => m,
                None => continue,
            };
            let usage = match &msg.usage {
                Some(u) => u,
                None => continue,
            };
            let model = match &msg.model {
                Some(m) => m.clone(),
                None => continue,
            };

            let ts = match DateTime::parse_from_rfc3339(&entry.timestamp) {
                Ok(dt) => dt.with_timezone(&Local),
                Err(_) => continue,
            };

            if ts.date_naive() < since_date {
                continue;
            }

            entries.push(ParsedEntry {
                timestamp: ts,
                model,
                input_tokens: usage.input_tokens.unwrap_or(0),
                output_tokens: usage.output_tokens.unwrap_or(0),
                cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
                cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
            });
        }
    }

    entries.sort_by_key(|e| e.timestamp);
    entries
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture`
Expected: all 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/parser.rs
git commit -m "feat(parser): add Claude JSONL parsing with file scanning"
```

---

### Task 5: Add Codex JSONL parsing

**Files:**
- Modify: `src-tauri/src/parser.rs`

- [ ] **Step 1: Write failing tests for Codex parsing**

Add to the `tests` module in `parser.rs`:

```rust
    fn write_codex_jsonl(dir: &std::path::Path, filename: &str, lines: &[&str]) {
        fs::write(dir.join(filename), lines.join("\n")).unwrap();
    }

    #[test]
    fn parse_codex_uses_final_token_count_only() {
        let dir = TempDir::new().unwrap();
        let session_dir = dir.path().join("2026").join("03").join("15");
        fs::create_dir_all(&session_dir).unwrap();

        write_codex_jsonl(&session_dir, "session-001.jsonl", &[
            r#"{"type":"event_msg","timestamp":"2026-03-15T10:00:00Z","payload":{"type":"turn_start","model":"gpt-5.3-codex"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-03-15T10:00:05Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"output_tokens":50,"reasoning_output_tokens":10}}}}"#,
            r#"{"type":"event_msg","timestamp":"2026-03-15T10:00:10Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":200,"output_tokens":100,"reasoning_output_tokens":20}}}}"#,
        ]);

        let entries = read_codex_entries(dir.path(), "20260315");
        // Should only use the FINAL token_count event (cumulative), not sum them
        assert_eq!(entries.len(), 1, "should produce 1 entry per session file (final totals)");
        assert_eq!(entries[0].input_tokens, 200);
        // output_tokens includes reasoning_output_tokens
        assert_eq!(entries[0].output_tokens, 120); // 100 + 20
        assert_eq!(entries[0].model, "gpt-5.3-codex");
    }

    #[test]
    fn parse_codex_filters_by_date() {
        let dir = TempDir::new().unwrap();
        // Data for March 14 — should be excluded when since=20260315
        let old_dir = dir.path().join("2026").join("03").join("14");
        fs::create_dir_all(&old_dir).unwrap();
        write_codex_jsonl(&old_dir, "old.jsonl", &[
            r#"{"type":"event_msg","timestamp":"2026-03-14T10:00:00Z","payload":{"type":"turn_start","model":"gpt-5.3-codex"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-03-14T10:00:05Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":999,"output_tokens":999,"reasoning_output_tokens":0}}}}"#,
        ]);

        // Data for March 15 — should be included
        let today_dir = dir.path().join("2026").join("03").join("15");
        fs::create_dir_all(&today_dir).unwrap();
        write_codex_jsonl(&today_dir, "today.jsonl", &[
            r#"{"type":"event_msg","timestamp":"2026-03-15T10:00:00Z","payload":{"type":"turn_start","model":"gpt-5.3-codex"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-03-15T10:00:05Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"output_tokens":50,"reasoning_output_tokens":0}}}}"#,
        ]);

        let entries = read_codex_entries(dir.path(), "20260315");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].input_tokens, 100);
    }

    #[test]
    fn parse_codex_empty_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let entries = read_codex_entries(dir.path(), "20260315");
        assert!(entries.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture 2>&1 | head -20`
Expected: compilation error — `read_codex_entries` not defined

- [ ] **Step 3: Implement Codex JSONL parsing**

Add to `parser.rs` (after the Claude parsing section):

```rust
// ── Codex JSONL structs ──

#[derive(Deserialize)]
struct CodexRolloutEntry {
    #[serde(default)]
    timestamp: String,
    #[serde(rename = "type", default)]
    entry_type: String,
    payload: Option<CodexPayload>,
}

#[derive(Deserialize)]
struct CodexPayload {
    #[serde(rename = "type", default)]
    payload_type: String,
    info: Option<CodexTokenInfo>,
}

#[derive(Deserialize)]
struct CodexTokenInfo {
    last_token_usage: Option<CodexTokenUsage>,
}

#[derive(Deserialize)]
struct CodexTokenUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
}

fn extract_model_from_line(line: &str) -> Option<String> {
    let marker = "\"model\":\"";
    let start = line.find(marker)? + marker.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

// ── Codex parsing ──

fn read_codex_entries(sessions_dir: &Path, since: &str) -> Vec<ParsedEntry> {
    let since_date = match parse_since_date(since) {
        Some(d) => d,
        None => return vec![],
    };

    let mut entries = Vec::new();

    // Codex stores sessions at sessions_dir/YYYY/MM/DD/*.jsonl
    // Iterate date directories from since_date to today
    let today = Local::now().date_naive();
    let mut date = since_date;

    while date <= today {
        let day_dir = sessions_dir
            .join(date.format("%Y").to_string())
            .join(date.format("%m").to_string())
            .join(date.format("%d").to_string());

        if day_dir.exists() {
            if let Ok(dir_entries) = std::fs::read_dir(&day_dir) {
                for dir_entry in dir_entries.flatten() {
                    let path = dir_entry.path();
                    if !path.extension().map_or(false, |e| e == "jsonl") {
                        continue;
                    }

                    if let Some(entry) = parse_codex_session_file(&path, since_date) {
                        entries.push(entry);
                    }
                }
            }
        }

        date += chrono::Duration::days(1);
    }

    entries.sort_by_key(|e| e.timestamp);
    entries
}

fn parse_codex_session_file(path: &Path, since_date: NaiveDate) -> Option<ParsedEntry> {
    let contents = std::fs::read_to_string(path).ok()?;

    let mut session_model = String::from("gpt-5.4");
    let mut last_usage: Option<(u64, u64, u64, u64)> = None; // (input, output, reasoning, cached)
    let mut last_timestamp: Option<DateTime<Local>> = None;

    for line in contents.lines() {
        // Extract model from any line containing "model":"
        if line.contains("\"model\":\"") {
            if let Some(model) = extract_model_from_line(line) {
                session_model = model;
            }
        }

        let entry: CodexRolloutEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.entry_type != "event_msg" {
            continue;
        }

        // Use match/continue instead of ? to avoid early return on non-token_count events
        let payload = match &entry.payload {
            Some(p) => p,
            None => continue,
        };
        if payload.payload_type != "token_count" {
            continue;
        }
        let info = match &payload.info {
            Some(i) => i,
            None => continue,
        };
        let usage = match &info.last_token_usage {
            Some(u) => u,
            None => continue,
        };

        let ts = match DateTime::parse_from_rfc3339(&entry.timestamp) {
            Ok(dt) => dt.with_timezone(&Local),
            Err(_) => continue,
        };

        if ts.date_naive() < since_date {
            continue;
        }

        // Track the LAST (cumulative) token count — do not sum
        last_usage = Some((
            usage.input_tokens.unwrap_or(0),
            usage.output_tokens.unwrap_or(0),
            usage.reasoning_output_tokens.unwrap_or(0),
            usage.cached_input_tokens.unwrap_or(0),
        ));
        last_timestamp = Some(ts);
    }

    let (input, output, reasoning, cached) = last_usage?;
    let ts = last_timestamp?;

    Some(ParsedEntry {
        timestamp: ts,
        model: session_model,
        input_tokens: input,
        output_tokens: output + reasoning, // reasoning billed as output
        cache_creation_tokens: 0,
        cache_read_tokens: cached, // OpenAI cached input → cache_read for discounted rate
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture`
Expected: all 6 parser tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/parser.rs
git commit -m "feat(parser): add Codex JSONL parsing with cumulative token handling"
```

---

### Task 6: Add UsageParser struct with daily aggregation and caching

**Files:**
- Modify: `src-tauri/src/parser.rs`

- [ ] **Step 1: Write failing tests for daily aggregation**

Add to the `tests` module:

```rust
    #[test]
    fn daily_aggregation_groups_by_date() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test");
        fs::create_dir_all(&project_dir).unwrap();

        write_claude_jsonl(&project_dir, "session.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-03-14T10:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":1000,"output_tokens":500}}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-14T11:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":2000,"output_tokens":1000}}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-15T09:00:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":500,"output_tokens":250}}}"#,
        ]);

        let parser = UsageParser::with_claude_dir(dir.path().join("projects"));
        let payload = parser.get_daily("claude", "20260314");

        assert_eq!(payload.chart_buckets.len(), 2, "should have 2 days");
        assert_eq!(payload.chart_buckets[0].label, "Mar 14");
        assert_eq!(payload.chart_buckets[1].label, "Mar 15");
        assert!(payload.total_cost > 0.0, "should have calculated costs");
        assert!(payload.total_tokens > 0, "should have summed tokens");
    }

    #[test]
    fn daily_aggregation_model_breakdown() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test");
        fs::create_dir_all(&project_dir).unwrap();

        write_claude_jsonl(&project_dir, "session.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-03-15T10:00:00Z","message":{"model":"claude-sonnet-4-6-20260301","usage":{"input_tokens":1000,"output_tokens":500}}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-15T11:00:00Z","message":{"model":"claude-opus-4-6-20260301","usage":{"input_tokens":500,"output_tokens":250}}}"#,
        ]);

        let parser = UsageParser::with_claude_dir(dir.path().join("projects"));
        let payload = parser.get_daily("claude", "20260315");

        assert_eq!(payload.model_breakdown.len(), 2, "should have 2 model summaries");
    }

    #[test]
    fn cache_returns_same_payload_within_ttl() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test");
        fs::create_dir_all(&project_dir).unwrap();

        write_claude_jsonl(&project_dir, "session.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-03-15T10:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":50}}}"#,
        ]);

        let parser = UsageParser::with_claude_dir(dir.path().join("projects"));
        let p1 = parser.get_daily("claude", "20260315");
        assert!(!p1.from_cache);

        let p2 = parser.get_daily("claude", "20260315");
        assert!(p2.from_cache, "second call should be from cache");
        assert!((p1.total_cost - p2.total_cost).abs() < 0.001);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture 2>&1 | head -20`
Expected: compilation error — `UsageParser` not defined

- [ ] **Step 3: Implement UsageParser with daily aggregation**

Add to `parser.rs` (after the Codex parsing section, before `#[cfg(test)]`):

```rust
use chrono::{Datelike, Timelike};
use std::time::Duration;

const CACHE_TTL: Duration = Duration::from_secs(120);

// ── UsageParser ──

pub struct UsageParser {
    claude_dir: PathBuf,
    codex_dir: PathBuf,
    cache: Mutex<HashMap<String, (UsagePayload, Instant)>>,
}

impl UsageParser {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            claude_dir: home.join(".claude").join("projects"),
            codex_dir: home.join(".codex").join("sessions"),
            cache: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_claude_dir(claude_dir: PathBuf) -> Self {
        Self {
            claude_dir,
            codex_dir: PathBuf::from("/nonexistent"),
            cache: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_codex_dir(codex_dir: PathBuf) -> Self {
        Self {
            claude_dir: PathBuf::from("/nonexistent"),
            codex_dir,
            cache: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_dirs(claude_dir: PathBuf, codex_dir: PathBuf) -> Self {
        Self {
            claude_dir,
            codex_dir,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    fn check_cache(&self, key: &str) -> Option<UsagePayload> {
        let cache = self.cache.lock().unwrap();
        if let Some((payload, cached_at)) = cache.get(key) {
            if cached_at.elapsed() < CACHE_TTL {
                let mut p = payload.clone();
                p.from_cache = true;
                return Some(p);
            }
        }
        None
    }

    fn store_cache(&self, key: String, payload: &UsagePayload) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(key, (payload.clone(), Instant::now()));
    }

    fn read_entries(&self, provider: &str, since: &str) -> Vec<ParsedEntry> {
        match provider {
            "claude" => read_claude_entries(&self.claude_dir, since),
            "codex" => read_codex_entries(&self.codex_dir, since),
            _ => vec![],
        }
    }

    pub fn get_daily(&self, provider: &str, since: &str) -> UsagePayload {
        let cache_key = format!("daily:{}:{}", provider, since);
        if let Some(cached) = self.check_cache(&cache_key) {
            return cached;
        }

        let entries = self.read_entries(provider, since);
        let payload = aggregate_daily(&entries);

        self.store_cache(cache_key, &payload);
        payload
    }
}

// ── Aggregation: Daily ──

fn aggregate_daily(entries: &[ParsedEntry]) -> UsagePayload {
    use std::collections::BTreeMap;

    let mut days: BTreeMap<NaiveDate, Vec<&ParsedEntry>> = BTreeMap::new();
    for entry in entries {
        days.entry(entry.timestamp.date_naive()).or_default().push(entry);
    }

    let mut total_cost = 0.0;
    let mut total_tokens = 0u64;
    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut chart_buckets = Vec::new();
    let mut model_map: HashMap<String, (String, f64, u64)> = HashMap::new();

    for (date, day_entries) in &days {
        let mut seg_map: HashMap<String, (String, f64, u64)> = HashMap::new();
        let mut day_cost = 0.0;

        for entry in day_entries {
            let cost = pricing::calculate_cost(
                &entry.model,
                entry.input_tokens,
                entry.output_tokens,
                entry.cache_creation_tokens,
                entry.cache_read_tokens,
            );
            let tokens = entry.input_tokens + entry.output_tokens
                + entry.cache_creation_tokens + entry.cache_read_tokens;

            day_cost += cost;
            total_cost += cost;
            total_tokens += tokens;
            total_input += entry.input_tokens + entry.cache_creation_tokens + entry.cache_read_tokens;
            total_output += entry.output_tokens;

            let (display, key) = normalize_model(&entry.model);
            let seg = seg_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
            seg.1 += cost;
            seg.2 += tokens;

            let m = model_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
            m.1 += cost;
            m.2 += tokens;
        }

        let segments: Vec<ChartSegment> = seg_map.into_iter()
            .map(|(key, (name, cost, tokens))| ChartSegment {
                model: name, model_key: key, cost, tokens,
            }).collect();

        chart_buckets.push(ChartBucket {
            label: date.format("%b %-d").to_string(),
            total: day_cost,
            segments,
        });
    }

    let model_breakdown: Vec<ModelSummary> = model_map
        .into_iter()
        .map(|(key, (name, cost, tokens))| ModelSummary {
            display_name: name,
            model_key: key,
            cost,
            tokens,
        })
        .collect();

    UsagePayload {
        total_cost,
        total_tokens,
        session_count: chart_buckets.len() as u32,
        input_tokens: total_input,
        output_tokens: total_output,
        chart_buckets,
        model_breakdown,
        active_block: None,
        five_hour_cost: 0.0,
        last_updated: chrono::Local::now().to_rfc3339(),
        from_cache: false,
    }
}

fn normalize_model(raw: &str) -> (&str, &str) {
    // Reuse existing normalization from models.rs
    if raw.starts_with("gpt") || raw.starts_with("o1") || raw.starts_with("o3") || raw.starts_with("o4") || raw.contains("codex") {
        crate::models::normalize_codex_model(raw)
    } else {
        crate::models::normalize_claude_model(raw)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture`
Expected: all 9 parser tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/parser.rs
git commit -m "feat(parser): add UsageParser with daily aggregation and caching"
```

---

### Task 7: Add monthly, hourly, and blocks aggregation

**Files:**
- Modify: `src-tauri/src/parser.rs`

- [ ] **Step 1: Write failing tests for monthly, hourly, and blocks**

Add to the `tests` module:

```rust
    #[test]
    fn monthly_aggregation_groups_by_month() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test");
        fs::create_dir_all(&project_dir).unwrap();

        write_claude_jsonl(&project_dir, "session.jsonl", &[
            r#"{"type":"assistant","timestamp":"2026-01-15T10:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":1000,"output_tokens":500}}}"#,
            r#"{"type":"assistant","timestamp":"2026-02-20T10:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":2000,"output_tokens":1000}}}"#,
            r#"{"type":"assistant","timestamp":"2026-03-15T10:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":500,"output_tokens":250}}}"#,
        ]);

        let parser = UsageParser::with_claude_dir(dir.path().join("projects"));
        let payload = parser.get_monthly("claude", "20260101");

        assert_eq!(payload.chart_buckets.len(), 3);
        assert_eq!(payload.chart_buckets[0].label, "Jan");
        assert_eq!(payload.chart_buckets[1].label, "Feb");
        assert_eq!(payload.chart_buckets[2].label, "Mar");
    }

    #[test]
    fn hourly_aggregation_groups_by_hour() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test");
        fs::create_dir_all(&project_dir).unwrap();

        // Use today's date so the hourly parser includes them
        let today = Local::now().format("%Y-%m-%dT").to_string();
        write_claude_jsonl(&project_dir, "session.jsonl", &[
            &format!(r#"{{"type":"assistant","timestamp":"{}09:00:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":1000,"output_tokens":500}}}}}}"#, today),
            &format!(r#"{{"type":"assistant","timestamp":"{}09:30:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":2000,"output_tokens":1000}}}}}}"#, today),
            &format!(r#"{{"type":"assistant","timestamp":"{}10:00:00Z","message":{{"model":"claude-opus-4-6","usage":{{"input_tokens":500,"output_tokens":250}}}}}}"#, today),
        ]);

        let today_str = Local::now().format("%Y%m%d").to_string();
        let parser = UsageParser::with_claude_dir(dir.path().join("projects"));
        let payload = parser.get_hourly("claude", &today_str);

        assert!(payload.chart_buckets.len() >= 2, "should have at least 2 hour buckets");
        assert!(payload.total_cost > 0.0);
    }

    #[test]
    fn blocks_detects_activity_windows() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("projects").join("test");
        fs::create_dir_all(&project_dir).unwrap();

        let today = Local::now().format("%Y-%m-%dT").to_string();
        write_claude_jsonl(&project_dir, "session.jsonl", &[
            // Block 1: 09:00-09:10
            &format!(r#"{{"type":"assistant","timestamp":"{}09:00:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":1000,"output_tokens":500}}}}}}"#, today),
            &format!(r#"{{"type":"assistant","timestamp":"{}09:10:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":1000,"output_tokens":500}}}}}}"#, today),
            // Gap of >30 min
            // Block 2: 10:00-10:05
            &format!(r#"{{"type":"assistant","timestamp":"{}10:00:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":500,"output_tokens":250}}}}}}"#, today),
            &format!(r#"{{"type":"assistant","timestamp":"{}10:05:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":500,"output_tokens":250}}}}}}"#, today),
        ]);

        let today_str = Local::now().format("%Y%m%d").to_string();
        let parser = UsageParser::with_claude_dir(dir.path().join("projects"));
        let payload = parser.get_blocks("claude", &today_str);

        assert_eq!(payload.chart_buckets.len(), 2, "should detect 2 activity blocks separated by gap");
        assert!(payload.active_block.is_some() || payload.total_cost > 0.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture 2>&1 | head -20`
Expected: compilation error — `get_monthly`, `get_hourly`, `get_blocks` not defined

- [ ] **Step 3: Implement monthly aggregation**

Add to the `impl UsageParser` block:

```rust
    pub fn get_monthly(&self, provider: &str, since: &str) -> UsagePayload {
        let cache_key = format!("monthly:{}:{}", provider, since);
        if let Some(cached) = self.check_cache(&cache_key) {
            return cached;
        }

        let entries = self.read_entries(provider, since);
        let payload = aggregate_monthly(&entries);

        self.store_cache(cache_key, &payload);
        payload
    }
```

Add the `aggregate_monthly` function (after `aggregate_daily`):

```rust
fn aggregate_monthly(entries: &[ParsedEntry]) -> UsagePayload {
    use std::collections::BTreeMap;

    let mut months: BTreeMap<String, Vec<&ParsedEntry>> = BTreeMap::new();
    for entry in entries {
        let key = entry.timestamp.format("%Y-%m").to_string();
        months.entry(key).or_default().push(entry);
    }

    let mut total_cost = 0.0;
    let mut total_tokens = 0u64;
    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut chart_buckets = Vec::new();
    let mut model_map: HashMap<String, (String, f64, u64)> = HashMap::new();

    for (month_key, month_entries) in &months {
        let mut seg_map: HashMap<String, (String, f64, u64)> = HashMap::new();
        let mut month_cost = 0.0;

        for entry in month_entries {
            let cost = pricing::calculate_cost(
                &entry.model, entry.input_tokens, entry.output_tokens,
                entry.cache_creation_tokens, entry.cache_read_tokens,
            );
            let tokens = entry.input_tokens + entry.output_tokens
                + entry.cache_creation_tokens + entry.cache_read_tokens;

            month_cost += cost;
            total_cost += cost;
            total_tokens += tokens;
            total_input += entry.input_tokens + entry.cache_creation_tokens + entry.cache_read_tokens;
            total_output += entry.output_tokens;

            let (display, key) = normalize_model(&entry.model);
            let seg = seg_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
            seg.1 += cost;
            seg.2 += tokens;

            let m = model_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
            m.1 += cost;
            m.2 += tokens;
        }

        let label = NaiveDate::parse_from_str(&format!("{}-01", month_key), "%Y-%m-%d")
            .map(|d| d.format("%b").to_string())
            .unwrap_or_else(|_| month_key.clone());

        let segments: Vec<ChartSegment> = seg_map.into_iter()
            .map(|(key, (name, cost, tokens))| ChartSegment {
                model: name, model_key: key, cost, tokens,
            }).collect();

        chart_buckets.push(ChartBucket {
            label,
            total: month_cost,
            segments,
        });
    }

    let model_breakdown: Vec<ModelSummary> = model_map
        .into_iter()
        .map(|(key, (name, cost, tokens))| ModelSummary {
            display_name: name, model_key: key, cost, tokens,
        })
        .collect();

    UsagePayload {
        total_cost, total_tokens,
        session_count: chart_buckets.len() as u32,
        input_tokens: total_input, output_tokens: total_output,
        chart_buckets, model_breakdown,
        active_block: None, five_hour_cost: 0.0,
        last_updated: chrono::Local::now().to_rfc3339(),
        from_cache: false,
    }
}
```

- [ ] **Step 4: Implement hourly aggregation**

Add to the `impl UsageParser` block:

```rust
    pub fn get_hourly(&self, provider: &str, since: &str) -> UsagePayload {
        let cache_key = format!("hourly:{}:{}", provider, since);
        if let Some(cached) = self.check_cache(&cache_key) {
            return cached;
        }

        let entries = self.read_entries(provider, since);
        let payload = aggregate_hourly(&entries);

        self.store_cache(cache_key, &payload);
        payload
    }
```

Add the `aggregate_hourly` function:

```rust
fn format_hour(h: u32) -> String {
    match h {
        0 => "12AM".into(),
        1..=11 => format!("{}AM", h),
        12 => "12PM".into(),
        _ => format!("{}PM", h - 12),
    }
}

fn aggregate_hourly(entries: &[ParsedEntry]) -> UsagePayload {
    let mut hours: HashMap<u32, Vec<&ParsedEntry>> = HashMap::new();
    for entry in entries {
        hours.entry(entry.timestamp.hour()).or_default().push(entry);
    }

    let now = Local::now();
    let min_hour = hours.keys().copied().min().unwrap_or(now.hour());
    let max_hour = now.hour();

    let mut total_cost = 0.0;
    let mut total_tokens = 0u64;
    let mut chart_buckets = Vec::new();
    let mut model_map: HashMap<String, (String, f64, u64)> = HashMap::new();

    for h in min_hour..=max_hour {
        let mut seg_map: HashMap<String, (String, f64, u64)> = HashMap::new();
        let mut hour_cost = 0.0;

        if let Some(hour_entries) = hours.get(&h) {
            for entry in hour_entries {
                let cost = pricing::calculate_cost(
                    &entry.model, entry.input_tokens, entry.output_tokens,
                    entry.cache_creation_tokens, entry.cache_read_tokens,
                );
                let tokens = entry.input_tokens + entry.output_tokens
                    + entry.cache_creation_tokens + entry.cache_read_tokens;

                hour_cost += cost;
                total_cost += cost;
                total_tokens += tokens;

                let (display, key) = normalize_model(&entry.model);
                let seg = seg_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
                seg.1 += cost;
                seg.2 += tokens;

                let m = model_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
                m.1 += cost;
                m.2 += tokens;
            }
        }

        let segments: Vec<ChartSegment> = seg_map.into_iter()
            .map(|(key, (name, cost, tokens))| ChartSegment {
                model: name, model_key: key, cost, tokens,
            }).collect();

        chart_buckets.push(ChartBucket {
            label: format_hour(h),
            total: hour_cost,
            segments,
        });
    }

    let model_breakdown: Vec<ModelSummary> = model_map
        .into_iter()
        .map(|(key, (name, cost, tokens))| ModelSummary {
            display_name: name, model_key: key, cost, tokens,
        })
        .collect();

    UsagePayload {
        total_cost, total_tokens,
        session_count: chart_buckets.iter().filter(|b| b.total > 0.0).count() as u32,
        input_tokens: 0, output_tokens: 0,
        chart_buckets, model_breakdown,
        active_block: None, five_hour_cost: 0.0,
        last_updated: chrono::Local::now().to_rfc3339(),
        from_cache: false,
    }
}
```

- [ ] **Step 5: Implement blocks aggregation**

Add to the `impl UsageParser` block:

```rust
    pub fn get_blocks(&self, provider: &str, since: &str) -> UsagePayload {
        let cache_key = format!("blocks:{}:{}", provider, since);
        if let Some(cached) = self.check_cache(&cache_key) {
            return cached;
        }

        let entries = self.read_entries(provider, since);
        let payload = aggregate_blocks(&entries);

        self.store_cache(cache_key, &payload);
        payload
    }
```

Add the `aggregate_blocks` function:

```rust
fn aggregate_blocks(entries: &[ParsedEntry]) -> UsagePayload {
    let gap_threshold = chrono::Duration::minutes(30);
    if entries.is_empty() {
        return empty_payload();
    }

    // Split entries into blocks separated by gaps > 30 min
    let mut blocks: Vec<Vec<&ParsedEntry>> = Vec::new();
    let mut current_block: Vec<&ParsedEntry> = vec![&entries[0]];

    for entry in &entries[1..] {
        let last = current_block.last().unwrap();
        if entry.timestamp.signed_duration_since(last.timestamp) > gap_threshold {
            blocks.push(current_block);
            current_block = vec![entry];
        } else {
            current_block.push(entry);
        }
    }
    blocks.push(current_block);

    let mut total_cost = 0.0;
    let mut total_tokens = 0u64;
    let mut chart_buckets = Vec::new();
    let mut model_map: HashMap<String, (String, f64, u64)> = HashMap::new();

    for block in &blocks {
        let mut block_cost = 0.0;
        let mut seg_map: HashMap<String, (String, f64, u64)> = HashMap::new();

        for entry in block {
            let cost = pricing::calculate_cost(
                &entry.model, entry.input_tokens, entry.output_tokens,
                entry.cache_creation_tokens, entry.cache_read_tokens,
            );
            let tokens = entry.input_tokens + entry.output_tokens
                + entry.cache_creation_tokens + entry.cache_read_tokens;

            block_cost += cost;
            total_cost += cost;
            total_tokens += tokens;

            let (display, key) = normalize_model(&entry.model);
            let seg = seg_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
            seg.1 += cost;
            seg.2 += tokens;

            let m = model_map.entry(key.to_string()).or_insert((display.to_string(), 0.0, 0));
            m.1 += cost;
            m.2 += tokens;
        }

        let start = block.first().unwrap().timestamp;
        let label = start.format("%-I%P").to_string();
        let segments: Vec<ChartSegment> = seg_map.into_iter()
            .map(|(key, (name, cost, tokens))| ChartSegment {
                model: name, model_key: key, cost, tokens,
            }).collect();

        chart_buckets.push(ChartBucket {
            label,
            total: block_cost,
            segments,
        });
    }

    // Active block = last block, but only if recent (within 30 min of now)
    let now = Local::now();
    let active_block = blocks.last().and_then(|block| {
        let start = block.first().unwrap().timestamp;
        let end = block.last().unwrap().timestamp;
        let elapsed_hours = (end - start).num_seconds() as f64 / 3600.0;
        let block_cost: f64 = block.iter().map(|e| {
            pricing::calculate_cost(&e.model, e.input_tokens, e.output_tokens,
                e.cache_creation_tokens, e.cache_read_tokens)
        }).sum();

        let burn_rate = if elapsed_hours > 0.01 { block_cost / elapsed_hours } else { 0.0 };
        let is_active = (now - end).num_minutes() < 30;

        Some(ActiveBlock {
            cost: block_cost,
            burn_rate_per_hour: burn_rate,
            projected_cost: burn_rate * 5.0,
            is_active,
        })
    });

    let five_hour_cost = active_block.as_ref().map(|b| b.cost).unwrap_or(total_cost);

    let model_breakdown: Vec<ModelSummary> = model_map
        .into_iter()
        .map(|(key, (name, cost, tokens))| ModelSummary {
            display_name: name, model_key: key, cost, tokens,
        })
        .collect();

    UsagePayload {
        total_cost, total_tokens,
        session_count: chart_buckets.len() as u32,
        input_tokens: 0, output_tokens: 0,
        chart_buckets, model_breakdown,
        active_block, five_hour_cost,
        last_updated: chrono::Local::now().to_rfc3339(),
        from_cache: false,
    }
}

fn empty_payload() -> UsagePayload {
    UsagePayload {
        total_cost: 0.0, total_tokens: 0, session_count: 0,
        input_tokens: 0, output_tokens: 0,
        chart_buckets: vec![], model_breakdown: vec![],
        active_block: None, five_hour_cost: 0.0,
        last_updated: chrono::Local::now().to_rfc3339(),
        from_cache: false,
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib parser -- --nocapture`
Expected: all 12 parser tests PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/parser.rs
git commit -m "feat(parser): add monthly, hourly, and blocks aggregation"
```

---

## Chunk 3: Integration — Wire Up and Remove ccusage

### Task 8: Finalize module declarations in lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Remove ccusage and hourly modules, keep parser and pricing**

In `src-tauri/src/lib.rs`, update the module declarations (parser and pricing were added in Tasks 1 and 4; now remove the old modules):

```rust
// Before:
mod ccusage;
mod commands;
mod hourly;
mod models;
mod parser;
mod pricing;

// After:
mod commands;
mod models;
mod parser;
mod pricing;
```

- [ ] **Step 2: Verify it compiles (will fail — commands.rs still references ccusage)**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: errors in `commands.rs` and `lib.rs` about missing `ccusage` references — this is expected, we'll fix in the next tasks.

- [ ] **Step 3: Commit module registration change**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor(lib): register parser/pricing modules, remove ccusage/hourly"
```

---

### Task 9: Rewrite commands.rs to use parser

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Rewrite AppState and commands**

Replace the entire contents of `src-tauri/src/commands.rs` with:

```rust
use crate::models::*;
use crate::parser::UsageParser;
use chrono::{Datelike, Local, NaiveDate};
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

pub struct AppState {
    pub parser: Arc<UsageParser>,
    pub refresh_interval: Arc<RwLock<u64>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            parser: Arc::new(UsageParser::new()),
            refresh_interval: Arc::new(RwLock::new(30)),
        }
    }
}

#[tauri::command]
pub async fn set_refresh_interval(interval: u64, state: State<'_, AppState>) -> Result<(), String> {
    let mut current = state.refresh_interval.write().await;
    *current = interval;
    Ok(())
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.parser.clear_cache();
    Ok(())
}

#[tauri::command]
pub async fn get_usage_data(
    provider: String,
    period: String,
    state: State<'_, AppState>,
) -> Result<UsagePayload, String> {
    let parser = &state.parser;

    match provider.as_str() {
        "claude" | "codex" => Ok(get_provider_data(parser, &provider, &period)?),
        "all" => {
            let claude = get_provider_data(parser, "claude", &period)?;
            let codex = get_provider_data(parser, "codex", &period)?;
            Ok(merge_payloads(claude, codex))
        }
        _ => Err(format!("Unknown provider: {}", provider)),
    }
}

fn get_provider_data(parser: &UsageParser, provider: &str, period: &str) -> Result<UsagePayload, String> {
    let now = Local::now();
    let today = now.format("%Y%m%d").to_string();

    Ok(match period {
        // Codex has no 5-hour billing window concept — use daily for codex 5h
        "5h" if provider == "codex" => parser.get_daily(provider, &today),
        "5h" => parser.get_blocks(provider, &today),
        "day" => parser.get_hourly(provider, &today),
        "week" => {
            let week_start = (now - chrono::Duration::days(now.weekday().num_days_from_monday() as i64))
                .format("%Y%m%d").to_string();
            parser.get_daily(provider, &week_start)
        }
        "month" => {
            let month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
                .unwrap().format("%Y%m%d").to_string();
            parser.get_daily(provider, &month_start)
        }
        "year" => {
            let year_start = NaiveDate::from_ymd_opt(now.year(), 1, 1)
                .unwrap().format("%Y%m%d").to_string();
            parser.get_monthly(provider, &year_start)
        }
        _ => return Err(format!("Unknown period: {}", period)),
    })
}

fn empty_payload() -> UsagePayload {
    UsagePayload {
        total_cost: 0.0, total_tokens: 0, session_count: 0,
        input_tokens: 0, output_tokens: 0,
        chart_buckets: vec![], model_breakdown: vec![],
        active_block: None, five_hour_cost: 0.0,
        last_updated: chrono::Local::now().to_rfc3339(),
        from_cache: false,
    }
}

fn merge_payloads(mut c: UsagePayload, x: UsagePayload) -> UsagePayload {
    let mut bucket_map: std::collections::BTreeMap<String, ChartBucket> =
        std::collections::BTreeMap::new();
    for b in c.chart_buckets.iter().chain(x.chart_buckets.iter()) {
        let entry = bucket_map.entry(b.label.clone()).or_insert_with(|| ChartBucket {
            label: b.label.clone(),
            total: 0.0,
            segments: vec![],
        });
        entry.total += b.total;
        entry.segments.extend(b.segments.clone());
    }

    c.total_cost += x.total_cost;
    c.total_tokens += x.total_tokens;
    c.session_count += x.session_count;
    c.input_tokens += x.input_tokens;
    c.output_tokens += x.output_tokens;
    c.chart_buckets = bucket_map.into_values().collect();
    c.model_breakdown.extend(x.model_breakdown);
    c.five_hour_cost += x.five_hour_cost;
    c.from_cache = c.from_cache && x.from_cache;
    c
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: may still have errors in `lib.rs` — we'll fix those next.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor(commands): rewrite to use UsageParser, remove ccusage dependency"
```

---

### Task 10: Simplify lib.rs background loop

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Rewrite background_loop and update_tray_title**

Replace `update_tray_title` and `background_loop` in `lib.rs`:

```rust
async fn update_tray_title(app: &tauri::AppHandle, state: &AppState) {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let payload = state.parser.get_daily("claude", &today);
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_title(Some(&format!("${:.2}", payload.total_cost)));
    }
}

async fn background_loop(app: tauri::AppHandle) {
    // Small delay to let the frontend initialize
    tokio::time::sleep(Duration::from_secs(1)).await;

    let state = app.state::<AppState>();

    // Update tray title immediately
    update_tray_title(&app, &state).await;

    let mut update_counter: u64 = 0;
    loop {
        let interval_secs = {
            let interval = state.refresh_interval.read().await;
            *interval
        };

        if interval_secs == 0 {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        update_counter += 1;

        // Clear cache so next fetch reads fresh data
        state.parser.clear_cache();
        update_tray_title(&app, &state).await;
        let _ = app.emit("data-updated", update_counter);
    }
}
```

Also update the `invoke_handler` to remove the deleted commands:

```rust
        .invoke_handler(tauri::generate_handler![
            commands::get_usage_data,
            commands::set_refresh_interval,
            commands::clear_cache,
        ])
```

- [ ] **Step 2: Clean up models.rs — remove ccusage-only types**

Remove these types from `models.rs` that are no longer needed (parser produces `UsagePayload` directly):
- `ClaudeDailyResponse`, `ClaudeDayEntry`, `ClaudeModelBreakdown`
- `ClaudeMonthlyResponse`, `ClaudeMonthEntry`
- `ClaudeBlocksResponse`, `ClaudeBlockEntry`, `BurnRate`, `Projection`
- `CodexDailyResponse`, `CodexDayEntry`, `CodexModelUsage`

Keep:
- `UsagePayload`, `ChartBucket`, `ChartSegment`, `ModelSummary`, `ActiveBlock`
- `SetupStatus` (may still be needed by frontend temporarily)
- `normalize_claude_model`, `normalize_codex_model`

Also remove the tests in `models.rs` that test the deleted types (deserialization tests for `ClaudeDailyResponse`, etc.) and the tests in `commands.rs` that test deleted transform functions.

- [ ] **Step 3: Delete old files**

```bash
rm src-tauri/src/ccusage.rs
rm src-tauri/src/hourly.rs
```

- [ ] **Step 4: Verify it compiles and tests pass**

Run: `cd src-tauri && cargo test 2>&1 | tail -20`
Expected: all tests PASS, no compilation errors

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: remove ccusage.rs and hourly.rs, simplify lib.rs background loop"
```

---

### Task 11: Simplify frontend — remove setup flow

**Files:**
- Modify: `src/App.svelte`
- Modify: `src/lib/stores/usage.ts`
- Modify: `src/lib/components/SetupScreen.svelte`

- [ ] **Step 1: Remove initializeApp and checkSetup from usage.ts**

In `src/lib/stores/usage.ts`, delete the `initializeApp` and `checkSetup` functions and their imports. Keep `fetchData`, `warmCache`, `warmAllPeriods`, and the stores.

- [ ] **Step 2: Simplify App.svelte**

In `src/App.svelte`:

**Update imports** — remove `setupStatus`, `initializeApp`, `checkSetup`:

```typescript
  import {
    activeProvider,
    activePeriod,
    usageData,
    isLoading,
    fetchData,
    warmCache,
    warmAllPeriods,
  } from "./lib/stores/usage.js";
```

**Remove** the `status` state variable and its subscription from the `$effect` block. The `$effect` that subscribed to `setupStatus` and `isLoading` should only subscribe to `isLoading`:

```typescript
  $effect(() => {
    const unsub1 = usageData.subscribe((v) => (data = v));
    const unsub2 = isLoading.subscribe((v) => (loading = v));
    const unsub3 = settings.subscribe((s) => (brandTheming = s.brandTheming));
    return () => { unsub1(); unsub2(); unsub3(); };
  });
```

**Simplify onMount** — remove checkSetup/initializeApp and setup-complete listener:

```typescript
  onMount(async () => {
    try {
      const saved = await loadSettings();
      applyTheme(saved.theme);
      provider = saved.defaultProvider;
      period = saved.defaultPeriod;
      activeProvider.set(provider);
      activePeriod.set(period);
    } catch {
      // Settings load failed — continue with defaults
    }

    await fetchData(provider, period);
    warmAllPeriods(provider, period);
    warmAllPeriods(provider === "claude" ? "codex" : "claude");
    appReady = true;

    const pop = document.querySelector('.pop') as HTMLElement;
    let observer: ResizeObserver | undefined;
    if (pop) {
      observer = new ResizeObserver(() => resizeToContent());
      observer.observe(pop);
    }

    const unlisten = await listen("data-updated", () => {
      dataKey = `${provider}-${period}-${Date.now()}`;
      fetchData(provider, period);
    });
    return () => {
      unlisten(); observer?.disconnect();
      if (resizeTimer) clearTimeout(resizeTimer);
      cancelAnimationFrame(resizeRaf);
    };
  });
```

**Update template** — replace `{:else if !status.ready}` with empty-state check:

```svelte
  {:else if appReady && !data}
    <SetupScreen />
```

- [ ] **Step 3: Repurpose SetupScreen as empty state**

Replace `SetupScreen.svelte` contents to show a simple empty state message (no more "installing" state):

```svelte
<div class="setup">
  <div class="setup-icon">📊</div>
  <div class="setup-title">No usage data found</div>
  <div class="setup-text">Use Claude Code or Codex CLI to start tracking your token usage.</div>
</div>
```

(Keep existing styles or simplify them.)

- [ ] **Step 3b: Remove SetupStatus from TypeScript types**

In `src/lib/types/index.ts`, remove the `SetupStatus` interface (it is now dead code since `checkSetup` and `initializeApp` were removed).

- [ ] **Step 4: Verify the app builds**

Run: `cd /Users/michael/Documents/GitHub/TokenMonitor && npm run build 2>&1 | tail -10`
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
git add src/App.svelte src/lib/stores/usage.ts src/lib/components/SetupScreen.svelte src/lib/types/index.ts
git commit -m "refactor(frontend): remove setup flow, add empty state screen"
```

---

## Chunk 4: Final Verification

### Task 12: Run full test suite and verify

**Files:** None (verification only)

- [ ] **Step 1: Run all Rust tests**

Run: `cd src-tauri && cargo test 2>&1`
Expected: all tests PASS

- [ ] **Step 2: Run Rust clippy**

Run: `cd src-tauri && cargo clippy 2>&1 | tail -20`
Expected: no errors (warnings acceptable)

- [ ] **Step 3: Build the Tauri app**

Run: `cd /Users/michael/Documents/GitHub/TokenMonitor && npm run tauri build 2>&1 | tail -20`
Expected: build succeeds, producing a `.app` bundle

- [ ] **Step 4: Commit any final fixes**

If any fixes were needed, commit them:

```bash
git add -A
git commit -m "fix: address test/lint issues from ccusage removal"
```
