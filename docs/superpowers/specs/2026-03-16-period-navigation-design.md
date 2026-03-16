# Period Navigation — Design Spec

Navigate to past days, weeks, months, and years with back/forward arrows.

## Decisions

- **Supported periods:** Day, Week, Month, Year. The 5H view stays anchored to "now" (live burn-rate monitor).
- **Empty periods within data range:** Show chart area with "No usage data for this period" message; arrows remain active.
- **Data boundary:** Back arrow disables at the earliest period that contains data. Determined by a lightweight `has_entries_before()` check in the parser.
- **Layout:** Dedicated date navigation row between TimeTabs and MetricsRow: `‹  March 15, 2026  ›`
- **Date label click:** Resets to current period (offset 0). Only active when offset !== 0.
- **Approach:** Offset-based with backend metadata. Frontend sends `offset: i32` (0 = current, -1 = previous, …). Backend returns `period_label` and `has_earlier_data` alongside the existing payload.

## Backend Changes

### models.rs — UsagePayload

Add two fields:

```rust
pub struct UsagePayload {
    // ... existing fields ...
    pub period_label: String,
    pub has_earlier_data: bool,
}
```

`period_label` is a human-readable string describing the period being viewed. Format by period:

| Period | Example (offset 0) | Example (offset -1) |
|--------|--------------------|--------------------|
| day    | "March 16, 2026"   | "March 15, 2026"   |
| week   | "Mar 10 – 16, 2026"| "Mar 3 – 9, 2026"  |
| month  | "March 2026"       | "February 2026"    |
| year   | "2026"             | "2025"             |

**Week cross-boundary labels:** When a week spans two months, include both: "Mar 30 – Apr 5, 2026". When it spans two years: "Dec 29, 2025 – Jan 4, 2026".

`has_earlier_data` is `true` when there are log entries in the period *before* the one currently being viewed. Determined cheaply by the parser.

### commands.rs — get_usage_data

Updated signature:

```rust
#[tauri::command]
pub async fn get_usage_data(
    provider: String,
    period: String,
    offset: i32,        // new: 0 = current, -1 = previous, etc.
    state: State<'_, AppState>,
) -> Result<UsagePayload, String>
```

`get_provider_data` gains an `offset` parameter. Date computation logic per period:

- **day:** `Local::now().date_naive() + Duration::days(offset)` → pass to `get_hourly()`
- **week:** Compute Monday of current week, then add `offset * 7` days → pass to `get_daily()` with a 7-day window
- **month:** Compute 1st of current month, then shift by `offset` months → pass to `get_daily()` with month-bounded window
- **year:** Current year + offset → pass to `get_monthly()` with year-bounded window
- **5h:** Offset is ignored; always uses today.

For "all" provider: pass offset through to both `get_provider_data("claude", ...)` and `get_provider_data("codex", ...)`. `merge_payloads` copies `period_label` from the first (claude) payload and ANDs `has_earlier_data`.

### parser.rs

**get_hourly() past-day fix:** Currently iterates `min_hour..=current_hour`. For past days (when the `since` date is before today), iterate `0..=23` instead, since the full day has elapsed.

**New method — has_entries_before():**

```rust
pub fn has_entries_before(&self, provider: &str, before_date: NaiveDate) -> bool
```

Checks whether any log entries exist with a timestamp before `before_date`.

- **Claude:** Call `read_claude_entries` with `since: None` (no date filter — this disables the `modified_since` file-skip optimisation, since we need to read *old* files). Iterate entries and return `true` on the first entry with `timestamp.date_naive() < before_date`. This scans all files but bails early on the first match.
- **Codex:** Iterate `YYYY/MM/DD` date directories under `~/.codex/sessions/`. For each directory whose date is before `before_date`, check if any `.jsonl` files exist. Return `true` on first match. This is a cheap filesystem check with no file parsing.

Performance note: for users with many years of Claude log files, the Claude path may be slow on the first call because it globs all `.jsonl` files. Future optimisation (out of scope): cache the earliest-known date per provider.

### Cache key update

No change needed to the backend cache key format. The existing `since`-based keys (e.g., `"hourly:claude:20260315"`) already differentiate offsets naturally because each offset produces a different `since` date string.

## Frontend Changes

### types/index.ts — UsagePayload

Add matching fields:

```typescript
export interface UsagePayload {
  // ... existing fields ...
  period_label: string;
  has_earlier_data: boolean;
}
```

### stores/usage.ts

**New store:**

```typescript
export const activeOffset = writable<number>(0);
```

**fetchData signature:**

```typescript
export async function fetchData(provider: string, period: string, offset: number = 0)
```

Passes `offset` to the IPC invoke:

```typescript
invoke<UsagePayload>("get_usage_data", { provider, period, offset })
```

**Cache key:**

```typescript
function cacheKey(provider: string, period: string, offset: number) {
  return `${provider}:${period}:${offset}`;
}
```

**Adjacent-period warming:** After fetching offset N, silently warm N-1 and N+1 (if N+1 <= 0) for instant navigation:

