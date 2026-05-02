<script lang="ts">
  import {
    DEFAULT_HEADER_TABS,
    MAX_HEADER_TAB_LABEL_LENGTH,
    areHeaderTabsEqual,
    settings,
    updateSetting,
    type Settings as SettingsType,
  } from "../stores/settings.js";
  import {
    ALL_USAGE_PROVIDER_ID,
    getUsageProviderTitle,
    USAGE_PROVIDER_ORDER,
  } from "../providerMetadata.js";
  import type { HeaderTabConfig, UsageProvider } from "../types/index.js";
  import Toggle from "./Toggle.svelte";
  import ToggleSwitch from "./ToggleSwitch.svelte";

  let current = $derived($settings as SettingsType);

  const HEADER_TAB_FIELDS: Array<{ provider: UsageProvider; title: string }> = USAGE_PROVIDER_ORDER.map(
    (provider) => ({
      provider,
      title: getUsageProviderTitle(provider),
    }),
  );

  function createHeaderLabelInputs(nextHeaderTabs: SettingsType["headerTabs"]): Record<string, string> {
    const inputs: Record<string, string> = {};
    for (const provider of USAGE_PROVIDER_ORDER) {
      inputs[provider] = nextHeaderTabs[provider]?.label ?? DEFAULT_HEADER_TABS[provider]?.label ?? provider;
    }
    return inputs;
  }

  function syncHeaderLabelInputs(nextHeaderTabs: SettingsType["headerTabs"]) {
    headerLabelInputs = createHeaderLabelInputs(nextHeaderTabs);
  }

  let headerLabelInputs = $state<Record<string, string>>(createHeaderLabelInputs(DEFAULT_HEADER_TABS));

  let syncedHeaderTabs = $state(DEFAULT_HEADER_TABS);
  $effect(() => {
    const nextHeaderTabs = current.headerTabs;
    if (!areHeaderTabsEqual(syncedHeaderTabs, nextHeaderTabs)) {
      syncHeaderLabelInputs(nextHeaderTabs);
      syncedHeaderTabs = nextHeaderTabs;
    }
  });

  /** Live-updated tab list for the preview Toggle. Pulls labels from
   * the local input state (so they update as the user types) and
   * filters by the persisted `enabled` flag (so disabling a tab in
   * the table below removes it from the preview immediately). The
   * underlying header in the popover uses the same `getVisibleHeaderProviders`
   * derivation, so the preview is faithful by construction. */
  let previewOptions = $derived.by(() =>
    HEADER_TAB_FIELDS.filter(({ provider }) => current.headerTabs[provider].enabled).map(
      ({ provider }) => ({
        value: provider,
        label: headerLabelInputs[provider] || DEFAULT_HEADER_TABS[provider]?.label || provider,
      }),
    ),
  );

  /** Local active-provider state for the preview only — independent of
   * the popover's real `activeProvider` so clicking a preview tab
   * doesn't navigate the user away from where they are. We pin to the
   * first visible tab whenever the visible set changes (i.e. when the
   * currently-previewed tab is disabled, or when no tabs are left). */
  let previewActive = $state<UsageProvider>(USAGE_PROVIDER_ORDER[0]);
  $effect(() => {
    const visible = previewOptions;
    if (visible.length === 0) return;
    if (!visible.some((opt) => opt.value === previewActive)) {
      previewActive = visible[0].value;
    }
  });

  function updateHeaderTab(
    provider: UsageProvider,
    patch: Partial<HeaderTabConfig>,
  ) {
    updateSetting("headerTabs", {
      ...current.headerTabs,
      [provider]: {
        ...current.headerTabs[provider],
        ...patch,
      },
    });
  }

  function handleHeaderTabEnabled(provider: UsageProvider, enabled: boolean) {
    updateHeaderTab(provider, { enabled });
  }

  function handleHeaderLabelInput(provider: UsageProvider, value: string) {
    headerLabelInputs = {
      ...headerLabelInputs,
      [provider]: value,
    };
  }

  function persistHeaderLabel(provider: UsageProvider) {
    updateHeaderTab(provider, { label: headerLabelInputs[provider] });
  }

  function handleHeaderLabelKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      (e.target as HTMLInputElement).blur();
    }
  }
</script>

