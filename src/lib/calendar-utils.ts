export const INTENSITY_OPACITY = [0, 0.15, 0.40, 0.65, 0.90];

export function intensityLevel(cost: number, maxCost: number): number {
  if (maxCost === 0 || cost === 0) return 0;
  const ratio = cost / maxCost;
  if (ratio <= 0.25) return 1;
  if (ratio <= 0.50) return 2;
  if (ratio <= 0.75) return 3;
  return 4;
}

export function computeEarned(totalCost: number, planCost: number): number | null {
  if (planCost <= 0) return null;
  return totalCost - planCost;
}

export function heatmapColor(
  level: number,
  brandTheming: boolean,
  provider: "claude" | "codex" | "all" | string,
): string {
  if (level === 0) return "var(--surface-2)";
  const opacity = INTENSITY_OPACITY[level];
  if (brandTheming && provider === "claude") return `rgba(196, 112, 75, ${opacity})`;
  if (brandTheming && provider === "codex") return `rgba(74, 123, 157, ${opacity})`;
  return `rgba(77, 175, 74, ${opacity})`;
}
