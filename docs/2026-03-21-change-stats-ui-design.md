# Change Statistics UI Design Spec

Status: Approved
Owner: TokenMonitor
Date: 2026-03-21
Depends on: [design-change-stats.md](../../design-change-stats.md) (data model and parser design)

## Problem

TokenMonitor tracks spend, tokens, and sessions — the same metrics every AI usage tracker shows. It does not surface what the AI actually *produced*. Two sessions with identical spend can look very different in practice: one may be a large code refactor across many files, another may be mostly plans and investigation. Without change volume and composition, the app is a billing dashboard, not a productivity tool.

## Product Positioning

Change statistics are TokenMonitor's primary differentiator. The goal is to shift the app's identity from "billing tracker" to "engineering intelligence tool" by answering: **what is the shape of the work?**

The composition breakdown (code vs docs vs config) is the visual signature no other usage tracker provides. It must be a first-class citizen in the UI, not a secondary detail.

## Product Language

Use:
- "changed lines"
- "model-attributed changes"
- "lines added / removed"
- "net lines"

Never use:
- "lines written"
- "true model LOC"
- "pure AI output"

## Design Decisions

### MetricsRow: 2×2 Grid

Replace the current 3-card horizontal row with a 2×2 grid. This elevates changes to co-equal status with cost.

#### Layout

```
┌─────────────────┬─────────────────┐
│     COST        │    CHANGES      │
│    $12.34       │  +842 / −311    │
│  $2.10/h        │net +531 · 37 files│
├─────────────────┼─────────────────┤
│    TOKENS       │  COMPOSITION    │
│    245.3K       │  [████████░░]   │
│ 180K in/65K out │  code 72% ...   │
└─────────────────┴─────────────────┘
```

#### Card Definitions

**Cost (top-left):**
- Value: `formatCost(total_cost)`
- Label: "Cost" (or "Over budget" with alert styling when threshold exceeded)
- Sublabel: burn rate when live (`$X.XX/h`), session count + avg cost otherwise
- Live dot: animated pulse when `active_block.is_active`

**Changes (top-right):**
- Value: `+{added_lines}` (green) `/` `−{removed_lines}` (red)
- Label: "Changes"
- Sublabel: `net +{net_lines} · {files_touched} files` (spell out "files" — do not abbreviate)
- When net is negative: sublabel text turns red (`net −414 · 22 files`)
- Note: `net_lines` is the only signed integer field — it can be negative. All other integer fields (`added_lines`, `removed_lines`, `files_touched`, etc.) are non-negative.
- Empty state: value `—`, sublabel "No structured edits detected"

**Tokens (bottom-left):**
- Value: `formatTokens(total_tokens)`
- Label: "Tokens"
- Sublabel: `{input} in · {output} out`

**Composition (bottom-right):**
- Header row: label "Composition" left-aligned, efficiency metric right-aligned (`$X.XX/100L`)
- Composition bar: 5px tall, rounded, segmented by file category
- Legend: inline items with 4px dots — show categories with >0 share
- Efficiency metric: `cost_per_100_net_lines` — the label and value area always render, but the value shows `—` when `net_lines <= 0` (not hidden, just dashed)
- Empty state: value `—`, sublabel "No file changes to classify"

#### Grid CSS

```css
.met {
  display: flex;
  padding: 12px 12px 10px;
  gap: 4px;
  flex-wrap: wrap;
}
.m {
  flex: 1 1 calc(50% - 2px);
  min-width: calc(50% - 2px);
  /* existing card styling unchanged */
}
```

This is the only structural change to MetricsRow. All existing card internals (`.m-v`, `.m-l`, `.m-s`, alert state, live dot) remain unchanged.

### Composition Bar

A horizontal stacked bar showing the proportion of changed lines by file category.

#### Categories and Colors

