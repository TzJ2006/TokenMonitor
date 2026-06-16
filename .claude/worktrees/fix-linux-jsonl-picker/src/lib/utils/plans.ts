import { ALL_USAGE_PROVIDER_ID, getUsageProviderPlanTierCost } from "../providerMetadata.js";
import type { UsageProvider } from "../types/index.js";

/**
 * Returns the monthly subscription cost (USD) for a given plan tier and provider.
 * Returns 0 if the tier is unknown or the provider is "all".
 */
export function planTierCost(tier: string | null, provider: UsageProvider): number {
  if (!tier || provider === ALL_USAGE_PROVIDER_ID) return 0;
  return getUsageProviderPlanTierCost(provider, tier);
}
