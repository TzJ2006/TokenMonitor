<script lang="ts">
  import { modelColor, formatCost, formatTokens } from "../utils/format.js";
  import { settings } from "../stores/settings.js";
  import type { ModelSummary } from "../types/index.js";

  interface Props { models: ModelSummary[] }
  let { models }: Props = $props();

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
</script>

<div class="mdl">
  {#each sorted as m}
    <div class="mr">
      <span class="mb" style="background:{modelColor(m.model_key)}"></span>
      <span class="mn">{m.display_name}</span>
      <span class="mc">{formatCost(m.cost)}</span>
      <span class="mt">{formatTokens(m.tokens)}</span>
    </div>
  {/each}
</div>

<style>
  .mdl { padding: 6px 12px 8px; animation: fadeUp .28s ease both .12s; }
  .mr {
    display: flex; align-items: center; padding: 5px 7px; border-radius: 6px;
    transition: background .15s; gap: 7px;
  }
  .mr:hover { background: var(--surface-2); }
  .mb { width: 2.5px; height: 14px; border-radius: 1.5px; flex-shrink: 0; }
  .mn { font: 400 10px/1 'Inter', sans-serif; color: var(--t2); flex: 1; letter-spacing: .1px; }
  .mc { font: 500 10px/1 'Inter', sans-serif; color: var(--t1); font-variant-numeric: tabular-nums; }
  .mt { font: 400 9px/1 'Inter', sans-serif; color: var(--t3); font-variant-numeric: tabular-nums; min-width: 32px; text-align: right; }
</style>
