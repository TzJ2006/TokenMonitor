<script lang="ts">
  import { modelColor, formatCost, formatTokens } from "../utils/format.js";
  import { settings } from "../stores/settings.js";
  import type { ModelSummary } from "../types/index.js";

  interface Props { models: ModelSummary[] }
  let { models }: Props = $props();

  const MAX_VISIBLE = 5;
  let expanded = $state(false);

  let hiddenModels = $state<string[]>([]);
  $effect(() => {
    const unsub = settings.subscribe((s) => (hiddenModels = s.hiddenModels));
    return unsub;
  });

  let sorted = $derived(
    [...models]
      .filter((m) => !hiddenModels.includes(m.model_key))
      .sort((a, b) => b.cost - a.cost)
  );

  let visible = $derived(expanded ? sorted : sorted.slice(0, MAX_VISIBLE));
  let remaining = $derived(sorted.length - MAX_VISIBLE);
</script>

<div class="mdl">
  {#each visible as m}
    <div class="mr">
      <span class="mb" style="background:{modelColor(m.model_key)}"></span>
      <span class="mn">{m.display_name}</span>
      <span class="mc">{formatCost(m.cost)}</span>
      <span class="mt">{formatTokens(m.tokens)}</span>
    </div>
  {/each}
  {#if remaining > 0}
    <button class="more" onclick={() => expanded = !expanded}>
      {expanded ? "Show less" : `+ ${remaining} more`}
    </button>
  {/if}
</div>

<style>
  .mdl { padding: 6px 12px 8px; animation: fadeUp .28s ease both .12s; }
  .mr {
    display: flex; align-items: center; padding: 5px 7px; border-radius: 6px;
    transition: background .15s; gap: 7px;
  }
  .mr:hover { background: var(--surface-2); }
  .mb { width: 2.5px; height: 14px; border-radius: 1.5px; flex-shrink: 0; }
  .mn { font: 400 10px/1 'Inter', sans-serif; color: var(--t2); flex: 1; letter-spacing: .1px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mc { font: 500 10px/1 'Inter', sans-serif; color: var(--t1); font-variant-numeric: tabular-nums; }
  .mt { font: 400 9px/1 'Inter', sans-serif; color: var(--t3); font-variant-numeric: tabular-nums; min-width: 32px; text-align: right; }
  .more {
    display: block; width: 100%; padding: 4px 7px; margin-top: 2px;
    border: none; background: none; cursor: pointer;
    font: 400 9px/1 'Inter', sans-serif; color: var(--t3);
    text-align: left; border-radius: 6px; transition: color .15s, background .15s;
  }
  .more:hover { color: var(--t2); background: var(--surface-2); }
</style>
