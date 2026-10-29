export type OfficialModeProfile = {
  id: string;
  name: string;
};

export type OfficialModeAccount = {
  email: string;
  boundRelayProfileId?: string | null;
};

export type OfficialModeSnapshot = {
  liveAccountEmail?: string | null;
  accounts?: readonly OfficialModeAccount[];
};

export type OfficialAccountMode =
  | { kind: "mixed"; liveEmail: string; profile: OfficialModeProfile }
  | { kind: "official"; liveEmail: string }
  | { kind: "pureApi"; profile: OfficialModeProfile }
  | { kind: "none" };

/// 「当前生效模式」以本机实时 auth.json/Keychain 中的官方登录为最高优先级：
/// 有官方 token 且命中已保存账号的供应商绑定时是混合模式；没有官方 token 时，
/// 才按当前纯 API 供应商显示。避免保存状态里的 isActive 与实时登录不一致时误报。
export function resolveOfficialAccountMode(
  snapshot: OfficialModeSnapshot | null | undefined,
  profiles: readonly OfficialModeProfile[],
  activeRelayId: string,
): OfficialAccountMode {
  const liveEmail = String(snapshot?.liveAccountEmail || "").trim();
  if (liveEmail) {
    const account = (snapshot?.accounts || []).find(
      (candidate) => candidate.email.trim().toLowerCase() === liveEmail.toLowerCase(),
    );
    const boundProfileId = String(account?.boundRelayProfileId || "").trim();
    const profile = boundProfileId
      ? profiles.find((candidate) => candidate.id === boundProfileId)
      : undefined;
    return profile
      ? { kind: "mixed", liveEmail, profile }
      : { kind: "official", liveEmail };
  }
  const profile = profiles.find((candidate) => candidate.id === activeRelayId);
  return profile ? { kind: "pureApi", profile } : { kind: "none" };
}
