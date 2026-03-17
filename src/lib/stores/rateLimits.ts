import { get, writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { load } from "@tauri-apps/plugin-store";
import { providerHasActiveCooldown } from "../rateLimitsView.js";
import type {
  ProviderRateLimits,
  RateLimitsPayload,
  UsageProvider,
} from "../types/index.js";

type RequestScope = UsageProvider;
type ProviderScope = Exclude<UsageProvider, "all">;

interface RateLimitsRequestState {
  loading: boolean;
  loaded: boolean;
  error: string | null;
  deferredUntil: string | null;
}

const CACHE_FILE = "rate-limits.json";
const CACHE_KEY = "payload";
const CLAUDE_MIN_FETCH_INTERVAL_MS = 300_000;
const PROVIDER_MIN_FETCH_INTERVAL_MS: Record<ProviderScope, number> = {
  claude: CLAUDE_MIN_FETCH_INTERVAL_MS,
  codex: 0,
};
const DEFAULT_REQUEST_STATE: RateLimitsRequestState = {
  loading: false,
  loaded: false,
  error: null,
  deferredUntil: null,
};

export const rateLimitsData = writable<RateLimitsPayload | null>(null);
export const rateLimitsRequestState =
  writable<RateLimitsRequestState>({ ...DEFAULT_REQUEST_STATE });

let storeInstance: Awaited<ReturnType<typeof load>> | null = null;
let hydratePromise: Promise<void> | null = null;
let inflightFetch: Promise<void> | null = null;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
let lastRequestedScope: RequestScope = "all";

function requestedProviders(scope: RequestScope): ProviderScope[] {
  return scope === "all" ? ["claude", "codex"] : [scope];
}

function providerPayload(
  payload: RateLimitsPayload | null,
  provider: ProviderScope,
): ProviderRateLimits | null {
  if (!payload) return null;
  return provider === "claude" ? payload.claude : payload.codex;
}

function mergeProviderRateLimits(
  fresh: ProviderRateLimits | null | undefined,
  cached: ProviderRateLimits | null | undefined,
): ProviderRateLimits | null {
  if (fresh && cached && fresh.windows.length === 0 && fresh.error && cached.windows.length > 0) {
    return {
      ...cached,
      stale: true,
      error: fresh.error,
      retryAfterSeconds: fresh.retryAfterSeconds,
      cooldownUntil: fresh.cooldownUntil,
      fetchedAt: fresh.fetchedAt,
    };
  }

  return fresh ?? cached ?? null;
}

function mergeRateLimitsPayloads(
  fresh: RateLimitsPayload,
  cached: RateLimitsPayload | null,
): RateLimitsPayload {
  return {
    claude: mergeProviderRateLimits(fresh.claude, cached?.claude),
    codex: mergeProviderRateLimits(fresh.codex, cached?.codex),
  };
}

function providerThrottleUntil(
  rateLimits: ProviderRateLimits | null,
  provider: ProviderScope,
  now = Date.now(),
): string | null {
  const minIntervalMs = PROVIDER_MIN_FETCH_INTERVAL_MS[provider];
  if (!rateLimits || minIntervalMs <= 0) return null;

  const fetchedAtMs = new Date(rateLimits.fetchedAt).getTime();
  if (!Number.isFinite(fetchedAtMs)) return null;

  const throttleUntilMs = fetchedAtMs + minIntervalMs;
  if (throttleUntilMs <= now) return null;

  return new Date(throttleUntilMs).toISOString();
}

function providerDeferredUntil(
  payload: RateLimitsPayload | null,
  provider: ProviderScope,
  now = Date.now(),
): string | null {
  const rateLimits = providerPayload(payload, provider);
  const activeCooldownUntil = providerHasActiveCooldown(rateLimits, now)
    ? rateLimits?.cooldownUntil ?? null
    : null;
  const throttleUntil = providerThrottleUntil(rateLimits, provider, now);

  if (activeCooldownUntil && throttleUntil) {
    return new Date(activeCooldownUntil).getTime() >= new Date(throttleUntil).getTime()
      ? activeCooldownUntil
      : throttleUntil;
  }

  return activeCooldownUntil ?? throttleUntil;
}

function earliestDeferredUntil(
  payload: RateLimitsPayload | null,
  scope: RequestScope,
): string | null {
  const deferredProviders = requestedProviders(scope)
    .map((provider) => providerDeferredUntil(payload, provider))
    .filter((value): value is string => Boolean(value));

  if (deferredProviders.length === 0) return null;
  deferredProviders.sort((left, right) => {
    return new Date(left).getTime() - new Date(right).getTime();
  });
  return deferredProviders[0];
}

function eligibleProviders(
  payload: RateLimitsPayload | null,
  scope: RequestScope,
): ProviderScope[] {
  return requestedProviders(scope).filter((provider) => {
    return providerDeferredUntil(payload, provider) === null;
  });
}

function fetchScopeFor(
  payload: RateLimitsPayload | null,
  scope: RequestScope,
): RequestScope | null {
  const eligible = eligibleProviders(payload, scope);
  if (eligible.length === 0) return null;
  if (scope !== "all") return scope;
  if (eligible.length === 2) return "all";
  return eligible[0];
}

function clearRetryTimer() {
  if (!retryTimer) return;
  clearTimeout(retryTimer);
  retryTimer = null;
}

function scheduleRetry(payload: RateLimitsPayload | null, scope: RequestScope) {
  clearRetryTimer();

  const deferredUntil = earliestDeferredUntil(payload, scope);
  rateLimitsRequestState.update((state) => ({
    ...state,
    deferredUntil,
  }));

  if (!deferredUntil) return;

  const delay = new Date(deferredUntil).getTime() - Date.now();
  if (delay <= 0) return;

  retryTimer = setTimeout(() => {
    retryTimer = null;
    void fetchRateLimits(lastRequestedScope);
  }, delay + 50);
}

async function ensureStore() {
  if (storeInstance) return storeInstance;
  storeInstance = await load(CACHE_FILE, { defaults: {}, autoSave: true });
  return storeInstance;
}

async function persistRateLimits(payload: RateLimitsPayload | null): Promise<void> {
  if (!storeInstance) return;

  try {
    await storeInstance.set(CACHE_KEY, payload);
    await storeInstance.save();
  } catch (error) {
    console.warn("Failed to persist rate limits:", error);
  }
}

function formatFetchError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Failed to fetch rate limits.";
}

