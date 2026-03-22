# Spec: Subagent Usage Attribution with Rich Stats

Status: Approved

Owner: TokenMonitor

Last Updated: 2026-03-21

Builds on: `docs/2026-03-21-subagent-usage-design.md`

## Summary

Add a local-first `Main` vs `Subagents` usage breakdown for both Claude Code and Codex, enriched with delegation intensity metrics, per-scope model usage, and per-scope change attribution.

The existing design doc (`2026-03-21-subagent-usage-design.md`) defines the core attribution model. This spec extends the V1 surface with three additional stat families that were not in the original design:

1. **Delegation intensity** — spawn count, avg cost/agent
2. **Model usage by scope** — top 2 models per scope (main vs subagent)
3. **Change attribution by scope** — added/removed lines split by main vs subagent

## Scope Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Parser approach | Extend `ParsedEntry` with `session_key` + `agent_scope` | Design doc spec, maximally correct |
| UI layout | Dense two-card grid with proportion bar | Matches existing MetricsRow pattern, packs all three stat families compactly |
| Visibility | All periods including 5h, independent of ModelList | Subagent data is useful even in the 5h rate-limit view |
| Settings toggle | None | Always visible when subagent data exists; no user preference needed |
| Named agents | Not in V1 | Phase 2 — slugs and roles are available in logs but not surfaced yet |

## Data Model

### Rust — `ParsedEntry` extensions

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentScope {
    #[default]
    Main,
    Subagent,
}

pub struct ParsedEntry {
    // ...existing fields...
    pub session_key: String,
    pub agent_scope: AgentScope,
}
```

`ParsedChangeEvent` also gains an `agent_scope: AgentScope` field.

### Rust — aggregation output (`subagent_stats.rs`)

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ScopeModelUsage {
    pub display_name: String,
    pub model_key: String,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopeUsageSummary {
    pub cost: f64,
    pub tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub session_count: u32,
    pub pct_of_total_cost: Option<f64>,
    pub top_models: Vec<ScopeModelUsage>,
    pub added_lines: u64,
    pub removed_lines: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubagentStats {
    pub main: ScopeUsageSummary,
    pub subagents: ScopeUsageSummary,
}
```

### TypeScript — `types/index.ts`

```typescript
export interface ScopeModelUsage {
  display_name: string;
  model_key: string;
  cost: number;
}

export interface ScopeUsageSummary {
  cost: number;
  tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  session_count: number;
  pct_of_total_cost: number | null;
  top_models: ScopeModelUsage[];
  added_lines: number;
  removed_lines: number;
}

export interface SubagentStats {
  main: ScopeUsageSummary;
  subagents: ScopeUsageSummary;
}
```

`UsagePayload` gains: `subagent_stats: SubagentStats | null`

### Design notes

