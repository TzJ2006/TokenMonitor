<script lang="ts">
  interface Props {
    checked: boolean;
    color?: string;
    onChange: (checked: boolean) => void;
  }

  let { checked, color = "#34C759", onChange }: Props = $props();
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
    width: 32px;
    height: 20px;
    border-radius: 10px;
    background: rgba(120, 120, 128, 0.32);
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
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    position: absolute;
    top: 2px;
    left: 2px;
    transition: transform 0.2s ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }
  .toggle.on .knob {
    transform: translateX(12px);
  }
</style>