| Category | Color (dark) | Color (light) | Token |
|----------|-------------|---------------|-------|
| Code | `#60a5fa` | `#3b82f6` | `--comp-code` |
| Docs | `#a78bfa` | `#8b5cf6` | `--comp-docs` |
| Config | `#fbbf24` | `#d97706` | `--comp-config` |
| Other | `rgba(255,255,255,0.15)` | `rgba(0,0,0,0.10)` | `--comp-other` |

#### Bar Rendering

- Height: 5px, border-radius: 2.5px
- Track background: `var(--surface-2)` (matches existing track style in UsageBars)
- Segments render left-to-right: code → docs → config → other
- Segments with 0% width are omitted
- First segment gets left border-radius, last gets right border-radius
- Animation: grow from left using `scaleX(0) → scaleX(1)` with staggered delay, matching the UsageBars `hBarGrow` pattern

#### Legend

- Inline flex, gap 6px
- Items: 4px rounded dot + category name + percentage
- Font: 7.5px Inter, `var(--t4)` color
- Only show categories that have >0 share
- If only one category exists (e.g., "code 100%"), show just that one

### Edge States

#### Rich Data (week/month view)
All four cards populated. Composition bar shows multiple segments. Full legend. Efficiency metric visible.

#### Live Session (5h view)
Cost card shows burn rate with live dot. Changes card shows session totals. Composition may have fewer categories (often mostly code during active work).

#### Low Data
Small numbers are fine — `+18 / −5` is still meaningful. Composition bar may show a single segment. This is correct behavior, not an error state.

#### Negative Net Lines
- Changes sublabel: `net −414` in red (`color: #f87171`)
- Composition efficiency: shows `—` (cost-per-100-LOC is undefined for negative net)
- Composition bar still renders normally (it shows total changed lines, not net)

#### No Change Data
- Changes card: value `—` in `var(--t4)`, sublabel "No structured edits detected" in dimmed text
- Composition card: value `—`, sublabel "No file changes to classify"
- Both cards use `.m-quiet` modifier to dim the value color

#### Zero Usage
If no usage data exists at all, the entire MetricsRow shows zeros as it does today. The 2×2 layout handles this naturally.

### ModelList Integration

The model list stays clean by default — cost and tokens only. No change stats in the default view.

#### Optional Toggle

Add a setting `showModelChangeStats` (default: `false`) in the Settings panel under the same section group where `hiddenModels` lives. When enabled, each model row gains:
- Net lines displayed as `+{added} / −{removed}` in 9px font between the model name and cost
- Uses the same green/red coloring as the Changes card

This follows the existing pattern of `hiddenModels` — user-configurable display preferences stored in Settings.

### Chart

No changes in v1. The chart remains cost-only. LOC chart toggle is deferred to phase 2.

## Data Requirements

The UI consumes the `ChangeStats` and `ModelChangeSummary` types defined in the data model design doc.

### UsagePayload additions

```typescript
interface UsagePayload {
  // ... existing fields ...
  change_stats: ChangeStats | null;
}

interface ChangeStats {
  added_lines: number;
  removed_lines: number;
  net_lines: number;
  files_touched: number;
  change_events: number;
  write_events: number;
  code_lines_changed: number;
  docs_lines_changed: number;
  config_lines_changed: number;
  other_lines_changed: number;
  avg_lines_per_event: number | null;
  cost_per_100_net_lines: number | null;
  tokens_per_net_line: number | null;
  rewrite_ratio: number | null;
  churn_ratio: number | null;
  dominant_extension: string | null;
}

interface ModelSummary {
  // ... existing fields ...
  change_stats: ModelChangeSummary | null;
}

interface ModelChangeSummary {
  added_lines: number;
  removed_lines: number;
  net_lines: number;
  files_touched: number;
  change_events: number;
}
```

### Composition Derivation

The composition bar percentages are derived on the frontend from `ChangeStats`:

```
total = code_lines_changed + docs_lines_changed + config_lines_changed + other_lines_changed
code_pct = code_lines_changed / total * 100
docs_pct = docs_lines_changed / total * 100
config_pct = config_lines_changed / total * 100
other_pct = other_lines_changed / total * 100
```

