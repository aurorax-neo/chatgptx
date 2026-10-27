import { modelSlugFromRowName, suffixWindowString } from "./model-metadata.ts";
import { modelWindowRowsFromProfile, serializeModelWindowRows } from "./model-windows.ts";

export type LimitPreset = {
  label: string;
  tokens: number;
  value: string;
};

/// 上下文窗口常用档位预设（参考 pi-desktop Issue #202 与市场主流模型尺寸）
export const CONTEXT_WINDOW_PRESETS: readonly LimitPreset[] = [
  { label: "128k", tokens: 128_000, value: "128k" },
  { label: "200k", tokens: 200_000, value: "200k" },
  { label: "256k", tokens: 256_000, value: "256k" },
  { label: "312k", tokens: 312_000, value: "312k" },
  { label: "500k", tokens: 500_000, value: "500k" },
  { label: "1M", tokens: 1_000_000, value: "1M" },
  { label: "2M", tokens: 2_000_000, value: "2M" },
];

/// 把各种格式的窗口输入（128k / 128000 / 1M / 1000000 等）换算为 token 整数。
export function parseWindowToTokens(value: string | number | null | undefined): number | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "number") {
    return Number.isFinite(value) && value > 0 ? Math.round(value) : null;
  }
  const trimmed = value.trim();
  if (!trimmed) return null;
  const match = trimmed.match(/^(\d+(?:\.\d+)?)\s*([KkMm])?$/);
  if (!match) return null;
  const num = parseFloat(match[1]);
  if (!Number.isFinite(num) || num <= 0) return null;
  const unit = match[2]?.toLowerCase();
  const multiplier = unit === "m" ? 1_000_000 : unit === "k" ? 1_000 : 1;
  return Math.round(num * multiplier);
}

/// 将 token 数转换为用户友好的紧凑窗口字符串（如 128_000 -> "128k", 1_000_000 -> "1M"）。
export function formatTokensToWindowString(tokens: number | string | null | undefined): string {
  const count = typeof tokens === "number" ? tokens : parseWindowToTokens(tokens);
  if (!count || count <= 0) return "";
  if (count >= 1_000_000 && count % 100_000 === 0) {
    const m = count / 1_000_000;
    return `${m}M`;
  }
  if (count === 1_048_576) {
    return "1M";
  }
  if (count === 2_097_152) {
    return "2M";
  }
  if (count >= 1_000 && count % 1_000 === 0) {
    return `${count / 1_000}k`;
  }
  return String(count);
}

/// 检查当前窗口值与预设档位的匹配索引，未匹配返回 -1。
export function matchPresetIndex(
  presets: readonly LimitPreset[],
  value: string | number | null | undefined,
): number {
  const tokens = parseWindowToTokens(value);
  if (tokens === null) return -1;
  // 1048576 等二进制 1M 兼容归入 1M 预设
  if (tokens === 1_048_576) {
    const mIndex = presets.findIndex((p) => p.tokens === 1_000_000);
    if (mIndex >= 0) return mIndex;
  }
  if (tokens === 2_097_152) {
    const mIndex = presets.findIndex((p) => p.tokens === 2_000_000);
    if (mIndex >= 0) return mIndex;
  }
  return presets.findIndex((preset) => preset.tokens === tokens);
}

/// 剥离日期版本后缀（例如 gpt-4o-2024-08-06 -> gpt-4o, claude-3-5-sonnet-20241022 -> claude-3-5-sonnet）。
export function stripReleaseSuffix(value: string): string {
  const trimmed = value.trim();
  // 剥离 -latest, _latest, :latest
  const latestMatch = trimmed.match(/^(.+?)[-_:]latest$/i);
  if (latestMatch) return stripReleaseSuffix(latestMatch[1]);
  // 剥离 :tag 形式的日期 (如 :20240806, :2024-08-06)
  const colonTagMatch = trimmed.match(/^(.+?):202\d[-_.]?(?:0[1-9]|1[0-2])[-_.]?(?:0[1-9]|[12]\d|3[01])$/i);
  if (colonTagMatch) return colonTagMatch[1];
  // -YYYY-MM-DD 或 -YYYY_MM_DD 或 -YYYY.MM.DD
  const dateSepMatch = trimmed.match(/^(.+?)[-_.]202\d[-_.](?:0[1-9]|1[0-2])[-_.](?:0[1-9]|[12]\d|3[01])$/i);
  if (dateSepMatch) return dateSepMatch[1];
  // -YYYYMMDD (如 -20240806)
  const compactDateMatch = trimmed.match(/^(.+?)[-_]202\d(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])$/i);
  if (compactDateMatch) return compactDateMatch[1];
  // -MMDD (如 -0731, -0813)
  const shortDateMatch = trimmed.match(/^(.+?)[-_](?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])$/i);
  if (shortDateMatch) return shortDateMatch[1];
  return trimmed;
}

