<script lang="ts">
  interface Props {
    checked: boolean;
    color?: string;
    onChange: (checked: boolean) => void;
  }

  let { checked, color = "var(--haiku)", onChange }: Props = $props();
</script>

<button
  class="toggle"
  class:on={checked}
  style:--toggle-color={color}
  onclick={() => onChange(!checked)}
  role="switch"
  aria-checked={checked}
  aria-label="Toggle"
>
  <div class="knob"></div>
</button>

<style>
  .toggle {
    width: 28px;
    height: 16px;
    border-radius: 8px;
    background: var(--border);
    border: none;
    cursor: pointer;
    position: relative;
    transition: background 0.2s ease;
    flex-shrink: 0;
  }
  .toggle.on {
    background: var(--toggle-color);
  }
  .knob {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    position: absolute;
    top: 2px;
    left: 2px;
    transition: transform 0.2s ease;
  }
  .toggle.on .knob {
    transform: translateX(12px);
  }
</style>
