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
  maxVisibleModels = 5,
): ModelListRow[] {
  if (models.length <= maxVisibleModels) {
    return models;
  }

  const visible = models.slice(0, maxVisibleModels);
  const hidden = models.slice(maxVisibleModels);
  const hiddenCount = hidden.length;
  const hiddenCost = hidden.reduce((sum, model) => sum + model.cost, 0);
  const hiddenTokens = hidden.reduce((sum, model) => sum + model.tokens, 0);

  return [
    ...visible,
    {
      display_name: hiddenCount === 1 ? "1 more model" : `${hiddenCount} more models`,
      model_key: "other",
      cost: hiddenCost,
      tokens: hiddenTokens,
      isAggregate: true,
    },
  ];
}
