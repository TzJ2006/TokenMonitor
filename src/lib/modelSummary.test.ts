import { describe, expect, it } from "vitest";
import { summarizeModelRows } from "./modelSummary.js";
import type { ModelSummary } from "./types/index.js";

function model(
  display_name: string,
  model_key: string,
  cost: number,
  tokens: number,
): ModelSummary {
  return { display_name, model_key, cost, tokens };
}

describe("summarizeModelRows", () => {
  it("returns all rows when the list is already short", () => {
    const rows = summarizeModelRows([
      model("Opus", "opus", 10, 100),
      model("Sonnet", "sonnet", 5, 50),
    ]);

    expect(rows).toHaveLength(2);
    expect(rows[1].display_name).toBe("Sonnet");
  });

  it("returns all rows even when many models are present", () => {
    const rows = summarizeModelRows([
      model("Opus", "opus", 10, 100),
      model("Sonnet", "sonnet", 9, 90),
      model("Haiku", "haiku", 8, 80),
      model("GPT-5.4", "gpt54", 7, 70),
      model("GPT-5.3", "gpt53", 6, 60),
      model("GPT-5.2", "gpt52", 5, 50),
      model("o3", "o3", 4, 40),
    ]);

    expect(rows).toHaveLength(7);
    expect(rows.at(-1)).toEqual(model("o3", "o3", 4, 40));
  });
});
