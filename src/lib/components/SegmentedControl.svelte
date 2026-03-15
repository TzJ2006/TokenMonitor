<script lang="ts">
  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    options: Option[];
    value: string;
    onChange: (value: string) => void;
  }

  let { options, value, onChange }: Props = $props();
</script>

<div class="seg">
  {#each options as opt}
    <button
      class="seg-btn"
      class:active={value === opt.value}
      onclick={() => onChange(opt.value)}
    >
      {opt.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    background: var(--surface-2);
    border-radius: 5px;
    overflow: hidden;
    gap: 1px;
  }
  .seg-btn {
    padding: 3px 7px;
    font: 500 8px/1 'Inter', sans-serif;
    color: var(--t3);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: all 0.18s ease;
    white-space: nowrap;
  }
  .seg-btn:hover {
    color: var(--t2);
  }
  .seg-btn.active {
    background: var(--surface-hover);
    color: var(--t1);
    border-radius: 4px;
  }
</style>
