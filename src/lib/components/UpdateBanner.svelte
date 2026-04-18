<script lang="ts">
  import {
    updaterStore,
    installUpdate,
    skipVersion,
    dismissBanner,
  } from "../stores/updater.js";
  import { openUrl } from "@tauri-apps/plugin-opener";

  let installing = $state(false);
  let installError = $state<string | null>(null);

  const snapshot = $derived($updaterStore);
  const available = $derived(snapshot.available);
  const progress = $derived(snapshot.progress);
  const isSkipped = $derived(
    available ? snapshot.skippedVersions.includes(available.version) : false,
  );
  const visible = $derived(
    available != null && !snapshot.dismissedForSession && !isSkipped,
  );
  const manualInstall = $derived(snapshot.installMode === "manual");

  async function onUpdate() {
    if (manualInstall && available) {
      await openUrl(
        `https://github.com/Michael-OvO/TokenMonitor/releases/tag/v${available.version}`,
      );
      await dismissBanner();
      return;
    }
    installing = true;
    installError = null;
    try {
      await installUpdate();
    } catch (e) {
      installError = e instanceof Error ? e.message : String(e);
      installing = false;
    }
  }

  async function onSkip() {
    if (!available) return;
    await skipVersion(available.version);
  }

  async function onLater() {
    await dismissBanner();
  }

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

{#if visible && available}
  <div class="update-banner" role="status">
    {#if installError}
      <div class="msg error">Update failed: {installError}</div>
      <div class="actions">
        <button class="primary" onclick={onUpdate}>Retry</button>
        <button onclick={onLater}>Dismiss</button>
      </div>
    {:else if installing && progress}
      <div class="msg">
        Downloading v{available.version}… {progress.percent?.toFixed(0) ?? "?"}%
        {#if progress.total}
          ({formatBytes(progress.downloaded)} / {formatBytes(progress.total)})
        {/if}
      </div>
      <div class="progress-track">
        <div class="progress-fill" style="width: {progress.percent ?? 0}%"></div>
      </div>
    {:else}
      <div class="msg">
        <strong>Update available:</strong> v{available.version}
        {#if available.notes}
          <details><summary>Release notes</summary><pre>{available.notes}</pre></details>
        {/if}
      </div>
      <div class="actions">
        <button class="primary" onclick={onUpdate}>
          {manualInstall ? "Download" : "Update Now"}
        </button>
        <button onclick={onLater}>Later</button>
        <button onclick={onSkip}>Skip</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .update-banner {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: rgba(64, 128, 255, 0.12);
    border-bottom: 1px solid rgba(64, 128, 255, 0.35);
    font-size: 12px;
  }
  .msg.error {
    color: #c23;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  button {
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 4px;
    border: 1px solid rgba(0, 0, 0, 0.2);
    background: transparent;
    cursor: pointer;
  }
  button.primary {
    background: rgba(64, 128, 255, 0.25);
    border-color: rgba(64, 128, 255, 0.6);
    font-weight: 500;
  }
  .progress-track {
    height: 4px;
    background: rgba(0, 0, 0, 0.1);
    border-radius: 2px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: rgba(64, 128, 255, 0.8);
    transition: width 0.2s linear;
  }
  details {
    margin-top: 6px;
    font-size: 11px;
  }
  pre {
    white-space: pre-wrap;
    max-height: 120px;
    overflow: auto;
    font-size: 11px;
    margin: 4px 0;
  }
</style>
