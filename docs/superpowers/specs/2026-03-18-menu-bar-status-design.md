# Menu Bar Status — Customizable Tray Widget

**Date:** 2026-03-18

## Overview

Replace the current simple icon + cost text tray item with a fully customizable menu bar widget. Users configure each element independently via a new "Menu Bar" settings section with a live preview.

## What It Shows

The macOS system tray item displays rate limit utilization as visual progress bars, with optional percentage text and cost. Each element is independently configurable.

## Configuration Elements

### 1. Bars
- **Display:** Off / Single / Both
- **Provider** (when Single): Claude / Codex
- When "Both": two thin horizontal bars stacked vertically (Claude on top, Codex below)
- When "Single": one wider bar for the selected provider
- Bar color: Claude = `#d4a574` (amber), Codex = `#7aafff` (blue)
- Bar track background: `rgba(255,255,255,0.12)`

### 2. Percentages
- **Show:** toggle on/off
- **Format** (when shown):
  - Compact: `72 · 35` — bar colors identify which is which, top = top bar
  - Verbose: `Claude Code 72% Codex 35%` — full provider names with percentages

### 3. Cost
- **Show:** toggle on/off (replaces current `showTrayAmount`)
- **Precision** (when shown):
  - Whole: `$17`
  - Full: `$17.19`

## Settings UI

### Live Preview
A zoomed (~1.6×) rendering of the menu bar fragment, displayed on a macOS-style gradient wallpaper background. The fragment shows the TokenMonitor widget in context between neighboring tray icons (battery, Wi-Fi, clock). Updates instantly as the user changes settings below.

### Configuration Panel
Three sections (Bars, Percentages, Cost) using the existing Settings.svelte card/row/segmented-control pattern. Disabled rows dim to ~22% opacity when their parent toggle is off.

## Settings Schema Changes

Replace `showTrayAmount: boolean` with:

```typescript
interface TrayConfig {
  barDisplay: 'off' | 'single' | 'both';    // default: 'both'
  barProvider: 'claude' | 'codex';            // default: 'claude' (used when barDisplay === 'single')
  showPercentages: boolean;                    // default: false
  percentageFormat: 'compact' | 'verbose';     // default: 'compact'
  showCost: boolean;                           // default: true
  costPrecision: 'whole' | 'full';             // default: 'full'
}
```

Backward compatibility: migrate existing `showTrayAmount` to `showCost`.

## Rust Backend Changes

### Dynamic Tray Rendering
- Render bars as RGBA image data and set via `tray.set_icon()`
- Combine icon + bars into a single template image
- Render at 2× for Retina (44px height for 22px logical)
- Update on each refresh cycle and when rate limit data changes

### Tray Title
- Compose title string from percentages + cost based on config
- Format: `"72 · 35  $17.19"` or `"$17"` or `""` depending on toggles
- Set via `tray.set_title()`

### New IPC Commands
- `set_tray_config(config: TrayConfig)` — replaces `set_show_tray_amount`
- `get_tray_config() -> TrayConfig`

## Data Flow

1. Rate limit data arrives via existing `rateLimitMonitor` polling
2. Frontend sends tray config changes via `set_tray_config` IPC
3. Rust backend renders bar image + title string
4. `tray.set_icon()` + `tray.set_title()` update the macOS menu bar
5. Settings preview component reads the same rate limit data to render the zoomed mockup

## Migration

- `showTrayAmount: true` → `{ showCost: true, costPrecision: 'full', barDisplay: 'off', showPercentages: false }`
- `showTrayAmount: false` → `{ showCost: false, barDisplay: 'off', showPercentages: false }`
- Existing users keep their current behavior; bars are opt-in (default: 'both' for new installs only)

## Scope Exclusions

- No custom bar colors (uses provider brand colors)
- No bar width/height configuration
- No animation in the tray (static render per update)
