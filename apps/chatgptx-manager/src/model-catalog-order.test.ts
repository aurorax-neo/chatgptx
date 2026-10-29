import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const fragment = (name: string) => readFileSync(
  new URL(`../../../assets/inject/renderer-inject/${name}`, import.meta.url),
  "utf8",
);

function orderRuntime(catalogModels: string[], customNames: string[]) {
  const source = fragment("70-model-catalog.js");
  const start = source.indexOf("  function modelItemName(");
  const end = source.indexOf("\n  function patchModelNameArray(", start);
  assert.ok(start >= 0 && end > start, "模型目录顺序函数必须位于 patchModelNameArray 之前");
  const body = source.slice(start, end);
  const factory = new Function(
    "uniqueValues",
    "codexModelCatalog",
    "chatgptxModelNames",
    `${body}\nreturn { reorderModelCollectionByCatalog, reorderModelSetByCatalog, chatgptxCatalogOrderedModelNames };`,
  );
  return factory(
    (values: unknown[]) => Array.from(new Set(values.filter(
      (value): value is string => typeof value === "string" && value.trim().length > 0,
    ))),
    { models: catalogModels },
    () => customNames,
  ) as {
    reorderModelCollectionByCatalog: (models: Array<string | { model: string }>) => boolean;
    reorderModelSetByCatalog: (models: Set<string>) => boolean;
    chatgptxCatalogOrderedModelNames: () => string[];
  };
}

test("Codex 模型对象按 ChatGPTX 模型列表顺序重排", () => {
  const runtime = orderRuntime(["a", "b", "c"], ["c", "a", "b"]);
  const models = [{ model: "c" }, { model: "a" }, { model: "b" }];
  assert.equal(runtime.reorderModelCollectionByCatalog(models), true);
  assert.deepEqual(models.map((item) => item.model), ["a", "b", "c"]);
  assert.equal(runtime.reorderModelCollectionByCatalog(models), false);
});

test("Codex 模型字符串数组同样按 ChatGPTX 顺序重排", () => {
  const runtime = orderRuntime(["a", "b", "c"], ["a", "b", "c"]);
  const models = ["c", "a", "b"];
  assert.equal(runtime.reorderModelCollectionByCatalog(models), true);
  assert.deepEqual(models, ["a", "b", "c"]);
});

test("目录外的原生模型存在时不重排，避免误动官方列表", () => {
  const runtime = orderRuntime(["a", "b"], ["a", "b"]);
  const models = ["b", "official", "a"];
  assert.equal(runtime.reorderModelCollectionByCatalog(models), false);
  assert.deepEqual(models, ["b", "official", "a"]);
});

test("Codex 模型 Set 也按 ChatGPTX 顺序重排", () => {
  const runtime = orderRuntime(["a", "b", "c"], ["a", "b", "c"]);
  const models = new Set(["c", "a", "b"]);
  assert.equal(runtime.reorderModelSetByCatalog(models), true);
  assert.deepEqual([...models], ["a", "b", "c"]);
  assert.equal(runtime.reorderModelSetByCatalog(models), false);
});

test("配置模型只补到目录末尾，不抢到列表第一位", () => {
  const runtime = orderRuntime(["a", "c"], ["default", "a", "c"]);
  assert.deepEqual(runtime.chatgptxCatalogOrderedModelNames(), ["a", "c", "default"]);
});

test("对象数组和字符串数组两条补丁路径都接入了顺序同步", () => {
  const source = fragment("70-model-catalog.js");
  const calls = source.match(/if \(allowed && reorderModelCollectionByCatalog\(models\)\) changed = true;/g) ?? [];
  assert.equal(calls.length, 2);
  const setCalls = source.match(/if \(allowed && reorderModelSetByCatalog\(value\.availableModels\)\) changed = true;/g) ?? [];
  const snakeSetCalls = source.match(/if \(allowed && reorderModelSetByCatalog\(value\.available_models\)\) changed = true;/g) ?? [];
  assert.equal(setCalls.length, 1);
  assert.equal(snakeSetCalls.length, 1);
});

test("配置模型位于模型列表下方并使用下拉选择", () => {
  const app = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
  const list = app.indexOf('<strong>{t("模型列表")}</strong>');
  const config = app.indexOf('label={t("配置模型")}');
  assert.ok(list >= 0 && config > list, "配置模型必须排在模型列表之后");
  const block = app.slice(config, app.indexOf("</Field>", config));
  assert.match(block, /<AppSelect/);
  assert.doesNotMatch(block, /<Input/);
});

test("清空模型同时清空模型列表与配置模型", () => {
  const app = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
  const title = app.indexOf('title={t("清空模型")}');
  assert.ok(title > 0);
  const block = app.slice(app.lastIndexOf("<Button", title), title);
  assert.match(block, /setModelWindowRows\(\[\]\)/);
  assert.match(block, /updateDraft\(\{ model: "" \}\)/);
});
