# Menu Bar Status — Customizable Tray Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the simple icon + cost tray item with a customizable widget showing rate limit bars, percentages, and cost — each independently configurable via a settings panel with live preview.

**Architecture:** Extend the Settings type with a `TrayConfig` object. The Rust backend composes a tray title string from config + data. The frontend gets a new `MenuBarPreview` component and a "Menu Bar" settings section. Dynamic bar rendering in the tray icon is deferred to a follow-up (this plan uses tray title text only — bars in the icon require native image rendering).

**Tech Stack:** Svelte 5, TypeScript, Rust/Tauri, Vitest

**Spec:** `docs/superpowers/specs/2026-03-18-menu-bar-status-design.md`

---

### Task 1: Add TrayConfig type and settings schema

**Files:**
- Modify: `src/lib/types/index.ts`
- Modify: `src/lib/stores/settings.ts`
- Test: `src/lib/stores/settings.test.ts`

- [ ] **Step 1: Add TrayConfig type to types/index.ts**

Add after the existing `RateLimitsPayload` interface (~line 98):

```typescript
export type BarDisplay = 'off' | 'single' | 'both';
export type PercentageFormat = 'compact' | 'verbose';
export type CostPrecision = 'whole' | 'full';

export interface TrayConfig {
  barDisplay: BarDisplay;
  barProvider: 'claude' | 'codex';
  showPercentages: boolean;
  percentageFormat: PercentageFormat;
  showCost: boolean;
  costPrecision: CostPrecision;
}
```

- [ ] **Step 2: Update Settings interface in stores/settings.ts**

Replace `showTrayAmount: boolean;` with `trayConfig: TrayConfig;` in the `Settings` interface (line 16).

Update `DEFAULTS` (line 21) — replace `showTrayAmount: true,` with:

```typescript
trayConfig: {
  barDisplay: 'both',
  barProvider: 'claude',
  showPercentages: false,
  percentageFormat: 'compact',
  showCost: true,
  costPrecision: 'full',
},
```

- [ ] **Step 3: Add migration logic in loadSettings**

In `loadSettings()`, after `const merged = { ...DEFAULTS, ...saved };` (line 46), add migration:

```typescript
// Migrate legacy showTrayAmount → trayConfig
if (saved && 'showTrayAmount' in saved && !('trayConfig' in saved)) {
  const legacy = saved as Record<string, unknown>;
  merged.trayConfig = {
    ...DEFAULTS.trayConfig,
    showCost: legacy.showTrayAmount !== false,
  };
}
delete (merged as Record<string, unknown>).showTrayAmount;
```

- [ ] **Step 4: Update settings.test.ts**

Replace all `showTrayAmount: true` / `showTrayAmount: false` references with the new `trayConfig` object. Update the test that checks `set_show_tray_amount` IPC to check `set_tray_config` instead.

- [ ] **Step 5: Run tests**

Run: `npx vitest run src/lib/stores/settings.test.ts`
Expected: All tests pass with updated schema.

- [ ] **Step 6: Commit**

```bash
git add src/lib/types/index.ts src/lib/stores/settings.ts src/lib/stores/settings.test.ts
git commit -m "feat: add TrayConfig type and migrate showTrayAmount"
```

---

### Task 2: Add tray title formatting logic (frontend)

**Files:**
- Create: `src/lib/trayTitle.ts`
- Create: `src/lib/trayTitle.test.ts`

- [ ] **Step 1: Write failing tests for tray title formatting**

Create `src/lib/trayTitle.test.ts`:

