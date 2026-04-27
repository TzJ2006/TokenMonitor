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
  import ToggleSwitch from "./ToggleSwitch.svelte";

  let current = $derived($settings as SettingsType);

  let expanded = $state(false);

  const HEADER_TAB_FIELDS: Array<{ provider: UsageProvider; title: string }> = USAGE_PROVIDER_ORDER.map(
    (provider) => ({
      provider,
      title: getUsageProviderTitle(provider),
    }),
  );

  let enabledCount = $derived(
    HEADER_TAB_FIELDS.filter((tab) => current.headerTabs[tab.provider].enabled).length,
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

<div class="card">
  <button class="row toggle-row" type="button" onclick={() => (expanded = !expanded)}>
    <span class="label">Provider</span>
    <div class="toggle-right">
      <span class="toggle-count">{enabledCount} of {HEADER_TAB_FIELDS.length} enabled</span>
      <svg class="toggle-chevron" class:open={expanded} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="6 9 12 15 18 9"></polyline>
      </svg>
    </div>
  </button>
  <div class="collapse" class:open={expanded}>
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
</div>

<style>
  .card {
    background: var(--surface-2);
    border-radius: 8px;
    overflow: hidden;
  }
  .row {
    padding: 7px 10px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .row.border {
    border-bottom: 1px solid var(--border-subtle);
  }
  .toggle-row {
    width: 100%;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    cursor: pointer;
    user-select: none;
  }
  .toggle-row:hover {
    background: var(--surface-hover);
  }
  .toggle-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .toggle-count {
    font: 400 9px/1 'Inter', sans-serif;
    color: var(--t3);
  }
  .toggle-chevron {
    color: var(--t3);
    transition: transform var(--t-normal) ease;
    transform: rotate(-90deg);
  }
  .toggle-chevron.open {
    transform: rotate(0deg);
  }
  .collapse {
    max-height: 0;
    overflow: hidden;
    transition: max-height var(--t-normal) ease;
  }
  .collapse.open {
    max-height: 300px;
  }
  .label {
    font: 400 10px/1 'Inter', sans-serif;
    color: var(--t1);
  }
  .header-tab-row {
    gap: 8px;
  }
  .header-source {
    width: 46px;
    flex-shrink: 0;
    font: 500 9px/1 'Inter', sans-serif;
    color: var(--t2);
  }
  .text-input {
    min-width: 0;
    flex: 1;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--surface-hover);
    color: var(--t1);
    font: 400 9px/1 'Inter', sans-serif;
    padding: 5px 7px;
  }
  .text-input:focus {
    outline: none;
    border-color: var(--border);
  }
</style>
