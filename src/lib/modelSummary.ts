import type { ModelSummary } from "./types/index.js";

export interface ModelListRow {
  display_name: string;
  model_key: string;
  cost: number;
  tokens: number;
  isAggregate?: boolean;
}

export function summarizeModelRows(
  models: ModelSummary[],
  _maxVisibleModels = 5,
): ModelListRow[] {
  return models;
}