```typescript
import { describe, it, expect } from "vitest";
import { formatTrayTitle } from "./trayTitle.js";
import type { TrayConfig, RateLimitsPayload } from "./types/index.js";

const DEFAULT_CONFIG: TrayConfig = {
  barDisplay: 'both',
  barProvider: 'claude',
  showPercentages: true,
  percentageFormat: 'compact',
  showCost: true,
  costPrecision: 'full',
};

const RATE_LIMITS: RateLimitsPayload = {
  claude: {
    provider: 'claude',
    planTier: 'Max 5x',
    windows: [{ windowId: 'w1', label: 'Primary', utilization: 0.72, resetsAt: null }],
    extraUsage: null,
    stale: false,
    error: null,
    cooldownUntil: null,
    fetchedAt: '2026-03-18T00:00:00Z',
  },
  codex: {
    provider: 'codex',
    planTier: 'Pro',
    windows: [{ windowId: 'w2', label: 'Primary', utilization: 0.35, resetsAt: null }],
    extraUsage: null,
    stale: false,
    error: null,
    cooldownUntil: null,
    fetchedAt: '2026-03-18T00:00:00Z',
  },
};

describe("formatTrayTitle", () => {
  it("returns compact percentages + full cost", () => {
    expect(formatTrayTitle(DEFAULT_CONFIG, RATE_LIMITS, 12.456)).toBe("72 · 35  $12.46");
  });

  it("returns compact percentages + whole cost", () => {
    const config = { ...DEFAULT_CONFIG, costPrecision: 'whole' as const };
    expect(formatTrayTitle(config, RATE_LIMITS, 12.456)).toBe("72 · 35  $12");
  });

  it("returns only cost when percentages off", () => {
    const config = { ...DEFAULT_CONFIG, showPercentages: false };
    expect(formatTrayTitle(config, RATE_LIMITS, 12.456)).toBe("$12.46");
  });

  it("returns only percentages when cost off", () => {
    const config = { ...DEFAULT_CONFIG, showCost: false };
    expect(formatTrayTitle(config, RATE_LIMITS, 12.456)).toBe("72 · 35");
  });

  it("returns empty string when both off", () => {
    const config = { ...DEFAULT_CONFIG, showPercentages: false, showCost: false };
    expect(formatTrayTitle(config, RATE_LIMITS, 12.456)).toBe("");
  });

  it("shows single provider percentage", () => {
    const config = { ...DEFAULT_CONFIG, barDisplay: 'single' as const, barProvider: 'claude' as const, percentageFormat: 'compact' as const };
    expect(formatTrayTitle(config, RATE_LIMITS, 12.456)).toBe("72  $12.46");
  });

  it("shows verbose format", () => {
    const config = { ...DEFAULT_CONFIG, percentageFormat: 'verbose' as const };
    expect(formatTrayTitle(config, RATE_LIMITS, 12.456)).toBe("Claude Code 72%  Codex 35%  $12.46");
  });

  it("handles null rate limits gracefully", () => {
    const config = { ...DEFAULT_CONFIG };
    expect(formatTrayTitle(config, null, 5.0)).toBe("$5.00");
  });

  it("returns empty string when bars off, percentages off, cost off", () => {
    const config = { ...DEFAULT_CONFIG, barDisplay: 'off' as const, showPercentages: false, showCost: false };
    expect(formatTrayTitle(config, null, 0)).toBe("");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/trayTitle.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement formatTrayTitle**

Create `src/lib/trayTitle.ts`:

```typescript
import type { TrayConfig, RateLimitsPayload } from "./types/index.js";

function primaryUtilization(
  rateLimits: RateLimitsPayload | null,
  provider: 'claude' | 'codex',
): number | null {
  const data = rateLimits?.[provider];
  if (!data || data.windows.length === 0) return null;
  return Math.round(data.windows[0].utilization * 100);
}

