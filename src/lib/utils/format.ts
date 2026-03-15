// Approximate exchange rates from USD (updated 2025-03)
const CURRENCIES: Record<string, { symbol: string; rate: number }> = {
  USD: { symbol: "$", rate: 1 },
  EUR: { symbol: "€", rate: 0.92 },
  GBP: { symbol: "£", rate: 0.79 },
  JPY: { symbol: "¥", rate: 149.5 },
  CNY: { symbol: "¥", rate: 7.24 },
};

let activeCurrency = "USD";

export function setCurrency(currency: string) {
  activeCurrency = currency;
}

export function currencySymbol(): string {
  return CURRENCIES[activeCurrency]?.symbol ?? "$";
}

export function convertCost(value: number): number {
  const cur = CURRENCIES[activeCurrency] ?? CURRENCIES.USD;
  return value * cur.rate;
}

export function formatCost(value: number): string {
  const cur = CURRENCIES[activeCurrency] ?? CURRENCIES.USD;
  const converted = value * cur.rate;
  if (activeCurrency === "JPY") {
    return `${cur.symbol}${Math.round(converted)}`;
  }
  return `${cur.symbol}${converted.toFixed(2)}`;
}

export function formatTokens(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${Math.round(count / 1_000)}K`;
  return count.toString();
}

export function formatTimeAgo(isoString: string): string {
  const seconds = Math.floor((Date.now() - new Date(isoString).getTime()) / 1000);
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
}

export function modelColor(key: string): string {
  const colors: Record<string, string> = {
    opus: "var(--opus)",
    sonnet: "var(--sonnet)",
    haiku: "var(--haiku)",
    gpt54: "var(--gpt54)",
    gpt53: "var(--gpt53)",
    gpt52: "var(--gpt52)",
    o3: "var(--o3)",
    o3mini: "var(--o3mini)",
    o4mini: "var(--o4mini)",
    o1: "var(--o1)",
    o1mini: "var(--o1mini)",
    codex: "var(--codex)",
    unknown: "var(--t3)",
  };
  return colors[key] ?? colors.unknown;
}
