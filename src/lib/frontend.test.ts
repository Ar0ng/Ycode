/// <reference types="node" />
import { describe, expect, it } from "vitest";
import { clampEffort, defaultModel, supportedEfforts } from "./effort";
import { dictionaries, translate } from "./i18n";
import { fmt, hitRate } from "../components/UsageLine";
import type { ModelCaps } from "./types";
import { isHidden, isLegacy } from "./models";
import { displayVersion } from "./format";
import { pickGreeting, timeOfDay } from "./greeting";

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

describe("legacy models", () => {
  const m = (provider: ModelCaps["provider"], id: string, note?: string) => caps({ provider, id, note });
  it("hides older generations per provider", () => {
    for (const id of ["gpt-5.5-mini", "gpt-5.5-nano", "gpt-5.4", "gpt-5", "gpt-4.1", "gpt-4o", "o3", "o4-mini", "chatgpt-4o-latest"]) expect(isLegacy(m("openai", id))).toBe(true);
    for (const id of ["gpt-5.5", "gpt-5.6", "gpt-6-astra", "gpt-6.1-terra", "gpt-6-luna"]) expect(isLegacy(m("openai", id))).toBe(false);
    for (const id of ["claude-opus-4-8", "claude-sonnet-4-5-20250929", "claude-haiku-4-5", "claude-3-5-sonnet-20241022"]) expect(isLegacy(m("anthropic", id))).toBe(true);
    for (const id of ["claude-opus-5-5", "claude-sonnet-5-5", "claude-fable-5-1", "claude-opus-5"]) expect(isLegacy(m("anthropic", id))).toBe(false);
    expect(isLegacy(m("anthropic", "claude-x-9", "deprecated"))).toBe(true);
    for (const id of ["deepseek-chat", "deepseek-reasoner", "deepseek-v4-flash"]) expect(isLegacy(m("deepseek", id))).toBe(true);
    for (const id of ["deepseek-flash", "deepseek-v4-pro"]) expect(isLegacy(m("deepseek", id))).toBe(false);
  });
  it("combines the auto rule with hand-hidden models", () => {
    const s = { hide_legacy_models: false, hidden_models: ["openai:gpt-6-luna"] };
    expect(isHidden(m("openai", "gpt-6-luna"), s)).toBe(true);
    expect(isHidden(m("openai", "gpt-5.5-mini"), s)).toBe(false);
    expect(isHidden(m("openai", "gpt-5.5-mini"), { ...s, hide_legacy_models: true })).toBe(true);
  });
});

describe("version display", () => {
  it("shows versions with a capital V", () => {
    expect(displayVersion("0.1.4")).toBe("V0.1.4");
    expect(displayVersion("v0.1.4")).toBe("V0.1.4");
    expect(displayVersion("V1.2.0")).toBe("V1.2.0");
    expect(displayVersion(null)).toBe("");
  });
});

describe("greeting", () => {
  const at = (h: number) => new Date(2026, 10, 6, h, 0, 0);
  it("buckets the time of day", () => {
    expect([6, 12, 15, 20, 2].map(timeOfDay)).toEqual(["morning", "noon", "afternoon", "evening", "night"]);
  });
  it("picks only applicable variants", () => {
    const all = new Set([0, 0.4, 0.7, 0.99].map((r) => pickGreeting({ lang: "zh-CN", name: "Ar0ng", workspace: "E:/proj/demo-app", now: at(9), random: r })));
    expect(all).toEqual(new Set(["今天想做点什么？", "早上好，Ar0ng", "我们应该在 demo-app 中做些什么？"]));
    // No name and no folder: only the generic question.
    expect(pickGreeting({ lang: "zh-CN", name: " ", workspace: null, now: at(20), random: 0.99 })).toBe("今天想做点什么？");
    expect(pickGreeting({ lang: "en-US", name: "Ar0ng", workspace: null, now: at(20), random: 0.99 })).toBe("Good evening, Ar0ng");
  });
});

describe("store selectors", () => {
  it("never derive new arrays/objects inside useStore selectors (infinite re-render → blank window)", async () => {
    const { readdirSync, readFileSync, statSync } = await import("node:fs");
    const { join } = await import("node:path");
    const files: string[] = [];
    const walk = (d: string) => {
      for (const f of readdirSync(d)) {
        const p = join(d, f);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.tsx?$/.test(f) && !f.endsWith(".test.ts")) files.push(p);
      }
    };
    walk("src");
    const bad = /useStore\(\(\w+\)\s*=>\s*(?:[^)]*\.(?:filter|map|slice|concat|reduce)\(|\{|\[|\(\{)/;
    const offenders = files.flatMap((f) =>
      readFileSync(f, "utf8")
        .split("\n")
        .map((l, i) => [l, i + 1] as const)
        .filter(([l]) => bad.test(l))
        .map(([, n]) => `${f}:${n}`),
    );
    expect(offenders).toEqual([]);
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

describe("context estimate", () => {
  it("uses recent request input and output, and never reports a negative remainder", async () => {
    const { contextEstimate } = await import("../components/UsageSummary");
    const usage = { input_tokens: 80, output_tokens: 10, cached_input_tokens: 40 };
    expect(contextEstimate(usage, 100)).toEqual({ used: 90, remaining: 10, window: 100 });
    expect(contextEstimate(usage, 50)?.remaining).toBe(0);
    expect(contextEstimate(null, 100)).toBeNull();
  });
});