export function formatTrayTitle(
  config: TrayConfig,
  rateLimits: RateLimitsPayload | null,
  totalCost: number,
): string {
  const parts: string[] = [];

  // Percentages
  if (config.showPercentages) {
    const claudePct = primaryUtilization(rateLimits, 'claude');
    const codexPct = primaryUtilization(rateLimits, 'codex');

    if (config.barDisplay === 'both') {
      if (claudePct !== null && codexPct !== null) {
        if (config.percentageFormat === 'compact') {
          parts.push(`${claudePct} · ${codexPct}`);
        } else {
          parts.push(`Claude Code ${claudePct}%  Codex ${codexPct}%`);
        }
      }
    } else if (config.barDisplay === 'single') {
      const pct = primaryUtilization(rateLimits, config.barProvider);
      if (pct !== null) {
        if (config.percentageFormat === 'compact') {
          parts.push(`${pct}`);
        } else {
          const name = config.barProvider === 'claude' ? 'Claude Code' : 'Codex';
          parts.push(`${name} ${pct}%`);
        }
      }
    }
  }

  // Cost
  if (config.showCost) {
    if (config.costPrecision === 'whole') {
      parts.push(`$${Math.round(totalCost)}`);
    } else {
      parts.push(`$${totalCost.toFixed(2)}`);
    }
  }

  return parts.join("  ");
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/trayTitle.test.ts`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/trayTitle.ts src/lib/trayTitle.test.ts
git commit -m "feat: add formatTrayTitle with configurable output"
```

---

### Task 3: Update Rust backend for TrayConfig

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Replace `show_tray_amount` with `tray_config` in AppState**

In `commands.rs`, replace the `show_tray_amount` field (line 22) with:

```rust
pub tray_config: Arc<RwLock<TrayConfig>>,
```

Add the TrayConfig struct before AppState:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayConfig {
    pub bar_display: String,       // "off" | "single" | "both"
    pub bar_provider: String,      // "claude" | "codex"
    pub show_percentages: bool,
    pub percentage_format: String, // "compact" | "verbose"
    pub show_cost: bool,
    pub cost_precision: String,    // "whole" | "full"
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            bar_display: "both".to_string(),
            bar_provider: "claude".to_string(),
            show_percentages: false,
            percentage_format: "compact".to_string(),
            show_cost: true,
            cost_precision: "full".to_string(),
        }
    }
}
```

Update `AppState::new()` (line 28):

```rust
tray_config: Arc::new(RwLock::new(TrayConfig::default())),
```

- [ ] **Step 2: Update format_tray_title to use TrayConfig**

Replace `format_tray_title` (line 130) and `sync_tray_title` (line 138):

```rust
fn format_tray_title(config: &TrayConfig, total_cost: f64) -> String {
    let mut parts: Vec<String> = Vec::new();

    if config.show_cost {
        if config.cost_precision == "whole" {
            parts.push(format!("${}", total_cost.round() as i64));
        } else {
            parts.push(format!("${:.2}", total_cost));
        }
    }

    parts.join("  ")
}

pub async fn sync_tray_title(app: &tauri::AppHandle, state: &AppState) {
    let config = state.tray_config.read().await.clone();
    let title = if config.show_cost {
        let today = Local::now().format("%Y%m%d").to_string();
        let claude = state.parser.get_daily("claude", &today);
        let codex = state.parser.get_daily("codex", &today);
        format_tray_title(&config, claude.total_cost + codex.total_cost)
    } else {
        String::new()
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_title(Some(title));
    }
}
```

- [ ] **Step 3: Replace set_show_tray_amount command with set_tray_config**

Replace the `set_show_tray_amount` command (line 212-224):

```rust
#[tauri::command]
pub async fn set_tray_config(
    config: TrayConfig,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut current = state.tray_config.write().await;
    *current = config;
    drop(current);

    sync_tray_title(&app, &state).await;

    Ok(())
}
```

- [ ] **Step 4: Update lib.rs invoke_handler**

In `lib.rs` line 116, replace `commands::set_show_tray_amount` with `commands::set_tray_config`.

- [ ] **Step 5: Update Rust tests**

Update all test instances that create `AppState` with `show_tray_amount: Arc::new(RwLock::new(true))` to use `tray_config: Arc::new(RwLock::new(TrayConfig::default()))`. Update `format_tray_title` tests.

- [ ] **Step 6: Build and verify**

Run: `cd src-tauri && cargo build`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(backend): replace show_tray_amount with TrayConfig"
```

---

### Task 4: Update bootstrap and Settings.svelte for TrayConfig

**Files:**
- Modify: `src/lib/bootstrap.ts`
- Modify: `src/lib/bootstrap.test.ts`
- Modify: `src/lib/components/Settings.svelte`

- [ ] **Step 1: Update bootstrap.ts**

In `bootstrap.ts` (line 33), replace:
```typescript
await invokeFn("set_show_tray_amount", { show: saved.showTrayAmount });
```
with:
```typescript
await invokeFn("set_tray_config", { config: saved.trayConfig });
```

- [ ] **Step 2: Update bootstrap.test.ts**

Replace `showTrayAmount` references with `trayConfig` in test fixtures. Update the IPC assertion to check for `set_tray_config` with the config object.

- [ ] **Step 3: Run bootstrap tests**

Run: `npx vitest run src/lib/bootstrap.test.ts`
Expected: PASS.

