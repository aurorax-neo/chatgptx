import assert from "node:assert/strict";
import { test } from "node:test";

import { resolveOfficialAccountMode } from "./official-accounts-mode.ts";

const profiles = [
  { id: "default", name: "cpa" },
  { id: "other", name: "backup" },
];

test("实时官方登录优先于保存的 active 状态", () => {
  assert.deepEqual(
    resolveOfficialAccountMode({
      liveAccountEmail: "owner@example.com",
      accounts: [
        { email: "owner@example.com", boundRelayProfileId: "default" },
        { email: "other@example.com" },
      ],
    }, profiles, "other"),
    {
      kind: "mixed",
      liveEmail: "owner@example.com",
      profile: { id: "default", name: "cpa" },
    },
  );
});

test("实时官方登录未保存时仍显示纯官方登录", () => {
  assert.deepEqual(
    resolveOfficialAccountMode({
      liveAccountEmail: "fresh@example.com",
      accounts: [{ email: "old@example.com" }],
    }, profiles, "default"),
    { kind: "official", liveEmail: "fresh@example.com" },
  );
});

test("没有实时官方登录时才按当前纯 API 供应商显示", () => {
  assert.deepEqual(
    resolveOfficialAccountMode({
      liveAccountEmail: null,
      accounts: [{ email: "old@example.com", boundRelayProfileId: "default" }],
    }, profiles, "default"),
    { kind: "pureApi", profile: { id: "default", name: "cpa" } },
  );
});
