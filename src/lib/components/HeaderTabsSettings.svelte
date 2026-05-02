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
       "see what you're configuring" feedback.
       Uses the real Toggle component for fidelity, with two deliberate
       overrides:
         1. `brandTheming={false}` — drops the per-brand provider-logo
            row above the segmented control. That row is taller than
            the segmented control itself and isn't directly relevant
            to label/order configuration.
         2. The wrapper resets `--accent-soft` to a neutral surface
            color. The popover's accent flips with the active provider
            (`applyProvider`), so an unscoped Toggle would highlight
            the preview's "active" segment in whichever brand the
            user happens to have selected globally — making it look
            as if clicking "Claude" colored it Codex-blue (or vice
            versa). Pinning to a brand-neutral grey makes the preview
            mean only "this is the order and labels," nothing more. -->
  <div class="header-preview">
    {#if previewOptions.length > 0}
      <Toggle
        active={previewActive}
        options={previewOptions}
        brandTheming={false}
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

  /* Preview surface. Tighter than `.tray-preview` because the hosted
     Toggle no longer carries its brand-logo row (brandTheming=false),
     which means there's nothing tall enough to need extra padding. */
  .header-preview {
    background: var(--surface-2);
    border-radius: 8px;
    padding: 8px 4px;
    margin-bottom: 8px;
    overflow: hidden;
    /* Brand-neutral slider color in the preview scope only. App.svelte
       sets `--accent-soft` based on the popover's active provider
       (applyProvider); without this override the preview would tint
       its "active" segment in whatever brand color the popover is
       currently on, which lies about what the configured header will
       look like when each tab activates. Pinning to a low-alpha
       neutral makes the active highlight purely positional, not
       brand-coded. */
    --accent-soft: rgba(255, 255, 255, 0.08);
  }
  :global([data-theme="light"]) .header-preview {
    --accent-soft: rgba(0, 0, 0, 0.06);
  }
  /* Pull Toggle's intrinsic top padding back so the segmented control
     sits visually centered inside the preview surface instead of
     hugging the bottom edge — Toggle assumes it's the first thing in
     the popover (which is why .tog-wrap has padding-top: 10px). */
  .header-preview :global(.tog-wrap) {
    padding: 0 8px;
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
