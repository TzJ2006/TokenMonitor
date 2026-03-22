# Design: Change Statistics for Claude Code and Codex

Status: Proposed

Owner: TokenMonitor

Last Updated: 2026-03-20

## Summary

TokenMonitor currently tracks spend, tokens, model mix, and rate-limit context from local Claude Code and Codex logs. It does not track model-attributed code change volume.

This design adds a new family of local-first "change statistics" derived from provider logs for both Claude Code and Codex. The goal is to answer questions like:

- How much did the model change?
- Was the work mostly code, docs, or config?
- How many files were touched?
- Was the session patch-heavy or rewrite-heavy?
- What was the spend or token cost relative to changed lines?

The design deliberately avoids claiming perfect authorship. User-facing copy should prefer "model-attributed changes" or "changed lines" over "lines written by the model."

## Problem

The current product shows usage cost and token volume, but not output shape. Two sessions with identical spend can be very different in practice:

- one may be a large code refactor across many files
- one may be mostly prose, plans, or investigation
- one may contain many small patch edits
- one may be dominated by full-file rewrites

For heavy Claude Code and Codex users, spend without change volume is incomplete. The app needs an additional local metric family that is:

- provider-aware
- period-aware
- honest about confidence and caveats
- compact enough to fit the existing popover UI

## Goals

- Add robust change metrics for both Claude Code and Codex from local logs only.
- Preserve current provider and period semantics: `claude`, `codex`, `all`; `5h`, `day`, `week`, `month`, `year`.
- Keep the implementation incremental and compatible with existing parser caching.
- Distinguish exact or high-confidence metrics from estimated ones.
- Support future UI surfaces such as top-row stats, model list enrichment, and chart metric toggles.

## Non-Goals

- Perfectly reconstruct every line ever authored by a model.
- Attribute shell edits with full certainty when the shell command rewrites files outside structured edit tools.
- Compute semantic quality or a synthetic "productivity score."
- Use git history as a hard dependency for v1.
- Parse programming languages semantically in v1. File extension classification is sufficient initially.

## Product Language

The product should not say:

- "lines written"
- "true model LOC"
- "pure AI output"

The product should say:

- "changed lines"
- "model-attributed changes"
- "lines added / removed"
- "raw change volume"
- "surviving changes" only if the metric is explicitly computed

## Accuracy Tiers

Every proposed metric falls into one of these tiers:

### Tier A: Structured

Derived directly from structured edit payloads or structured patches.

Examples:

- Claude `Edit` with `structuredPatch`
- Codex `apply_patch` unified diff

These are the strongest metrics and should be the backbone of v1.

### Tier B: Content-Diff

Derived from whole-file `Write` events by diffing prior and new content.

These are still useful but noisier, especially when full-file rewrites replace mostly identical content.

### Tier C: Inferred

Derived from secondary metadata, snapshots, or external state such as current repo contents.

These are appropriate for phase 2 or explicitly optional metrics.

## Source Capability Matrix

### Claude Code

Available in local JSONL:

- assistant tool calls with model identity
- `Edit` operations with `old_string`, `new_string`
- `toolUseResult.structuredPatch` for at least some edit flows
- `Write` operations with full output content
- `file-history-snapshot` metadata
- per-message `cwd`

Implications:

- Claude supports strong line-based change metrics.
- Claude supports rewrite-heavy metrics better than Codex because full-file `Write` is explicit.
- Claude may support future "surviving changes" metrics if snapshot backup resolution is implemented.

### Codex

Available in local JSONL:

- `turn_context` with model and cwd
- `apply_patch` custom tool calls carrying unified diffs
- token usage and model context already parsed today

Implications:

- Codex supports strong patch-based change metrics.
- Codex does not provide the same full-file rewrite surface as Claude `Write`.
- Shell-driven file edits remain out of scope for exact attribution in v1.

## Proposed Metrics

This section defines the candidate metrics and whether they should ship in v1.

### Core Additive Metrics

These should ship in v1 and be available for `claude`, `codex`, and `all`.

#### `added_lines`

Definition:

- Count of lines added by model-attributed edit operations in the selected period.

Computation:

- Claude `Edit`: count `+` patch lines excluding patch metadata
- Claude `Write`: diff old and new content when old content is available, otherwise treat as a full-file write event with provider-specific fallback
- Codex `apply_patch`: count `+` diff lines excluding patch metadata

#### `removed_lines`

Definition:

- Count of lines removed by model-attributed edit operations in the selected period.

Computation:

- Claude `Edit`: count `-` patch lines
- Claude `Write`: diff old and new content when prior content is available
- Codex `apply_patch`: count `-` diff lines

