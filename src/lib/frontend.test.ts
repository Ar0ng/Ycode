import { describe, expect, it } from "vitest";
import { clampEffort, defaultModel, supportedEfforts } from "./effort";
import { dictionaries, translate } from "./i18n";
import { fmt, hitRate } from "../components/UsageLine";
import type { ModelCaps } from "./types";

const caps = (over: Partial<ModelCaps>): ModelCaps => ({
  provider: "deepseek",
  id: "deepseek-flash",
  display_name: "Flash",
  efforts: ["none", "low", "high", "max"],
  default_effort: "high",
  effort_update: "param",
  tools: true,
  vision: true,
  context_window: 1_000_000,
  max_output: 300_000,
  min_cache_tokens: 0,
  recommended: ["office"],
  adaptive_thinking: false,
  server_fallbacks: false,
  source: "builtin",
  ...over,
});

describe("effort", () => {
  it("only offers supported levels in canonical order", () => {
    expect(supportedEfforts(caps({ efforts: ["max", "low", "high"] }))).toEqual(["low", "high", "max"]);
    expect(supportedEfforts(undefined)).toEqual([]);
  });
  it("clamps unsupported values to the model default", () => {
    const c = caps({});
    expect(clampEffort(c, "medium")).toBe("high");
    expect(clampEffort(c, "max")).toBe("max");
    expect(clampEffort(caps({ efforts: [] }), "high")).toBeNull();
  });
  it("picks a recommended default model per mode", () => {
    const models = [caps({ id: "a", recommended: ["coding"] }), caps({ id: "b", recommended: ["office"] })];
    expect(defaultModel(models, "deepseek", "office")).toBe("b");
    expect(defaultModel(models, "deepseek", "coding")).toBe("a");
  });
});

describe("i18n", () => {
  it("has identical keys in both languages and no empty strings", () => {
    const en = Object.keys(dictionaries["en-US"]).sort();
    const zh = Object.keys(dictionaries["zh-CN"]).sort();
    expect(zh).toEqual(en);
    for (const v of [...Object.values(dictionaries["en-US"]), ...Object.values(dictionaries["zh-CN"])]) expect(v.length).toBeGreaterThan(0);
  });
  it("interpolates variables", () => {
    expect(translate("en-US", "provider.testOk", { n: 3 })).toBe("Connected · 3 models returned");
    expect(translate("zh-CN", "effort.active", { level: "高" })).toBe("当前生效：高");
  });
  it("covers every provider error kind", () => {
    const kinds = ["missing_key", "invalid_key", "insufficient_balance", "model_not_found", "model_no_access", "rate_limited", "overloaded", "timeout", "network", "stream_interrupted", "bad_request", "refused", "server", "unknown"];
    for (const k of kinds) expect(Object.keys(dictionaries["zh-CN"])).toContain(`err.${k}`);
  });
});

describe("usage", () => {
  it("reports N/A without a reliable denominator", () => {
    expect(hitRate({ input_tokens: 10, output_tokens: 1, cached_input_tokens: 5 })).toBeNull();
    expect(hitRate({ input_tokens: 1000, output_tokens: 1, cached_input_tokens: 768, eligible_input_tokens: 1000 })).toBeCloseTo(0.768);
  });
  it("formats token counts", () => {
    expect(fmt(950)).toBe("950");
    expect(fmt(1500)).toBe("1.5k");
    expect(fmt(2_500_000)).toBe("2.5M");
  });
});