```typescript
warmCache(provider, period, offset - 1);
if (offset < 0) warmCache(provider, period, offset + 1);
```

**warmAllPeriods:** Always warms at offset `0` for all periods, since switching periods resets offset to 0. Signature unchanged — it does not need to accept an offset parameter.

### New component — DateNav.svelte

Sits between TimeTabs and MetricsRow. Hidden when `period === "5h"`.

**Props:**

```typescript
interface Props {
  periodLabel: string;
  hasEarlierData: boolean;
  isAtPresent: boolean;     // offset === 0
  onBack: () => void;
  onForward: () => void;
  onReset: () => void;      // click date label → jump to current
}
```

**Rendering:**

```
‹   March 15, 2026   ›
```

- Back arrow (‹): disabled when `!hasEarlierData` → `color: var(--t4)`, `pointer-events: none`
- Forward arrow (›): disabled when `isAtPresent` → same disabled style
- Date label: centered, `font: 500 10px/1 'Inter'`, `color: var(--t1)`. When `!isAtPresent`, `cursor: pointer` and on hover `color: var(--accent)`.
- Arrow buttons: `background: none`, `border: none`, `font-size: 14px`, `color: var(--t2)`, hover → `color: var(--t1)`.
- Row: `display: flex; align-items: center; justify-content: center; gap: 12px; padding: 4px 12px 0;`
- Entrance animation: `fadeUp` matching TimeTabs.

### App.svelte

**New state:**

```typescript
let offset = $state(0);
```

**New handlers:**

```typescript
async function handleOffsetChange(delta: number) {
  offset += delta;
  activeOffset.set(offset);
  await fetchData(provider, period, offset);
  // Guard for stale fetch
  dataKey = `${provider}-${period}-${offset}-${Date.now()}`;
  await tick();
  syncSize();
}

async function handleOffsetReset() {
  if (offset === 0) return;
  offset = 0;
  activeOffset.set(0);
  await fetchData(provider, period, 0);
  dataKey = `${provider}-${period}-0-${Date.now()}`;
  await tick();
  syncSize();
}
```

**handlePeriodChange — reset offset:**

```typescript
async function handlePeriodChange(p) {
  offset = 0;
  activeOffset.set(0);
  // ... existing logic with offset passed ...
}
```

**handleProviderChange — preserve offset:**

```typescript
async function handleProviderChange(p) {
  // ... existing logic, but pass current offset ...
  await fetchData(p, period, offset);
}
```

**data-updated listener — preserve offset:**

The existing `listen("data-updated", ...)` callback must pass the current offset so background refreshes don't overwrite a past-period view with current data:

```typescript
unlisten = await listen("data-updated", () => {
  dataKey = `${provider}-${period}-${offset}-${Date.now()}`;
  fetchData(provider, period, offset);
});
```

**Template — insert DateNav:**

```svelte
<TimeTabs active={period} onChange={handlePeriodChange} />
{#if period !== "5h" && data}
  <DateNav
    periodLabel={data.period_label}
    hasEarlierData={data.has_earlier_data}
    isAtPresent={offset === 0}
    onBack={() => handleOffsetChange(-1)}
    onForward={() => handleOffsetChange(1)}
    onReset={handleOffsetReset}
  />
{/if}
```

**Empty state — no-data message:**

When `period !== "5h"` and `data.total_cost === 0` and `data.total_tokens === 0`:

```svelte
<div class="empty-period">No usage data for this period</div>
```

Styled as centered, `color: var(--t3)`, `font: 400 10px/1 'Inter'`, `padding: 32px 0`.

Note: We check `total_cost === 0` rather than `chart_buckets.length === 0` because `get_hourly` always produces 24 buckets (one per hour) for past days, even when all are empty. Week/month/year only emit buckets for dates with data, but using `total_cost` is a consistent check across all periods. The empty-period message replaces the Chart component (no empty bars shown).

## Files Changed

| File | Type | Change |
|------|------|--------|
| `src-tauri/src/models.rs` | Edit | Add `period_label`, `has_earlier_data` to `UsagePayload` |
| `src-tauri/src/commands.rs` | Edit | Add `offset` param, offset-to-date logic, label formatting, `has_earlier_data` wiring |
| `src-tauri/src/parser.rs` | Edit | Fix `get_hourly()` for past days; add `has_entries_before()` |
| `src/lib/types/index.ts` | Edit | Add `period_label`, `has_earlier_data` to TS interface |
| `src/lib/stores/usage.ts` | Edit | Add `activeOffset` store; update `fetchData`/`cacheKey`/`warmCache` for offset |
| `src/lib/components/DateNav.svelte` | **New** | Date navigation row component |
| `src/App.svelte` | Edit | Wire offset state, handlers, DateNav, empty state |

## Testing

- **Backend unit tests:** Offset date arithmetic for each period; `has_earlier_data` true/false; `period_label` format correctness; `get_hourly()` returns 24 buckets for past days.
- **Existing tests:** Update to supply `offset: 0` where needed; add `period_label`/`has_earlier_data` to test payloads.