#### `net_lines`

Definition:

- `added_lines - removed_lines`

Notes:

- Can be negative.
- This is the best headline change metric for compact UI.

#### `files_touched`

Definition:

- Count of distinct repository-relative file paths touched by model-attributed changes in the selected period.

Rules:

- Count distinct normalized paths
- Exclude provider-internal paths and temp paths by default
- Count a file once per aggregated payload, not once per event

#### `change_events`

Definition:

- Count of model-attributed edit events.

Interpretation:

- Helps distinguish many small edits from a few large ones.

#### `write_events`

Definition:

- Count of full-file write events.

Notes:

- Claude supports this directly.
- Codex does not have an equivalent structured write event in the same way; use `0` for Codex v1.

### Composition Metrics

These should ship in v1 if path classification is implemented.

#### `code_lines_changed`

Definition:

- `added_lines + removed_lines` limited to files classified as code.

#### `docs_lines_changed`

Definition:

- `added_lines + removed_lines` limited to documentation files.

#### `config_lines_changed`

Definition:

- `added_lines + removed_lines` limited to config or structured text files.

#### `other_lines_changed`

Definition:

- Remaining changed lines not classified as code, docs, or config.

#### `code_share`

Definition:

- `code_lines_changed / total_changed_lines`

Notes:

- This is more useful than a single binary "code vs non-code" flag.

### Efficiency Metrics

These should ship in v1, but only when denominators are meaningful.

#### `cost_per_100_net_lines`

Definition:

- `(total_cost / net_lines) * 100`

Availability:

- Only when `net_lines > 0`

Notes:

- This should be hidden or `null` when `net_lines <= 0`
- Do not fabricate a value for zero or negative net change

#### `tokens_per_net_line`

Definition:

- `total_tokens / net_lines`

Availability:

- Only when `net_lines > 0`

#### `avg_lines_per_event`

Definition:

- `(added_lines + removed_lines) / change_events`

Interpretation:

- Large values imply fewer, larger edits
- Small values imply iterative patching

### Behavioral Metrics

These are useful, but only some should ship in v1.

#### `rewrite_ratio`

Definition:

- `rewrite_changed_lines / total_changed_lines`

Where:

- `rewrite_changed_lines` comes from full-file write events or edit flows classified as rewrites

Availability:

- Claude: supported
- Codex: unsupported in v1
- `all`: `null` if the selected provider set does not fully support the metric

#### `churn_ratio`

Definition:

- `removed_lines / max(added_lines, 1)`

Interpretation:

- High churn indicates the model repeatedly rewrote or backed out previous work

Notes:

- This is crude but useful
- It should be presented as an advanced secondary metric, not a top-line headline

#### `dominant_extension`

Definition:

- File extension contributing the largest changed-line share

Availability:

- Phase 1 or 2 depending on how much UI room exists

### Advanced Metrics

These should not block v1.

#### `surviving_added_lines`

Definition:

- Lines added by model-attributed edits that still survive in the final file state.

Why It Is Hard:

- Requires a stable before/after reference, not just emitted edits
- Claude snapshot metadata needs verified backup resolution
- Codex would likely need a repo diff or explicit file snapshots

Recommendation:

- Defer to phase 2

#### `surviving_net_lines`

Definition:

- Net change after reconciling rewrites and later removals against final file state.

Recommendation:

- Defer to phase 2

## File Classification

V1 should classify file paths by extension into four buckets.

### Code

Examples:

- `rs`
- `ts`, `tsx`, `js`, `jsx`, `mjs`, `cjs`
- `py`
- `go`
- `java`, `kt`, `scala`
- `swift`
- `c`, `cc`, `cpp`, `h`, `hpp`
- `cs`
- `rb`, `php`
- `sh`, `bash`, `zsh`
- `sql`
- `html`, `css`, `scss`, `sass`
- `svelte`, `vue`

### Docs

Examples:

- `md`, `mdx`
- `txt`
- `rst`
- `adoc`, `asciidoc`

### Config

Examples:

- `json`
- `yaml`, `yml`
- `toml`
- `ini`
- `env`
- `xml`

### Other

- everything else

The classification table should be centralized in Rust and mirrored in TypeScript only if the frontend needs labels.

## Default Exclusion Rules

V1 should exclude provider-internal and obviously non-project files from change metrics.

Examples:

- `~/.claude/plans/**`
- provider tool-result caches
- temp directories
- editor transient files

General rule:

- Include only files under the session working directory when that directory is known
- Exclude provider-owned internal paths outside that workspace

This matters because Claude logs can include plan files and other provider artifacts that are not user project work.

