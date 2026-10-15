import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  defaultDreamSkinTheme,
  isDreamSkinDraftDirty,
  normalizeDreamSkinTheme,
  resolveDreamSkinStylePreset,
} from "./dream-skin.ts";

describe("dream skin theme helpers", () => {
  it("uses the Codex-Dream-Skin theme.json defaults", () => {
    const theme = defaultDreamSkinTheme();
    assert.equal(theme.schemaVersion, 1);
    assert.equal(theme.name, "Dream Skin");
    assert.equal(theme.stylePreset, undefined);
    assert.equal(theme.brandSubtitle, "CODEX DREAM SKIN");
    assert.equal(theme.colors!.accent, "#E25563");
  });

  it("restores an invalid color without dropping valid text", () => {
    const base = defaultDreamSkinTheme();
    const theme = normalizeDreamSkinTheme({
      ...base,
      name: "My Theme",
      colors: { ...base.colors!, accent: "javascript:bad" },
    });
    assert.equal(theme.name, "My Theme");
    assert.equal(theme.colors!.accent, "#E25563");
  });

  it("maps legacy market themes to their layout presets", () => {
    assert.equal(resolveDreamSkinStylePreset("preset-cyber-neon", undefined), "cyber-neon");
    assert.equal(resolveDreamSkinStylePreset("codex-snow-skin", "dream-original"), "codex-snow");
    assert.equal(resolveDreamSkinStylePreset("custom-theme", undefined), "dream-original");
  });

  it("detects text, color, and image draft changes", () => {
    const draft = {
      config: defaultDreamSkinTheme(),
      imagePath: "current.png",
      builtin: false,
    };
    assert.equal(isDreamSkinDraftDirty(draft, draft), false);
    assert.equal(isDreamSkinDraftDirty(draft, { ...draft, imagePath: "other.png" }), true);
  });
});