/// 剥离路径/供应商前缀（如 openai/gpt-4o -> gpt-4o, deepseek-ai/DeepSeek-V3 -> DeepSeek-V3）。
export function pathLeaf(value: string): string {
  const trimmed = value.trim();
  const idx = trimmed.lastIndexOf("/");
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}

/// 生成点与横杠互换的版本号别名候选（如 claude-3.5-sonnet <-> claude-3-5-sonnet）
export function notationCandidates(value: string): string[] {
  const trimmed = value.trim();
  const set = new Set<string>([trimmed]);
  // 3.5 -> 3-5, 3.7 -> 3-7, 4.1 -> 4-1, 5.3 -> 5-3
  const dashed = trimmed.replace(/(\d+)\.(\d+)/g, "$1-$2");
  set.add(dashed);
  // 3-5 -> 3.5
  const dotted = trimmed.replace(/(\d+)-(\d+)/g, "$1.$2");
  set.add(dotted);
  return Array.from(set);
}

/// 从模型名称中识别内嵌的上下文窗口尺寸（如 llama-3.1-8b-instruct-128k, moonshot-v1-32k, glm-4-9b-chat-1m 等）。
export function inferContextWindowFromModelName(modelName: string): string | null {
  const leaf = pathLeaf(modelSlugFromRowName(modelName));
  // 匹配末尾或连字符间的上下文尺寸标识，如 -128k, -1m, -32k, -200k, -256k, -512k, -1000k
  const markerMatch = leaf.match(/[-_:](\d+)(k|m)$/i);
  if (markerMatch) {
    const count = parseInt(markerMatch[1], 10);
    const unit = markerMatch[2].toLowerCase();
    if (count > 0 && count <= 10000) {
      return `${count}${unit === "m" ? "M" : "k"}`;
    }
  }
  return null;
}

/// 常见公认大模型的标准上下文窗口常数表（针对主流原厂与广泛兼容网关模型）。
const KNOWN_MODEL_CONTEXT_WINDOWS: ReadonlyArray<[RegExp, string]> = [
  // 优先匹配长上下文专有模型标识
  [/(?:qwen-long|glm-4-long)/i, "1M"],
  [/^moonshot-v1-8k/i, "8k"],
  [/^moonshot-v1-32k/i, "32k"],
  [/^moonshot-v1-128k/i, "128k"],
  // Claude 3 / 3.5 / 3.7 系列均标配 200k
  [/^claude-(?:3|3[-.]5|3[-.]7|4)/i, "200k"],
  // OpenAI o1 / o3 系列（o1, o1-preview 128k/200k, o3-mini 200k）
  [/^o1-(?:mini|preview)/i, "128k"],
  [/^o[13](?:-mini|-preview)?$/i, "200k"],
  [/^gpt-4[-.]1/i, "1M"],
  [/^gpt-5/i, "272k"],
  [/^gpt-4o/i, "128k"],
  [/^gpt-4-turbo/i, "128k"],
  [/^chatgpt-4o/i, "128k"],
  // DeepSeek 官方标准窗口为 128k（含 deepseek-chat, deepseek-reasoner, deepseek-v3, deepseek-r1 等）
  [/^deepseek-(?:chat|reasoner|v3|r1|v4)/i, "128k"],
  // Google Gemini 官方标准窗口 Flash 为 1M，Pro 为 2M
  [/^gemini-.*pro/i, "2M"],
  [/^gemini-(?:1[-.]5|2[-.]0|3[-.]\d)/i, "1M"],
  // xAI Grok 系列
  [/^grok-4/i, "500k"],
  [/^grok-3/i, "1M"],
  [/^grok-(?:2|beta)/i, "128k"],
  // 智谱 GLM-4 / GLM-5 系列
  [/^glm-(?:4|5)(?:-plus|-air|-flash|-long)?/i, "128k"],
  // 阿里通义千问 Qwen 2.5 / 3 系列（turbo/plus 为 128k，max 为 32k/128k）
  [/^qwen.*-max/i, "32k"],
  [/^qwen/i, "128k"],
  // Meta Llama 3 / 3.1 / 3.2 / 3.3
  [/^llama-3/i, "128k"],
  // Mistral Large / Codestral
  [/^(?:mistral-large|codestral)/i, "128k"],
  [/^mistral/i, "32k"],
  // MiniMax / Kimi
  [/^kimi/i, "200k"],
  [/^minimax/i, "256k"],
];

