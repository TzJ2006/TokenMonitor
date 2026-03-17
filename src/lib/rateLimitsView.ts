import type { ProviderRateLimits } from "./types/index.js";

export type ProviderRateLimitViewState = "ready" | "error" | "empty";

export function hasRateLimitWindows(
  rateLimits: ProviderRateLimits | null | undefined,
): boolean {
  return (rateLimits?.windows.length ?? 0) > 0;
}

export function providerRateLimitViewState(
  rateLimits: ProviderRateLimits | null | undefined,
): ProviderRateLimitViewState {
  if (hasRateLimitWindows(rateLimits)) return "ready";
  if (rateLimits?.error) return "error";
  return "empty";
}

export function providerHasActiveCooldown(
  rateLimits: ProviderRateLimits | null | undefined,
  now = Date.now(),
): boolean {
  if (!rateLimits?.cooldownUntil) return false;
  return new Date(rateLimits.cooldownUntil).getTime() > now;
}
