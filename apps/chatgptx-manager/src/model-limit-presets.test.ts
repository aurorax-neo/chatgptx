import assert from "node:assert/strict";
import test from "node:test";
import {
  CONTEXT_WINDOW_PRESETS,
  parseWindowToTokens,
  formatTokensToWindowString,
  matchPresetIndex,
  stripReleaseSuffix,
  pathLeaf,
  inferContextWindowFromModelName,
  matchContextWindowForModel,
  autoFillModelWindowRows,
  autoFillAllProfilesContextWindows,
} from "./model-limit-presets.ts";

test("预设梯子包含常用的上下文窗口尺寸", () => {
  assert.deepEqual(
    CONTEXT_WINDOW_PRESETS.map((p) => p.tokens),
    [128_000, 200_000, 256_000, 312_000, 500_000, 1_000_000, 2_000_000],
  );
  assert.deepEqual(
    CONTEXT_WINDOW_PRESETS.map((p) => p.label),
    ["128k", "200k", "256k", "312k", "500k", "1M", "2M"],
  );
});

test("parseWindowToTokens 支持多种单位和整数", () => {
  assert.equal(parseWindowToTokens("128k"), 128_000);
  assert.equal(parseWindowToTokens("128K"), 128_000);
  assert.equal(parseWindowToTokens("1M"), 1_000_000);
  assert.equal(parseWindowToTokens("1m"), 1_000_000);
  assert.equal(parseWindowToTokens("200000"), 200_000);
  assert.equal(parseWindowToTokens(256_000), 256_000);
  assert.equal(parseWindowToTokens(""), null);
  assert.equal(parseWindowToTokens("invalid"), null);
});

test("formatTokensToWindowString 格式化为整洁的显示字符串", () => {
  assert.equal(formatTokensToWindowString(128_000), "128k");
  assert.equal(formatTokensToWindowString(200_000), "200k");
  assert.equal(formatTokensToWindowString(1_000_000), "1M");
  assert.equal(formatTokensToWindowString(1_048_576), "1M");
  assert.equal(formatTokensToWindowString(2_000_000), "2M");
  assert.equal(formatTokensToWindowString(272_000), "272k");
  assert.equal(formatTokensToWindowString(8_192), "8192");
});

test("matchPresetIndex 能够匹配数值与字符串档位并高亮", () => {
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, "128k"), 0);
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, 128_000), 0);
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, "200k"), 1);
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, "1M"), 5);
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, 1_048_576), 5); // 兼容二进制 1M
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, "2M"), 6);
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, "300k"), -1);
  assert.equal(matchPresetIndex(CONTEXT_WINDOW_PRESETS, ""), -1);
});

test("stripReleaseSuffix 剥离日期版本快照后缀", () => {
  assert.equal(stripReleaseSuffix("gpt-4o-2024-08-06"), "gpt-4o");
  assert.equal(stripReleaseSuffix("claude-3-5-sonnet-20241022"), "claude-3-5-sonnet");
  assert.equal(stripReleaseSuffix("deepseek-v4-flash-0731"), "deepseek-v4-flash");
  assert.equal(stripReleaseSuffix("model-v2_2025_01_15"), "model-v2");
  assert.equal(stripReleaseSuffix("gpt-4o"), "gpt-4o");
});

test("pathLeaf 提取模型路由叶子", () => {
  assert.equal(pathLeaf("openai/gpt-4o"), "gpt-4o");
  assert.equal(pathLeaf("anthropic/claude-3-5-sonnet"), "claude-3-5-sonnet");
  assert.equal(pathLeaf("deepseek-chat"), "deepseek-chat");
});

test("inferContextWindowFromModelName 从名称中提取尺寸", () => {
  assert.equal(inferContextWindowFromModelName("moonshot-v1-32k"), "32k");
  assert.equal(inferContextWindowFromModelName("llama-3.1-8b-instruct-128k"), "128k");
  assert.equal(inferContextWindowFromModelName("glm-4-9b-chat-1m"), "1M");
  assert.equal(inferContextWindowFromModelName("internlm2.5-200k"), "200k");
  assert.equal(inferContextWindowFromModelName("gpt-4o"), null);
});