- [ ] **Step 4: Update Settings.svelte — replace Menu Bar Cost toggle with Menu Bar section**

In `Settings.svelte`, replace the "Menu Bar Cost" row (lines 249-255) with the full Menu Bar configuration section. This includes:

- A `MenuBarPreview` component at the top of the section (built in Task 5)
- Bars section: Display (Off/Single/Both), Provider (Claude/Codex, dimmed when not Single)
- Percentages section: Show toggle, Format selector (72 · 35 / Claude Code 72% Codex 35%)
- Cost section: Show toggle, Precision ($17 / $17.19)

Update all `handleShowTrayAmount` references to use `updateSetting('trayConfig', ...)` with spread merging.

Replace `current.showTrayAmount` initialization (line 29) with `trayConfig` default.

- [ ] **Step 5: Verify app renders**

Run: `npm run dev` and confirm the Settings panel opens and the new Menu Bar section renders.

- [ ] **Step 6: Commit**

```bash
git add src/lib/bootstrap.ts src/lib/bootstrap.test.ts src/lib/components/Settings.svelte
git commit -m "feat(ui): add Menu Bar config section to Settings"
```

---

### Task 5: Build MenuBarPreview component

**Files:**
- Create: `src/lib/components/MenuBarPreview.svelte`
- Modify: `src/lib/components/Settings.svelte` (import and place)

- [ ] **Step 1: Create MenuBarPreview.svelte**

A zoomed (~1.6×) rendering of a macOS menu bar fragment. Shows the TokenMonitor widget (icon, bars, text) on a macOS-style gradient wallpaper background. Receives `trayConfig`, `rateLimits`, and `totalCost` as props.

Key styling:
- Background: layered radial gradients (deep purple/blue) simulating a macOS wallpaper
- Bar: translucent dark bar (`rgba(28,28,30,0.82)`) with `backdrop-filter: blur(40px)`
- Zoomed 1.6×: text at 15.5px, bars 52×3.5px, icon 18×18
- All text uses uniform styling — same size/weight/color for cost and percentages
- Context icons (battery, Wi-Fi, clock) at reduced opacity to frame the widget

Props:
```typescript
interface Props {
  config: TrayConfig;
  rateLimits: RateLimitsPayload | null;
  totalCost: number;
}
```

The component derives formatted text from `formatTrayTitle()` and renders bars based on `config.barDisplay` + rate limit utilization data.

- [ ] **Step 2: Wire into Settings.svelte**

Import `MenuBarPreview` and place it in the Menu Bar group, above the config cards. Pass the current `trayConfig`, rate limits store data, and 5-hour cost.

- [ ] **Step 3: Verify preview renders**

Run `npm run dev`, open Settings, confirm the preview shows and updates as you toggle options.

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/MenuBarPreview.svelte src/lib/components/Settings.svelte
git commit -m "feat(ui): add live MenuBarPreview to settings"
```

---

### Task 6: Wire tray config changes to backend

**Files:**
- Modify: `src/lib/components/Settings.svelte`

- [ ] **Step 1: Add IPC call on tray config change**

When any tray config option changes, call:

```typescript
invoke("set_tray_config", { config: current.trayConfig }).catch(() => {});
```

Use a single handler function `handleTrayConfigChange` that spreads the update into `current.trayConfig`, calls `updateSetting('trayConfig', newConfig)`, and invokes the IPC.

- [ ] **Step 2: Test end-to-end**

Run `npm run dev`, toggle each option in Settings, verify:
- The preview updates instantly
- The macOS menu bar title text updates
- Cost precision toggles work ($17 vs $17.19)
- Percentages toggle shows/hides percentage text

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/Settings.svelte
git commit -m "feat: wire tray config changes to Rust backend"
```

---

### Task 7: Run full test suite and cleanup

**Files:**
- All modified test files

- [ ] **Step 1: Run all tests**

Run: `npx vitest run`
Expected: All tests pass.

- [ ] **Step 2: Fix any failures**

Address test failures related to the `showTrayAmount` → `trayConfig` migration.

- [ ] **Step 3: Run Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass.

- [ ] **Step 4: Final commit if needed**

```bash
git add -A
git commit -m "test: fix remaining tests for trayConfig migration"
```