export async function hydrateRateLimits(): Promise<void> {
  if (hydratePromise) return hydratePromise;

  hydratePromise = (async () => {
    try {
      const store = await ensureStore();
      const cached = (await store.get<RateLimitsPayload | null>(CACHE_KEY)) ?? null;
      if (!cached) return;
      rateLimitsData.set(cached);
      scheduleRetry(cached, lastRequestedScope);
    } catch (error) {
      console.warn("Failed to load persisted rate limits:", error);
    }
  })();

  return hydratePromise;
}

export async function fetchRateLimits(scope: RequestScope = "all"): Promise<void> {
  lastRequestedScope = scope;
  await hydrateRateLimits();

  const cached = get(rateLimitsData);
  const fetchScope = fetchScopeFor(cached, scope);
  if (!fetchScope) {
    scheduleRetry(cached, scope);
    rateLimitsRequestState.set({
      loading: false,
      loaded: true,
      error: null,
      deferredUntil: earliestDeferredUntil(cached, scope),
    });
    return;
  }

  if (inflightFetch) return inflightFetch;

  rateLimitsRequestState.set({
    ...get(rateLimitsRequestState),
    loading: true,
    error: null,
    deferredUntil: earliestDeferredUntil(cached, scope),
  });

  inflightFetch = (async () => {
    try {
      const payload = await invoke<RateLimitsPayload>("get_rate_limits", {
        provider: fetchScope === "all" ? null : fetchScope,
      });
      const mergedPayload = mergeRateLimitsPayloads(payload, cached);

      rateLimitsData.set(mergedPayload);
      await persistRateLimits(mergedPayload);
      scheduleRetry(mergedPayload, scope);
      rateLimitsRequestState.set({
        loading: false,
        loaded: true,
        error: null,
        deferredUntil: earliestDeferredUntil(mergedPayload, scope),
      });
    } catch (error) {
      console.error("Failed to fetch rate limits:", error);
      const deferredUntil = earliestDeferredUntil(get(rateLimitsData), scope);
      scheduleRetry(get(rateLimitsData), scope);
      rateLimitsRequestState.set({
        loading: false,
        loaded: true,
        error: formatFetchError(error),
        deferredUntil,
      });
    } finally {
      inflightFetch = null;
    }
  })();

  return inflightFetch;
}
