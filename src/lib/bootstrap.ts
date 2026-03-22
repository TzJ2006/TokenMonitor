import { invoke } from "@tauri-apps/api/core";
import { activePeriod, activeProvider } from "./stores/usage.js";
import { applyGlass, applyTheme, resolveVisibleProvider, type Settings } from "./stores/settings.js";
import { syncTrayConfig } from "./traySync.js";
import { syncNativeWindowSurface } from "./windowAppearance.js";

type StartupDeps = {
  invokeFn?: typeof invoke;
  applyThemeFn?: typeof applyTheme;
  applyGlassFn?: typeof applyGlass;
  syncNativeWindowSurfaceFn?: (invokeFn?: typeof invoke, glassEnabled?: boolean) => Promise<void>;
};

export async function initializeRuntimeFromSettings(
  saved: Settings,
  deps: StartupDeps = {},
) {
  const invokeFn = deps.invokeFn ?? invoke;
  const applyThemeFn = deps.applyThemeFn ?? applyTheme;
  const applyGlassFn = deps.applyGlassFn ?? applyGlass;
  const syncNativeWindowSurfaceFn =
    deps.syncNativeWindowSurfaceFn ?? syncNativeWindowSurface;
  const provider = resolveVisibleProvider(saved.defaultProvider, saved.headerTabs);

  applyThemeFn(saved.theme);
  applyGlassFn(saved.glassEffect);
  activeProvider.set(provider);
  activePeriod.set(saved.defaultPeriod);

  try {
    await invokeFn("set_glass_effect", { enabled: saved.glassEffect });
  } catch {
    // Keep startup resilient if the backend IPC is not ready yet.
  }

  try {
    await invokeFn("set_dock_icon_visible", { visible: saved.showDockIcon });
  } catch {
    // Keep startup resilient if the backend IPC is not ready yet.
  }

  try {
    await syncNativeWindowSurfaceFn(invokeFn, saved.glassEffect);
  } catch {
    // Keep startup resilient if the backend IPC is not ready yet.
  }

  try {
    await invokeFn("set_refresh_interval", { interval: saved.refreshInterval });
    await syncTrayConfig(saved.trayConfig, null, invokeFn);
  } catch {
    // Keep startup resilient if the backend IPC is not ready yet.
  }

  return {
    provider,
    period: saved.defaultPeriod,
  };
}
