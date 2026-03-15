<script lang="ts">
  import type { SetupStatus } from "../types/index.js";

  interface Props { status: SetupStatus }
  let { status }: Props = $props();
</script>

<div class="setup">
  {#if status.installing}
    <div class="spinner"></div>
    <div class="msg">Setting up ccusage...</div>
    <div class="sub">First-time install, this takes a moment</div>
  {:else if status.error}
    <div class="icon">!</div>
    <div class="msg">Setup failed</div>
    <div class="sub err">{status.error}</div>
  {:else}
    <div class="spinner"></div>
    <div class="msg">Initializing...</div>
  {/if}
</div>

<style>
  .setup {
    display: flex; flex-direction: column; align-items: center;
    justify-content: center; height: 100%; padding: 40px 24px;
    text-align: center;
  }
  .spinner {
    width: 20px; height: 20px;
    border: 2px solid var(--border);
    border-top-color: var(--t2);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin-bottom: 12px;
  }
  .icon {
    width: 24px; height: 24px;
    border: 1.5px solid var(--sonnet);
    border-radius: 50%;
    font: 600 13px/24px 'Inter', sans-serif;
    color: var(--sonnet);
    margin-bottom: 12px;
  }
  .msg { font: 500 11px/1 'Inter', sans-serif; color: var(--t2); margin-bottom: 6px; }
  .sub { font: 400 9px/1.4 'Inter', sans-serif; color: var(--t3); max-width: 220px; }
  .err { color: var(--sonnet); }
</style>