## Provider Parsing Design

### Claude Code Parsing

#### Events to Parse

- `assistant` messages containing `tool_use`
- paired `user` messages containing `tool_result` or `toolUseResult`
- `file-history-snapshot` metadata for future advanced metrics

#### Structured Edit Flow

For `Edit`:

- attribute the event to the assistant message model
- resolve path from `file_path`
- prefer `toolUseResult.structuredPatch`
- fallback to diffing `oldString` and `newString`

#### Full-File Write Flow

For `Write`:

- attribute the event to the assistant message model
- resolve path from `file_path`
- treat the event as a rewrite-capable event
- derive line deltas by diffing prior and new content when available

#### Claude Confidence Rules

- `Edit` with structured patch: Tier A
- `Edit` with `oldString` and `newString` only: Tier B
- `Write` with no recoverable prior content: record event and path, but do not inflate additive line metrics with naive full-file length unless the fallback behavior is explicitly chosen

Recommendation:

- In v1, only count Claude `Write` line deltas when a prior version is available in the same log event or paired result data.
- If prior content is not available, still increment `write_events` and `files_touched`, but leave line deltas unchanged.

This is intentionally conservative.

### Codex Parsing

#### Events to Parse

- `turn_context` for cwd and model
- `custom_tool_call` or equivalent `apply_patch` events with unified diffs

#### Codex Diff Flow

For `apply_patch`:

- attribute the patch to the current resolved model
- resolve paths relative to the current session cwd
- parse unified diff hunks
- count added and removed lines
- record touched files

#### Unsupported Codex Surfaces in v1

- shell-driven rewrites through `exec_command`
- editor operations not represented as structured diffs

These may be explored later, but should not be mixed into v1 with low confidence.

## Data Model Changes

The existing `UsagePayload` should gain a nested change stats object rather than many new top-level fields.

### Proposed Rust Types

```rust
pub struct ChangeStats {
    pub added_lines: u64,
    pub removed_lines: u64,
    pub net_lines: i64,
    pub files_touched: u32,
    pub change_events: u32,
    pub write_events: u32,
    pub code_lines_changed: u64,
    pub docs_lines_changed: u64,
    pub config_lines_changed: u64,
    pub other_lines_changed: u64,
    pub avg_lines_per_event: Option<f64>,
    pub cost_per_100_net_lines: Option<f64>,
    pub tokens_per_net_line: Option<f64>,
    pub rewrite_ratio: Option<f64>,
    pub churn_ratio: Option<f64>,
    pub dominant_extension: Option<String>,
}

pub struct ModelChangeSummary {
    pub added_lines: u64,
    pub removed_lines: u64,
    pub net_lines: i64,
    pub files_touched: u32,
    pub change_events: u32,
}
```

### Usage Payload Integration

```rust
pub struct UsagePayload {
    // existing fields...
    pub change_stats: Option<ChangeStats>,
}
```

### Model Summary Integration

```rust
pub struct ModelSummary {
    pub display_name: String,
    pub model_key: String,
    pub cost: f64,
    pub tokens: u64,
    pub change_stats: Option<ModelChangeSummary>,
}
```

This is preferable to adding raw LOC numbers directly to `ChartBucket` and `ChartSegment` in v1. A chart toggle can be phase 2 once the UI design is finalized.

## Internal Parser Model

Today the file cache stores only usage entries. That should be expanded.

### Current Shape

- cached file stamp
- parsed usage entries
- earliest date

### Proposed Shape

Introduce a cached file record that includes both usage entries and change events.

Conceptually:

```rust
struct ParsedFileData {
    usage_entries: Vec<ParsedEntry>,
    change_events: Vec<ParsedChangeEvent>,
    earliest_usage_date: Option<NaiveDate>,
    earliest_change_date: Option<NaiveDate>,
}
```

This keeps file parsing single-pass and lets usage and change aggregation share the same file cache.

### Parsed Change Event

Conceptually:

```rust
struct ParsedChangeEvent {
    timestamp: DateTime<Local>,
    model: String,
    provider: String,
    path: String,
    normalized_path: String,
    kind: ChangeEventKind,
    added_lines: u64,
    removed_lines: u64,
    category: FileCategory,
    unique_hash: Option<String>,
}
```

Where `ChangeEventKind` is something like:

- `PatchEdit`
- `FullWrite`

## Aggregation Semantics

Change stats should follow the same period slicing semantics as existing usage payloads.

### `5h`

- Aggregate change events inside the same block-based logic used by `get_blocks`
- This enables active-session change volume

### `day`

- Group by hour for charts if chart support is added later
- Top-level `change_stats` should cover the full selected day

