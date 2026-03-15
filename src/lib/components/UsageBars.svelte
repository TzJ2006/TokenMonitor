<script lang="ts">
  import { formatCost, formatTokens, modelColor } from "../utils/format.js";
  import { settings } from "../stores/settings.js";
  import type { UsagePayload } from "../types/index.js";

  interface Props { data: UsagePayload }
  let { data }: Props = $props();

  let hiddenModels = $state<string[]>([]);
  $effect(() => {
    const unsub = settings.subscribe((s) => (hiddenModels = s.hiddenModels));
    return unsub;
  });

  // Max cost for the current 5h billing window (Anthropic's limit varies)
  // We'll show percentage relative to projected cost or a reasonable cap
  const FIVE_HOUR_CAP = 100; // $100 cap for visualization

  let sessionPct = $derived(
    Math.min((data.five_hour_cost / FIVE_HOUR_CAP) * 100, 100)
  );

  let burnRate = $derived(data.active_block?.burn_rate_per_hour ?? 0);
  let projected = $derived(data.active_block?.projected_cost ?? data.five_hour_cost);
  let projectedPct = $derived(Math.min((projected / FIVE_HOUR_CAP) * 100, 100));

  // Per-model bars (filtered by visibility)
  let modelBars = $derived(
    [...data.model_breakdown]
      .filter((m) => !hiddenModels.includes(m.model_key))
      .sort((a, b) => b.cost - a.cost)
      .map((m) => ({
        ...m,
        pct: data.total_cost > 0
          ? Math.max((m.cost / data.total_cost) * 100, 2)
          : 0,
      }))
  );
</script>

<div class="ub">
  <!-- Session cost bar -->
  <div class="ub-row">
    <div class="ub-head">
      <span class="ub-label">Session (5hr)</span>
      <span class="ub-val">{formatCost(data.five_hour_cost)}</span>
    </div>
    <div class="ub-track">
      <div
        class="ub-fill active"
        style="width: {sessionPct}%"
      ></div>
      {#if projected > data.five_hour_cost}
        <div
          class="ub-fill projected"
          style="width: {projectedPct}%; opacity: 0.25;"
        ></div>
      {/if}
    </div>
    <div class="ub-sub">
      {#if burnRate > 0}
        {formatCost(burnRate)}/hr burn rate
      {:else}
        No active session
      {/if}
    </div>
  </div>

  <!-- Per-model usage bars -->
  {#each modelBars as mb}
    <div class="ub-row">
      <div class="ub-head">
        <span class="ub-label">{mb.display_name}</span>
        <span class="ub-val">{formatCost(mb.cost)}</span>
      </div>
      <div class="ub-track">
        <div
          class="ub-fill"
          style="width: {mb.pct}%; background: {modelColor(mb.model_key)};"
        ></div>
      </div>
      <div class="ub-sub">{formatTokens(mb.tokens)} tokens</div>
    </div>
  {/each}
</div>

<style>
  .ub {
    padding: 10px 14px 6px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    animation: fadeUp .28s ease both .09s;
  }
  .ub-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ub-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .ub-label {
    font: 500 11px/1 'Inter', sans-serif;
    color: var(--t1);
  }
  .ub-val {
    font: 500 11px/1 'Inter', sans-serif;
    color: var(--t1);
    font-variant-numeric: tabular-nums;
  }
  .ub-track {
    position: relative;
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .ub-fill {
    position: absolute;
    top: 0; left: 0; height: 100%;
    border-radius: 3px;
    transition: width 0.5s cubic-bezier(.25,.8,.25,1);
  }
  .ub-fill.active {
    background: var(--opus);
  }
  .ub-fill.projected {
    background: var(--opus);
  }
  .ub-sub {
    font: 400 9px/1 'Inter', sans-serif;
    color: var(--t3);
  }
</style>