<div class="group">
  <div class="group-label">Header</div>

  <!-- Live preview: mirrors the Menubar/Floating-Ball preview pattern in
       TrayConfigSettings so every visual settings group has the same
       "see what you're configuring" feedback. Renders the real Toggle
       component with full brand theming (icons + accent colors) so the
       preview is exact, not a stand-in.

       Brand-color scoping: `--accent` and `--accent-soft` are normally
       set on `<html>` via `applyProvider(activeProvider)`, so the popover's
       active provider colors *every* descendant — including this preview.
       Without a fix, clicking "Claude" inside the preview while the
       popover was on Codex would tint the Claude logo Codex-blue (and
       vice versa), because the preview was reading the popover's accent
       not its own. We re-set `data-provider` on the preview wrapper so
       the brand-color cascade restarts here, anchored to `previewActive`
       — that way the preview always shows the brand color of the tab
       the user is actively previewing, regardless of where the popover
       happens to be. The `null` for "all" is intentional: the popover
       removes the attribute in that case (no brand override) and so
       does the preview, so they share the same neutral default. -->
  <div
    class="header-preview"
    data-provider={previewActive === ALL_USAGE_PROVIDER_ID ? null : previewActive}
  >
    {#if previewOptions.length > 0}
      <Toggle
        active={previewActive}
        options={previewOptions}
        brandTheming={current.brandTheming}
        onChange={(p) => (previewActive = p)}
      />
    {:else}
      <div class="header-preview-empty">Enable at least one tab to see the preview.</div>
    {/if}
  </div>

  <div class="card">
    {#each HEADER_TAB_FIELDS as tab, index}
      <div class="row header-tab-row" class:border={index < HEADER_TAB_FIELDS.length - 1}>
        <span class="header-source">{tab.title}</span>
        <input
          class="text-input"
          type="text"
          maxlength={MAX_HEADER_TAB_LABEL_LENGTH}
          value={headerLabelInputs[tab.provider]}
          oninput={(e) => handleHeaderLabelInput(tab.provider, (e.target as HTMLInputElement).value)}
          onblur={() => persistHeaderLabel(tab.provider)}
          onkeydown={handleHeaderLabelKeydown}
        />
        <ToggleSwitch
          checked={current.headerTabs[tab.provider].enabled}
          onChange={(checked) => handleHeaderTabEnabled(tab.provider, checked)}
        />
      </div>
    {/each}
  </div>
  <div class="setting-note">Labels are cosmetic. Usage data is still backed by the registered usage integrations.</div>
</div>

<style>
  .group {
    margin-bottom: 8px;
  }
  /* `.group-label` is defined globally in `src/app.css`. */
  .card {
    background: var(--surface-2);
    border-radius: 8px;
    overflow: hidden;
  }
  /* Configuration rows: tighter than the standard `.row` (7px 10px) so
     three rows + two dividers + a preview all fit comfortably above
     the fold without the section feeling like a wall. The text input
     and toggle each shrunk a step too — their default sizes were
     calibrated for full-width settings, not a three-column layout. */
  .row {
    padding: 5px 10px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .row.border {
    border-bottom: 1px solid var(--border-subtle);
  }
  .header-tab-row {
    gap: 7px;
  }
  .header-source {
    width: 40px;
    flex-shrink: 0;
    font: 500 9px/1 'Inter', sans-serif;
    color: var(--t2);
  }
  .text-input {
    min-width: 0;
    flex: 1;
    border: 1px solid var(--border-subtle);
    border-radius: 5px;
    background: var(--surface-hover);
    color: var(--t1);
    font: 400 9px/1 'Inter', sans-serif;
    padding: 4px 6px;
  }
  .text-input:focus {
    outline: none;
    border-color: var(--border);
  }
  .setting-note {
    font: 400 8px/1.35 'Inter', sans-serif;
    color: var(--t4);
    padding: 4px 4px 0;
  }

  /* Preview surface. */
  .header-preview {
    background: var(--surface-2);
    border-radius: 8px;
    padding: 6px 4px 8px;
    margin-bottom: 8px;
    overflow: hidden;
  }
  /* Pull Toggle's intrinsic top padding back so the segmented control
     sits visually centered inside the preview surface instead of
     hugging the bottom edge — Toggle assumes it's the first thing in
     the popover (which is why .tog-wrap has padding-top: 10px). */
  .header-preview :global(.tog-wrap) {
    padding: 0 8px;
  }

  /* ── Preview brand-color scope ─────────────────────────────────────
     These rules mirror the `[data-provider="…"]` block in `app.css`
     but anchor them to `.header-preview` so the cascade restarts at
     the wrapper. The `app.css` rules use `[data-theme="light"][data-provider]`
     selectors that require both attributes on the *same* element —
     fine for `<html>`, but `data-theme` lives on `<html>` and our
     `data-provider` lives on `.header-preview`, so the global rules
     can't reach this scope. We re-establish them here.

     Values are duplicated rather than shared via a custom property
     because the upstream rules already use literal hex values; one
     source of truth would mean refactoring app.css too, which is
     outside the scope of this preview. If brand colors ever change,
     update both locations. */
  .header-preview[data-provider="claude"] {
    --accent: #E8784A;
    --accent-soft: rgba(232, 120, 74, 0.12);
  }
  :global([data-theme="light"]) .header-preview[data-provider="claude"] {
    --accent: #C85E2A;
    --accent-soft: rgba(200, 94, 42, 0.14);
  }
  .header-preview[data-provider="codex"] {
    --accent: #52A8DC;
    --accent-soft: rgba(82, 168, 220, 0.12);
  }
  :global([data-theme="light"]) .header-preview[data-provider="codex"] {
    --accent: #2E7EB5;
    --accent-soft: rgba(46, 126, 181, 0.14);
  }
  /* Empty state for the no-tabs-enabled corner case. Subtle enough
     that it reads as "informational" rather than "error." */
  .header-preview-empty {
    padding: 12px 12px;
    text-align: center;
    font: 400 9.5px/1.4 'Inter', sans-serif;
    color: var(--t4);
  }
</style>
