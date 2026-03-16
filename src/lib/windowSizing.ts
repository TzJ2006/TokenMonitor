export const WINDOW_WIDTH = 340;
export const WINDOW_HEIGHT_PADDING = 2;
export const MIN_WINDOW_HEIGHT = 100;
export const DEFAULT_MAX_WINDOW_HEIGHT = 2400;
export const WINDOW_MONITOR_MARGIN = 24;
export const RESIZE_SETTLE_DELAY_MS = 16;

export type ResizeDisposition = "grow" | "shrink" | "skip";

export function measureTargetWindowHeight(contentHeight: number): number {
  return Math.ceil(contentHeight) + WINDOW_HEIGHT_PADDING;
}

export function clampWindowHeight(
  targetHeight: number,
  maxWindowHeight = DEFAULT_MAX_WINDOW_HEIGHT,
  minimumHeight = MIN_WINDOW_HEIGHT,
): number {
  const boundedMax = Math.max(maxWindowHeight, minimumHeight);
  return Math.max(minimumHeight, Math.min(targetHeight, boundedMax));
}

export function resolveMonitorMaxWindowHeight(
  workAreaPhysicalHeight: number,
  scaleFactor: number,
  fallbackMaxWindowHeight = DEFAULT_MAX_WINDOW_HEIGHT,
  monitorMargin = WINDOW_MONITOR_MARGIN,
): number {
  if (!Number.isFinite(workAreaPhysicalHeight) || workAreaPhysicalHeight <= 0) {
    return fallbackMaxWindowHeight;
  }

  if (!Number.isFinite(scaleFactor) || scaleFactor <= 0) {
    return fallbackMaxWindowHeight;
  }

  const logicalWorkAreaHeight = Math.floor(workAreaPhysicalHeight / scaleFactor) - monitorMargin;
  return clampWindowHeight(logicalWorkAreaHeight, fallbackMaxWindowHeight);
}

export function classifyResize(
  targetHeight: number,
  lastWindowHeight: number,
  minimumHeight = MIN_WINDOW_HEIGHT,
): ResizeDisposition {
  if (!Number.isFinite(targetHeight) || targetHeight < minimumHeight) {
    return "skip";
  }

  if (targetHeight === lastWindowHeight) {
    return "skip";
  }

  return targetHeight > lastWindowHeight ? "grow" : "shrink";
}