- `top_models` capped at 2 per scope (enough for the card UI, avoids unbounded lists)
- Change lines live directly on `ScopeUsageSummary` — flat, not nested
- `cache_write_tokens` maps to the sum of `cache_creation_5m_tokens + cache_creation_1h_tokens` from `ParsedEntry`. Will be `0` for Codex (Codex logs don't expose cache creation)
- `pct_of_total_cost` is backend-computed for stable display
- `tokens` equals `input_tokens + output_tokens + cache_read_tokens + cache_write_tokens` (consistent with `total_tokens` elsewhere)

## Parsing Design

### Claude

Add three fields to `ClaudeJsonlEntry` deserialization:

```rust
#[serde(rename = "isSidechain", default)]
is_sidechain: Option<bool>,
#[serde(rename = "sessionId", default)]
session_id: Option<String>,
#[serde(rename = "agentId", default)]
agent_id: Option<String>,
```

Attribution rules:
- `is_sidechain == Some(true)` → `AgentScope::Subagent`
- everything else → `AgentScope::Main`
- `session_key = "claude:{sessionId}:{agentId|main}"` — falls back to `"claude:file:{path}"` when `sessionId` is absent

Dedupe hash update: currently `message.id + requestId`. Changes to include `isSidechain + agentId` in the hash inputs (e.g., `isSidechain + agentId + message.id + requestId`) to prevent root and sidechain entries from colliding during recursive glob. Note: `session_key` itself is not used in the hash because it depends on `sessionId` which may be computed at a different point in the parse loop. The hash only needs enough to distinguish root vs sidechain entries — `isSidechain` and `agentId` are sufficient.

`ParsedChangeEvent` inherits `agent_scope` from the entry that contains the `tool_use`.

### Codex

Add a `session_meta` parse at the start of `parse_codex_session_file`. Runs once per file, sets file-level state:

```rust
let mut session_key = format!("codex-file:{}", path_to_string(path));
let mut agent_scope = AgentScope::Main;

// First session_meta line encountered:
if entry_type == "session_meta" {
    if let Some(id) = payload.get("id").and_then(Value::as_str) {
        session_key = format!("codex:{id}");
    }
    if payload.pointer("/source/subagent").is_some() {
        agent_scope = AgentScope::Subagent;
    }
}
```

All `ParsedEntry` and `ParsedChangeEvent` from the file inherit the same scope and key. Codex subagent attribution is file-level (one file = one agent scope).

### Compatibility

Older logs missing metadata default to `AgentScope::Main` with file-path-based session key. No breakage for existing data.

## Aggregation

### Core function

```rust
pub fn aggregate_subagent_stats(
    entries: &[ParsedEntry],
    change_events: &[ParsedChangeEvent],
    total_cost: f64,
) -> Option<SubagentStats>
```

Algorithm:
1. Partition entries by `agent_scope`
2. Per scope: sum cost (via existing pricing helper), tokens, input/output/cache
3. Count distinct `session_key` values per scope → `session_count`
4. Sort models by cost descending, take top 2 → `top_models`
5. Partition `change_events` by `agent_scope`, sum added/removed lines per scope
6. Compute `pct_of_total_cost = scope_cost / total_cost * 100` when `total_cost > 0`
7. Return `None` if the subagent scope has zero cost AND zero tokens (hides the UI section). This handles both the case where all entries are `Main` and the edge case where a subagent session_meta exists but contains no meaningful token usage

### Wiring

In `get_provider_data` (after change stats):

```rust
payload.subagent_stats = aggregate_subagent_stats(&entries, &change_events, payload.total_cost);
```

### Merge for "all" tab

```rust
merged.subagent_stats = merge_subagent_stats(
    claude.subagent_stats,
    codex.subagent_stats,
    merged.total_cost,
);
```

Merge sums all numeric fields per scope, concatenates and re-sorts `top_models` (top 2 from combined), recomputes `pct_of_total_cost` against merged total.

### Period filtering

Uses the same `load_change_events_for_period` bounded date range — no accumulation.

## Invariant

**Top-line totals must not change.** This feature only decomposes existing data. If `total_cost` or `total_tokens` differ after the feature lands, the parser has regressed. The existing `compare_all_with_ccusage` debug test serves as the regression guard.

## UI Design

### Component: `SubagentList.svelte`

Props: `{ stats: SubagentStats }`

Layout (Dense Cards):
- Full-width proportion bar (main `--scope-main` / subagent `--scope-sub`)
- Two side-by-side cards below:

**Main card:**
- Colored dot + "Main" label
- Cost (13px) + tokens + percentage
- Divider → top 2 models (dot + name + cost, 7.5px)
- Change lines: `+added / −removed`

**Subagent card:**
- Colored dot + "Subagents" label
- Cost (13px) + tokens + percentage + spawn count (`session_count`)
- Divider → top 2 models (dot + name + cost, 7.5px)
- Change lines: `+added / −removed`

### Placement in `App.svelte`

```svelte
{#if data.subagent_stats}
  <div class="hr"></div>
  <SubagentList stats={data.subagent_stats} />
{/if}
```

Rendered after `ModelList` in non-5h periods. Rendered independently (after usage bars) in 5h view. Hidden when `subagent_stats` is `null`.

### Styling

- Reuses `--surface-2`, `--t1`–`--t4`, `--ch-plus`, `--ch-minus`
- Model dot colors from existing `modelColor()` utility
- Two new CSS custom properties: `--scope-main` and `--scope-sub` for scope dot/bar colors
- All CSS scoped to component

## File Map

### New files

| File | Responsibility |
|---|---|
| `src-tauri/src/subagent_stats.rs` | `AgentScope` enum, aggregation types, `aggregate_subagent_stats`, `merge_subagent_stats` |
| `src/lib/components/SubagentList.svelte` | Dense two-card UI component |

### Modified files

| File | What changes |
|---|---|
| `src-tauri/src/lib.rs` | Add `mod subagent_stats;` |
| `src-tauri/src/parser.rs` | Add `session_key` + `agent_scope` to `ParsedEntry`; parse `isSidechain`/`sessionId`/`agentId` for Claude; parse `session_meta` for Codex; update dedupe hash; add `agent_scope` to `ParsedChangeEvent` |
| `src-tauri/src/models.rs` | Add `subagent_stats: Option<SubagentStats>` to `UsagePayload` |
| `src-tauri/src/commands.rs` | Wire `aggregate_subagent_stats` into `get_provider_data`; add `merge_subagent_stats` call in `merge_payloads`; pass entries to aggregation |
| `src/lib/types/index.ts` | Add `SubagentStats`, `ScopeUsageSummary`, `ScopeModelUsage` interfaces; update `UsagePayload` |
| `src/lib/stores/usage.ts` | Add `subagent_stats: null` to `emptyPayload()` |
| `src/App.svelte` | Import and render `SubagentList` |

## Testing Plan

### Rust parser tests (7)

1. Claude root session entry → `AgentScope::Main`, session_key contains `"main"`
2. Claude subagent file with `isSidechain: true` → `AgentScope::Subagent`, session_key contains agentId
3. Claude dedupe does not collapse root + sidechain entries sharing same message/request IDs
4. Codex file with no `session_meta.source.subagent` → `AgentScope::Main`
5. Codex file with `source.subagent.other` → `AgentScope::Subagent`
6. Codex file with `source.subagent.thread_spawn` → `AgentScope::Subagent`
7. Multiple entries in same file produce same `session_key`

### Aggregation tests (5)

1. All-main entries (or subagent entries with zero cost and tokens) → returns `None`
2. Mixed main + subagent → correct cost/token split, correct session counts
3. `top_models` capped at 2 per scope, sorted by cost descending
4. Change events partitioned correctly by scope
5. Merged "all" payload recomputes `pct_of_total_cost` from merged total

### Frontend tests (3)

1. Component hidden when `subagent_stats` is `null`
2. Both cards render with correct values when subagent usage exists
3. Percentage badge only on subagent card

### Regression

- Existing `compare_all_with_ccusage` debug test continues passing (top-line totals unchanged)

## Acceptance Criteria

1. `UsagePayload` includes `subagent_stats` for `claude`, `codex`, and `all` providers
2. Claude totals remain unchanged before vs after the feature
3. Codex sessions with `session_meta.payload.source.subagent` are counted under `subagents`
4. Dense two-card UI shows cost, tokens, spawn count, top models, and change lines per scope
5. Proportion bar accurately reflects cost split
6. Section hidden when no subagent usage exists
7. No chart, rate-limit, or model-breakdown regressions