test("matchContextWindowForModel 智能匹配主流大模型及内置索引", () => {
  // 1. 显式 [1M] 后缀
  assert.equal(matchContextWindowForModel("custom-model[1M]"), "1M");
  assert.equal(matchContextWindowForModel("my-model[256k]"), "256k");

  // 2. 内置索引匹配（模拟 builtinIndex）
  const fakeIndex = new Map([
    ["custom-glm", { context_window: 128_000 }],
    ["company-model", { context_window: 1_000_000 }],
  ]);
  assert.equal(matchContextWindowForModel("custom-glm", fakeIndex), "128k");
  assert.equal(matchContextWindowForModel("prefix/company-model-20240806", fakeIndex), "1M");

  // 3. 常见公认模型匹配
  assert.equal(matchContextWindowForModel("claude-3-5-sonnet"), "200k");
  assert.equal(matchContextWindowForModel("anthropic/claude-3-7-sonnet-20250219"), "200k");
  assert.equal(matchContextWindowForModel("openai/gpt-4o-2024-08-06"), "128k");
  assert.equal(matchContextWindowForModel("o1-mini"), "128k");
  assert.equal(matchContextWindowForModel("o3-mini"), "200k");
  assert.equal(matchContextWindowForModel("deepseek-chat"), "128k");
  assert.equal(matchContextWindowForModel("deepseek-reasoner"), "128k");
  assert.equal(matchContextWindowForModel("gemini-1.5-pro"), "2M");
  assert.equal(matchContextWindowForModel("gemini-2.0-flash"), "1M");
  assert.equal(matchContextWindowForModel("qwen-turbo"), "128k");
  assert.equal(matchContextWindowForModel("qwen-max"), "32k");
});

test("autoFillModelWindowRows 批量填充模型行", () => {
  const rows = [
    { model: "claude-3-5-sonnet", window: "" },
    { model: "gpt-4o", window: "64k" }, // 已有值，默认不覆盖
    { model: "deepseek-chat", window: "" },
    { model: "unknown-model-xyz", window: "" }, // 无法匹配
  ];
  const result = autoFillModelWindowRows(rows);
  assert.equal(result.filledCount, 2);
  assert.equal(result.rows[0].window, "200k");
  assert.equal(result.rows[1].window, "64k");
  assert.equal(result.rows[2].window, "128k");
  assert.equal(result.rows[3].window, "");

  // 覆盖模式
  const overwriteResult = autoFillModelWindowRows(rows, undefined, { overwrite: true });
  assert.equal(overwriteResult.filledCount, 3);
  assert.equal(overwriteResult.rows[1].window, "128k");
});

test("autoFillAllProfilesContextWindows 批量为多个供应商更新上下文", () => {
  const profiles = [
    {
      id: "p1",
      name: "Provider 1",
      modelList: "claude-3-5-sonnet\ngpt-4o",
      modelWindows: JSON.stringify({}),
    },
    {
      id: "p2",
      name: "Provider 2",
      modelList: "deepseek-chat",
      modelWindows: JSON.stringify({ "deepseek-chat": "64k" }), // 已有值
    },
  ];
  const result = autoFillAllProfilesContextWindows(profiles);
  assert.equal(result.updatedProfilesCount, 1);
  assert.equal(result.updatedModelsCount, 2);

  const windows1 = JSON.parse(result.profiles[0].modelWindows);
  assert.equal(windows1["claude-3-5-sonnet"], "200k");
  assert.equal(windows1["gpt-4o"], "128k");

  // 覆盖测试
  const overwrite = autoFillAllProfilesContextWindows(profiles, undefined, { overwrite: true });
  assert.equal(overwrite.updatedProfilesCount, 2);
  assert.equal(overwrite.updatedModelsCount, 3);
  const windows2 = JSON.parse(overwrite.profiles[1].modelWindows);
  assert.equal(windows2["deepseek-chat"], "128k");
});