### `week`

- Aggregate all change events within the selected Monday-Sunday range

### `month`

- Aggregate all change events within the selected calendar month

### `year`

- Aggregate all change events within the selected calendar year

## Merge Rules for `provider = all`

Additive metrics should sum across providers:

- `added_lines`
- `removed_lines`
- `net_lines`
- `files_touched`
- `change_events`
- classified line counts

Ratio metrics should be recomputed from merged numerators and denominators, not averaged:

- `avg_lines_per_event`
- `cost_per_100_net_lines`
- `tokens_per_net_line`
- `churn_ratio`

Provider-specific metrics with incomplete support should become `null` in merged mode if that would mislead:

- `rewrite_ratio` should be `null` in `all` mode in v1 because Codex does not support the same rewrite surface as Claude

## UI Integration

### Phase 1

Update [MetricsRow.svelte](../src/lib/components/MetricsRow.svelte) to show:

- Cost
- Change Volume
- Efficiency

Example:

- Change Volume headline: `+842 / -311`
- Change Volume label: `Net 531 lines`
- Change Volume sublabel: `37 files · 52 edits`

Example efficiency card:

- `$0.44 / 100 LOC`
- `1.9K tok / LOC`

### Phase 1.5

Update [ModelList.svelte](../src/lib/components/ModelList.svelte) to optionally show:

- model cost
- model tokens
- model net lines or total changed lines

The row should not try to show all values simultaneously if it becomes visually dense.

### Phase 2

Update [Chart.svelte](../src/lib/components/Chart.svelte) to support a metric toggle:

- `Cost`
- `Tokens`
- `LOC`

This requires extending bucket or segment payloads with change totals.

## Backend Touchpoints

Primary files:

- `src-tauri/src/parser.rs`
- `src-tauri/src/models.rs`
- `src-tauri/src/commands.rs`

Frontend touchpoints:

- `src/lib/types/index.ts`
- `src/lib/components/MetricsRow.svelte`
- `src/lib/components/ModelList.svelte`
- `src/lib/components/Chart.svelte`

## Performance Considerations

- File parsing should remain single-pass per file.
- Parsed-file caching should include change events as well as usage entries.
- Diff parsing for structured patches is cheap relative to full JSONL scanning.
- Conservative handling of unsupported `Write` or shell cases avoids expensive repo introspection in v1.

## Testing Plan

### Rust Unit Tests

Add parser fixtures for:

- Claude `Edit` with `structuredPatch`
- Claude `Edit` fallback with `oldString` and `newString`
- Claude `Write` with resolvable prior content
- Codex `apply_patch` unified diff
- workspace exclusion rules
- file category classification
- merge behavior for `provider = all`
- null behavior for unsupported ratio metrics

### Command and Aggregation Tests

Add tests for:

- daily, weekly, monthly change aggregation
- period filters preserving additive change metrics
- merged provider recomputation of ratios

### Frontend Tests

Add tests for:

- metrics row formatting with and without change stats
- hidden or unavailable ratio metrics
- model list fallback behavior when change stats are absent

## Risks

### Overclaiming Precision

Risk:

- Users may interpret change stats as exact authorship.

Mitigation:

- Conservative metric naming
- conservative support matrix
- avoid shell attribution in v1

### Provider Noise

Risk:

- Claude plan files or internal provider files inflate totals.

Mitigation:

- workspace-relative inclusion rules
- default exclusion rules

### Rewrite Inflation

Risk:

- Full-file writes inflate line deltas even when semantic changes are small.

Mitigation:

- separate `write_events`
- expose `rewrite_ratio`
- only count line deltas when prior content is recoverable

## Rollout Plan

### Phase 1

- Add parser support for structured Claude `Edit` and Codex `apply_patch`
- Add `change_stats` to `UsagePayload`
- Add top-row change and efficiency cards
- Add model-level change stats

### Phase 2

- Add Claude `Write` diff support where prior content is recoverable
- Add chart metric toggle
- Add richer composition metrics in UI

### Phase 3

- Evaluate surviving-change metrics using Claude snapshot backups or optional repo diffing

## Recommendation

Ship v1 with the smallest honest set:

- `added_lines`
- `removed_lines`
- `net_lines`
- `files_touched`
- `change_events`
- `code_lines_changed`
- `docs_lines_changed`
- `config_lines_changed`
- `cost_per_100_net_lines`
- `tokens_per_net_line`

Do not block on:

- shell attribution
- surviving LOC
- chart-level LOC mode
- full-file rewrite support when prior content is not recoverable

This yields a strong first version that is useful, conservative, and clearly supportable for both Claude Code and Codex.
