import { describe, expect, it } from "vitest";
import { getUsageProviderPlanTierCost } from "./providerMetadata.js";

describe("getUsageProviderPlanTierCost", () => {
  it("returns the correct cost for each Claude plan tier", () => {
    expect(getUsageProviderPlanTierCost("claude", "Pro")).toBe(20);
    expect(getUsageProviderPlanTierCost("claude", "Max 5x")).toBe(100);
    expect(getUsageProviderPlanTierCost("claude", "Max 20x")).toBe(200);
    expect(getUsageProviderPlanTierCost("claude", "Free")).toBe(0);
  });

  it("returns the correct cost for each Codex plan tier", () => {
    expect(getUsageProviderPlanTierCost("codex", "Plus")).toBe(20);
    expect(getUsageProviderPlanTierCost("codex", "Pro")).toBe(200);
    expect(getUsageProviderPlanTierCost("codex", "Free")).toBe(0);
  });

  it("returns 0 for null or unknown tiers", () => {
    expect(getUsageProviderPlanTierCost("claude", null)).toBe(0);
    expect(getUsageProviderPlanTierCost("codex", null)).toBe(0);
    expect(getUsageProviderPlanTierCost("claude", "Enterprise")).toBe(0);
    expect(getUsageProviderPlanTierCost("codex", "Enterprise")).toBe(0);
  });

  it("returns 0 when provider is 'all'", () => {
    expect(getUsageProviderPlanTierCost("all", "Pro")).toBe(0);
    expect(getUsageProviderPlanTierCost("all", "Max 5x")).toBe(0);
  });
});