export type BuiltinIndexLike = Map<string, { context_window?: unknown }>;

/// 智能匹配模型的上下文窗口大小：
/// 1. 若模型名自身带有 `[1M]` 语法后缀，直接返回该后缀尺寸；
/// 2. 查询内置索引（精准、剥离路径前缀、剥离发布日期后缀、忽略大小写匹配）；
/// 3. 若内置未命中，查询知名主流模型标准规格表；
/// 4. 若未命中，尝试从模型名自身内嵌尺寸（如 `*-128k`）解析；
/// 5. 均未命中返回 null。
export function matchContextWindowForModel(
  modelName: string,
  builtinIndex?: BuiltinIndexLike,
): string | null {
  const raw = modelName.trim();
  if (!raw) return null;

  // 1. [1M] 显式后缀
  const suffix = suffixWindowString(raw);
  if (suffix) return formatTokensToWindowString(suffix);

  const cleanSlug = modelSlugFromRowName(raw);
  const leaf = pathLeaf(cleanSlug);
  const strippedLeaf = stripReleaseSuffix(leaf);

  // 2. 内置元数据索引查询
  if (builtinIndex && builtinIndex.size > 0) {
    const candidateKeys = new Set<string>();
    for (const base of [cleanSlug, leaf, strippedLeaf]) {
      for (const variant of notationCandidates(base)) {
        candidateKeys.add(variant.toLowerCase());
      }
    }
    const candidates = Array.from(candidateKeys);
    for (const key of candidates) {
      const hit = builtinIndex.get(key);
      if (
        hit
        && typeof hit.context_window === "number"
        && Number.isFinite(hit.context_window)
        && hit.context_window > 0
      ) {
        return formatTokensToWindowString(hit.context_window);
      }
    }
  }

  // 3. 常见公认大模型规则表匹配
  const allVariants = [
    ...notationCandidates(leaf),
    ...notationCandidates(strippedLeaf),
    ...notationCandidates(cleanSlug),
  ];
  for (const [pattern, window] of KNOWN_MODEL_CONTEXT_WINDOWS) {
    if (allVariants.some((v) => pattern.test(v))) {
      return window;
    }
  }

  // 4. 从模型名自身提取内嵌尺寸（如 *-128k）
  const inferred = inferContextWindowFromModelName(raw);
  if (inferred) return inferred;

  return null;
}

export type ModelWindowRowLike = {
  model: string;
  window: string;
};

/// 批量为模型列表匹配填充上下文窗口大小。
export function autoFillModelWindowRows<T extends ModelWindowRowLike>(
  rows: T[],
  builtinIndex?: BuiltinIndexLike,
  options: { overwrite?: boolean } = {},
): { rows: T[]; filledCount: number } {
  let filledCount = 0;
  const next = rows.map((row) => {
    const hasWindow = Boolean(row.window.trim());
    if (hasWindow && !options.overwrite) {
      return row;
    }
    const matched = matchContextWindowForModel(row.model, builtinIndex);
    if (!matched) {
      return row;
    }
    if (matched !== row.window.trim()) {
      filledCount += 1;
      return { ...row, window: matched };
    }
    return row;
  });
  return { rows: next, filledCount };
}

export type ProfileLike = {
  id: string;
  name: string;
  modelList: string;
  modelWindows: string;
  modelVlm?: string;
  modelAutoCompact?: string;
};

/// 为所有供应商配置批量智能匹配并填充上下文窗口大小
export function autoFillAllProfilesContextWindows<T extends ProfileLike>(
  profiles: T[],
  builtinIndex?: BuiltinIndexLike,
  options: { overwrite?: boolean } = {},
): { profiles: T[]; updatedProfilesCount: number; updatedModelsCount: number } {
  let updatedProfilesCount = 0;
  let updatedModelsCount = 0;

  const nextProfiles = profiles.map((profile) => {
    const rows = modelWindowRowsFromProfile(
      profile.modelList,
      profile.modelWindows,
      profile.modelVlm,
      profile.modelAutoCompact,
    );
    if (rows.length === 0) return profile;

    const { rows: nextRows, filledCount } = autoFillModelWindowRows(rows, builtinIndex, options);
    if (filledCount === 0) return profile;

    updatedProfilesCount += 1;
    updatedModelsCount += filledCount;

    const serialized = serializeModelWindowRows(nextRows);
    return {
      ...profile,
      modelList: serialized.modelList,
      modelWindows: serialized.modelWindows,
      modelVlm: serialized.modelVlm,
      modelAutoCompact: serialized.modelAutoCompact,
    };
  });

  return { profiles: nextProfiles, updatedProfilesCount, updatedModelsCount };
}