When `total === 0`, the composition card shows the empty state.

**Invariant:** `code_lines_changed + docs_lines_changed + config_lines_changed + other_lines_changed` must equal `added_lines + removed_lines`. The four categories partition all changed lines — every file is classified into exactly one bucket. This should be asserted in Rust unit tests.

**Note:** `files_touched` is deduplicated server-side (one count per distinct normalized path, not per edit event). The frontend uses this value directly.

## Settings Changes

Add to the `Settings` interface:

```typescript
interface Settings {
  // ... existing fields ...
  showModelChangeStats: boolean;  // default: false
}
```

Add a toggle in Settings.svelte under a display preferences group.

## Animation Design

All new elements follow existing animation patterns:

- **2×2 grid cards**: `fadeUp 0.28s ease both 0.07s` (existing MetricsRow animation)
- **Composition bar segments**: `hBarGrow 0.52s cubic-bezier(.22,1,.36,1) both` with staggered `--bar-delay` (matching UsageBars)
- **Composition bar shimmer**: `hBarShimmer 0.5s ease-out both` after grow completes (matching UsageBars)
- **Legend items**: no animation (static after bar renders)

## Accessibility

- Composition bar segments: `role="img"` with `aria-label` describing the breakdown (e.g., "72% code, 14% docs, 8% config, 6% other")
- Changes card values: use `aria-label` to spell out "842 lines added, 311 lines removed, net 531 lines"
- Color is never the sole indicator — percentages are always shown alongside the bar

## Files to Modify

### Frontend
- `src/lib/types/index.ts` — add `ChangeStats`, `ModelChangeSummary`, update `UsagePayload`, `ModelSummary`, `Settings`
- `src/lib/components/MetricsRow.svelte` — 2×2 grid layout, Changes card, Composition card
- `src/lib/components/ModelList.svelte` — optional per-model change stats (behind setting)
- `src/lib/components/Settings.svelte` — add `showModelChangeStats` toggle
- `src/lib/stores/settings.ts` — add default for new setting
- `src/app.css` — composition color tokens for light/dark/glass themes

### Backend
- `src-tauri/src/models.rs` — `ChangeStats`, `ModelChangeSummary` structs
- `src-tauri/src/parser.rs` — parse change events from Claude and Codex logs
- `src-tauri/src/commands.rs` — aggregate and attach `change_stats` to `UsagePayload`

## Testing

### Frontend Tests
- MetricsRow renders 2×2 grid with change stats present
- MetricsRow renders empty state when `change_stats` is null
- Composition bar renders correct segment widths
- Composition bar handles single-category gracefully
- Negative net lines renders red sublabel
- Efficiency metric hidden when `net_lines <= 0`
- ModelList hides change stats by default
- ModelList shows change stats when setting enabled

### Rust Tests
- Covered by the data model design doc's testing plan

## Fields Present but Not Rendered in v1

The following fields are included in the `ChangeStats` TypeScript interface (and computed by the Rust backend) but have no UI surface in v1. They are available for phase 2 features and are intentionally carried in the payload:

- `write_events` — useful for rewrite inflation analysis (phase 2)
- `avg_lines_per_event` — useful for behavioral analysis (phase 2)
- `rewrite_ratio` — Claude-only, useful for rewrite detection (phase 2)
- `churn_ratio` — useful for session quality analysis (phase 2)
- `tokens_per_net_line` — efficiency metric, candidate for phase 2 UI
- `dominant_extension` — candidate for phase 2 composition detail

These fields may be `null` in the payload. The frontend must not assume they are populated.

## Out of Scope (v1)

- Chart LOC toggle (phase 2)
- Surviving LOC metrics (phase 3)
- Shell attribution
- Full-file rewrite when prior content not recoverable
- Semantic language parsing for file classification
