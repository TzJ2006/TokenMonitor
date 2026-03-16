<script lang="ts">
  interface Props {
    periodLabel: string;
    hasEarlierData: boolean;
    isAtPresent: boolean;
    onBack: () => void;
    onForward: () => void;
    onReset: () => void;
  }
  let { periodLabel, hasEarlierData, isAtPresent, onBack, onForward, onReset }: Props = $props();
</script>

<div class="date-nav">
  <button
    class="arrow"
    class:disabled={!hasEarlierData}
    disabled={!hasEarlierData}
    onclick={onBack}
    aria-label="Previous period"
  >&#8249;</button>

  <button
    class="label"
    class:clickable={!isAtPresent}
    onclick={onReset}
    disabled={isAtPresent}
    aria-label="Return to current period"
  >{periodLabel}</button>

  <button
    class="arrow"
    class:disabled={isAtPresent}
    disabled={isAtPresent}
    onclick={onForward}
    aria-label="Next period"
  >&#8250;</button>
</div>

<style>
  .date-nav {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 4px 12px 0;
    animation: fadeUp .28s ease both .05s;
  }
  .arrow {
    background: none;
    border: none;
    font-size: 14px;
    color: var(--t2);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
    transition: color .2s, opacity .2s;
    line-height: 1;
  }
  .arrow:hover:not(:disabled) {
    color: var(--t1);
    background: rgba(255,255,255,0.02);
  }
  .arrow.disabled {
    color: var(--t4, var(--t3));
    cursor: default;
    opacity: 0.4;
  }
  .label {
    background: none;
    border: none;
    font: 500 10px/1 'Inter', sans-serif;
    color: var(--t1);
    letter-spacing: 0.2px;
    cursor: default;
    padding: 2px 4px;
    border-radius: 4px;
    transition: color .2s;
  }
  .label.clickable {
    cursor: pointer;
  }
  .label.clickable:hover {
    color: var(--accent);
  }
</style>
