import { invoke } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  ChevronDown,
  Copy,
  Download,
  Edit3,
  ExternalLink,
  Loader2,
  Plus,
  RefreshCw,
  Rocket,
  Search,
  TestTube,
  Trash2,
  Upload,
  Users,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";

import { Badge as UiBadge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { PageHeader } from "./ManagementPages";
import { usePopoverLayer } from "./use-popover-layer";

export type OfficialQuota = {
  hourlyPercentage: number;
  hourlyResetTime?: number | null;
  hourlyWindowMinutes?: number | null;
  hourlyWindowPresent?: boolean | null;
  weeklyPercentage: number;
  weeklyResetTime?: number | null;
  weeklyWindowMinutes?: number | null;
  weeklyWindowPresent?: boolean | null;
  codeReviewPercentage?: number | null;
  codeReviewResetTime?: number | null;
  codeReviewWindowMinutes?: number | null;
  rawData?: Record<string, unknown> | null;
  planType?: string | null;
  updatedAt: number;
  error?: string | null;
};

export type OfficialAccountView = {
  id: string;
  email: string;
  label?: string | null;
  userId?: string | null;
  planType?: string | null;
  accountId?: string | null;
  organizationId?: string | null;
  accountName?: string | null;
  hasRefreshToken: boolean;
  quota?: OfficialQuota | null;
  boundRelayProfileId?: string | null;
  proxyMode: string;
  egressProxyUrl?: string | null;
  createdAt: number;
  lastUsed: number;
  tags: string[];
  isActive: boolean;
};

export type OfficialProxyResource = {
  id: string;
  name: string;
  proxyUrl: string;
  note?: string | null;
};

export type OfficialAccountsSnapshot = {
  activeAccountId?: string | null;
  liveAccountEmail?: string | null;
  liveAccountId?: string | null;
  accounts: OfficialAccountView[];
  globalProxyMode: string;
  globalProxyUrl: string;
  effectiveLaunchProxy: string;
  proxyResources: OfficialProxyResource[];
  relayProfileProxies: Record<string, string>;
  relayProfileBoundAccounts: Record<string, string>;
};

export type OAuthLoginStartResponse = {
  loginId: string;
  authUrl: string;
  callbackPort: number;
  redirectUri: string;
};

export type ProxyConnectivityResult = {
  reachable: boolean;
  proxyUrl: string;
  latencyMs?: number | null;
  statusCode?: number | null;
  detail: string;
};

type CommandEnvelope<T> = {
  status: string;
  message: string;
  payload?: T;
} & T;

type RelayProfileOption = {
  id: string;
  name: string;
  relayMode: string;
  officialMixApiKey?: boolean;
};

function unwrapPayload<T>(res: CommandEnvelope<T>): T {
  if (res && typeof res === "object" && "payload" in res && res.payload !== undefined) {
    return res.payload as T;
  }
  return res as T;
}

export function accountInitial(value: string): string {
  const trimmed = (value || "").trim();
  if (!trimmed) return "O";
  return trimmed.slice(0, 1).toUpperCase();
}

export function formatPlanDisplayName(planType?: string | null): string {
  if (!planType) return "";
  const upper = planType.trim().toUpperCase();
  if (upper.includes("TEAM")) return "TEAM";
  if (upper.includes("ENTERPRISE")) return "ENTERPRISE";
  if (upper.includes("PRO")) return "PRO";
  if (upper.includes("PLUS")) return "PLUS";
  if (upper.includes("EDU")) return "EDU";
  if (upper.includes("FREE")) return "FREE";
  return upper;
}
/**
 * 与 AppSelect 保持完全一致的下拉组件（使用 .app-select / .app-select-trigger / .app-select-menu-portal）。
 */
function PortalSelect({
  value,
  options,
  onChange,
  disabled = false,
  className = "",
  ariaLabel,
}: {
  value: string;
  options: Array<{ value: string; label: string }>;
  onChange: (next: string) => void;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  const selected = options.find((opt) => opt.value === value) || options[0];
  const { rootRef, menuRef, menuStyle } = usePopoverLayer({
    open: open && !disabled,
    onOpenChange: setOpen,
    minWidth: 180,
    maxHeight: 260,
  });

  const selectOption = (nextValue: string) => {
    onChange(nextValue);
    setOpen(false);
  };

  return (
    <div
      className={`app-select ${open ? "open" : ""} ${disabled ? "disabled" : ""} ${className}`.trim()}
      ref={rootRef}
    >
      <button
        aria-expanded={open}
        aria-label={ariaLabel}
        className="app-select-trigger"
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        type="button"
      >
        <span>{selected?.label ?? value}</span>
        <ChevronDown className="h-4 w-4" />
      </button>
      {open && !disabled
        ? createPortal(
            <div
              className={`app-select-menu app-select-menu-portal ${className}`.trim()}
              ref={menuRef}
              role="listbox"
              style={menuStyle}
            >
              {options.map((option) => (
                <button
                  aria-selected={option.value === value}
                  className={`app-select-option ${option.value === value ? "selected" : ""}`}
                  key={option.value}
                  onClick={() => selectOption(option.value)}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    selectOption(option.value);
                  }}
                  type="button"
                >
                  {option.value === value ? (
                    <CheckCircle2 className="h-4 w-4" />
                  ) : (
                    <span className="app-select-option-spacer" />
                  )}
                  <span>{option.label}</span>
                </button>
              ))}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}

// ============================================================================
// 对齐 cockpit-tools 的 Codex 配额解析与展示逻辑
// ============================================================================

type QuotaDisplayWindow = {
  id: string;
  label: string;
  percentage: number;
  resetText: string;
  tone: "high" | "medium" | "low";
  showProgress?: boolean;
  valueText?: string;
};

function clampPercentage(value: number | null | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  if (value <= 0) return 0;
  if (value >= 100) return 100;
  return Math.round(value);
}

function getQuotaTone(pct: number): "high" | "medium" | "low" {
  if (pct >= 50) return "high";
  if (pct >= 20) return "medium";
  return "low";
}

function isQuotaWindowPresent(quota: OfficialQuota, window: "hourly" | "weekly"): boolean {
  const hasPresenceFlags =
    quota.hourlyWindowPresent !== undefined && quota.hourlyWindowPresent !== null
      ? true
      : quota.weeklyWindowPresent !== undefined && quota.weeklyWindowPresent !== null;
  if (!hasPresenceFlags) return true;
  if (quota.hourlyWindowPresent === false && quota.weeklyWindowPresent === false) {
    return window === "hourly";
  }
  return window === "hourly"
    ? quota.hourlyWindowPresent === true
    : quota.weeklyWindowPresent === true;
}

export function getCodexQuotaWindowLabel(
  windowMinutes: number | null | undefined,
  fallback: "hourly" | "weekly" = "hourly",
): string {
  const HOUR_MINUTES = 60;
  const DAY_MINUTES = 24 * HOUR_MINUTES;
  const WEEK_MINUTES = 7 * DAY_MINUTES;
  const safeMinutes =
    typeof windowMinutes === "number" && Number.isFinite(windowMinutes) && windowMinutes > 0
      ? Math.ceil(windowMinutes)
      : null;

  if (safeMinutes == null) {
    return fallback === "weekly" ? t("周配额") : t("5小时配额");
  }

  // 约 30 天（27~32 天，例如 Free 账号 limit_window_seconds = 2592000 即 43200 分钟）
  if (safeMinutes >= 27 * DAY_MINUTES && safeMinutes <= 32 * DAY_MINUTES) {
    return t("月配额");
  }

  // 约 7 天（10080 分钟）
  if (safeMinutes >= WEEK_MINUTES - 60 && safeMinutes <= WEEK_MINUTES + 60) {
    return t("周配额");
  }

  // 整周倍数（如 14 天 = 2 周）
  if (safeMinutes > WEEK_MINUTES && Math.abs(safeMinutes % WEEK_MINUTES) <= 60) {
    return `${Math.round(safeMinutes / WEEK_MINUTES)}${t("周配额")}`;
  }

  if (safeMinutes >= DAY_MINUTES - 1) {
    return `${Math.round(safeMinutes / DAY_MINUTES)}${t("天配额")}`;
  }

  if (safeMinutes >= HOUR_MINUTES) {
    return `${Math.round(safeMinutes / HOUR_MINUTES)}${t("小时配额")}`;
  }

  return `${Math.ceil(safeMinutes)}${t("分钟配额")}`;
}

function formatResetTimeAbsolute(resetTime: number): string {
  const resetDate = new Date(resetTime * 1000);
  const pad = (value: number) => String(value).padStart(2, "0");
  const month = pad(resetDate.getMonth() + 1);
  const day = pad(resetDate.getDate());
  const hours = pad(resetDate.getHours());
  const minutes = pad(resetDate.getMinutes());
  return `${month}/${day} ${hours}:${minutes}`;
}

export function formatCodexResetTime(resetTime?: number | null): string {
  if (!resetTime || resetTime <= 0) return "";
  const now = Math.floor(Date.now() / 1000);
  const diff = resetTime - now;
  if (diff <= 0) return t("即将重置");

  const totalMinutes = Math.floor(diff / 60);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;

  const parts: string[] = [];
  if (days > 0) parts.push(`${days}天`);
  if (hours > 0) parts.push(`${hours}小时`);
  if (minutes > 0 && days === 0) parts.push(`${minutes}分`);

  const relative = parts.length > 0 ? `${parts.join("")}后重置` : "1分钟内重置";
  const absolute = formatResetTimeAbsolute(resetTime);
  return `${relative} (${absolute})`;
}

function toJsonRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function toFiniteNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function normalizeUnixSeconds(value: unknown): number | undefined {
  const ts = toFiniteNumber(value);
  if (ts === undefined || ts <= 0) return undefined;
  return Math.floor(ts > 10_000_000_000 ? ts / 1000 : ts);
}

function normalizeAdditionalLimitLabel(limitName: string, meteredFeature: string): string {
  const fallback = limitName || meteredFeature;
  if (!fallback) return "";
  return fallback
    .replace(/^gpt[-\s]*/i, "GPT ")
    .replace(/[-_]+/g, " ")
    .replace(/\s+/g, " ")
    .replace(/\bcodex\b/gi, "Codex")
    .replace(/\bspark\b/gi, "Spark")
    .trim();
}

export function buildAccountQuotaWindows(quota?: OfficialQuota | null): QuotaDisplayWindow[] {
  if (!quota) return [];

  const hourlyPresent = isQuotaWindowPresent(quota, "hourly");
  const weeklyPresent = isQuotaWindowPresent(quota, "weekly");

  const rawHourly = hourlyPresent ? clampPercentage(quota.hourlyPercentage) : null;
  const weekly = weeklyPresent ? clampPercentage(quota.weeklyPercentage) : null;
  const weeklyBlocksHourly = weekly === 0 && rawHourly != null;
  const hourly = weeklyBlocksHourly ? 0 : rawHourly;

  const windows: QuotaDisplayWindow[] = [];

  if (hourlyPresent && hourly != null) {
    windows.push({
      id: "primary",
      label: getCodexQuotaWindowLabel(quota.hourlyWindowMinutes, "hourly"),
      percentage: hourly,
      resetText: formatCodexResetTime(quota.hourlyResetTime),
      tone: getQuotaTone(hourly),
    });
  }

  if (weeklyPresent && weekly != null) {
    windows.push({
      id: "secondary",
      label: getCodexQuotaWindowLabel(quota.weeklyWindowMinutes, "weekly"),
      percentage: weekly,
      resetText: formatCodexResetTime(quota.weeklyResetTime),
      tone: getQuotaTone(weekly),
    });
  }

  if (windows.length === 0) {
    const fallbackPct = clampPercentage(quota.hourlyPercentage);
    windows.push({
      id: "primary",
      label: getCodexQuotaWindowLabel(quota.hourlyWindowMinutes, "hourly"),
      percentage: fallbackPct,
      resetText: formatCodexResetTime(quota.hourlyResetTime),
      tone: getQuotaTone(fallbackPct),
    });
  }

  // 解析 rawData 中的附加配额窗口（如 Spark 等）
  const raw = toJsonRecord(quota.rawData);
  const additionalList = Array.isArray(raw?.additional_rate_limits)
    ? raw.additional_rate_limits
    : [];
  additionalList.forEach((entry, sourceIndex) => {
    const record = toJsonRecord(entry);
    const rateLimit = toJsonRecord(record?.rate_limit);
    if (!record || !rateLimit) return;
    const limitName = typeof record.limit_name === "string" ? record.limit_name : "";
    const meteredFeature = typeof record.metered_feature === "string" ? record.metered_feature : "";
    const limitLabel = normalizeAdditionalLimitLabel(limitName, meteredFeature) || t("额外额度");

    ([
      ["primary_window", "primary", "hourly"] as const,
      ["secondary_window", "secondary", "weekly"] as const,
    ]).forEach(([windowKey, windowKind, fallback]) => {
      const win = toJsonRecord(rateLimit[windowKey]);
      if (!win) return;
      const usedPct = toFiniteNumber(win.used_percent);
      if (usedPct === undefined) return;
      const pct = clampPercentage(100 - Math.round(usedPct));
      const limitSec = toFiniteNumber(win.limit_window_seconds);
      const winMins = limitSec !== undefined && limitSec > 0 ? Math.ceil(limitSec / 60) : undefined;
      const resetSec = normalizeUnixSeconds(win.reset_at);
      windows.push({
        id: `additional:${sourceIndex}:${windowKind}`,
        label: `${limitLabel} ${getCodexQuotaWindowLabel(winMins, fallback)}`,
        percentage: pct,
        resetText: formatCodexResetTime(resetSec),
        tone: getQuotaTone(pct),
      });
    });
  });

  // 解析 Code Review 配额
  let crPct: number | null =
    typeof quota.codeReviewPercentage === "number"
      ? clampPercentage(quota.codeReviewPercentage)
      : null;
  let crReset = quota.codeReviewResetTime ?? undefined;
  if (crPct === null && raw) {
    const crRateLimit = toJsonRecord(raw.code_review_rate_limit);
    const crWin =
      toJsonRecord(crRateLimit?.primary_window) || toJsonRecord(crRateLimit?.secondary_window);
    const used = toFiniteNumber(crWin?.used_percent);
    if (used !== undefined) {
      crPct = clampPercentage(100 - Math.round(used));
      crReset = normalizeUnixSeconds(crWin?.reset_at);
    }
  }
  if (crPct !== null) {
    windows.push({
      id: "code_review",
      label: "Code Review",
      percentage: crPct,
      resetText: formatCodexResetTime(crReset),
      tone: getQuotaTone(crPct),
    });
  }

  return windows;
}

// ============================================================================
// 官方账号主页面（工作区 -> 官方账号）
// ============================================================================

export function OfficialAccountsScreen({
  relayProfiles,
  activeRelayId,
  onSettingsSynced,
  onLaunchMultiInstance,
  multiLaunchPendingId,
  notify,
  openExternalUrl,
}: {
  relayProfiles: RelayProfileOption[];
  activeRelayId: string;
  onSettingsSynced: () => Promise<void>;
  onLaunchMultiInstance?: (account: OfficialAccountView) => void;
  multiLaunchPendingId?: string | null;
  notify: (title: string, message: string, status?: string) => Promise<void> | void;
  openExternalUrl: (url: string) => Promise<void> | void;
}) {
  const [snapshot, setSnapshot] = useState<OfficialAccountsSnapshot | null>(null);
  const [loading, setLoading] = useState(false);

  // 搜索与分页状态
  const [searchQuery, setSearchQuery] = useState("");
  const [page, setPage] = useState(1);
  const pageSize = 8;

  // OAuth 登录状态
  const [oauthSession, setOauthSession] = useState<OAuthLoginStartResponse | null>(null);
  const [oauthWaiting, setOauthWaiting] = useState(false);
  const [manualCallbackUrl, setManualCallbackUrl] = useState("");
  const [newAccountLabel, setNewAccountLabel] = useState("");

  // 导入/导出状态
  const [importOpen, setImportOpen] = useState(false);
  const [importText, setImportText] = useState("");
  const [importLabel, setImportLabel] = useState("");
  const [importing, setImporting] = useState(false);
  const [exportJsonText, setExportJsonText] = useState("");

  // 账号操作状态
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [refreshingQuotaId, setRefreshingQuotaId] = useState<string | null>(null);
  const [editingAccountId, setEditingAccountId] = useState<string | null>(null);
  const [editLabelDraft, setEditLabelDraft] = useState("");
  const [deleteConfirmTarget, setDeleteConfirmTarget] = useState<OfficialAccountView | null>(null);
  const [useConfirmTarget, setUseConfirmTarget] = useState<OfficialAccountView | null>(null);

  const loadSnapshot = async (silent = true) => {
    if (!silent) setLoading(true);
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>("load_official_accounts");
      setSnapshot(unwrapPayload(res));
    } catch (error) {
      if (!silent) {
        void notify(t("官方账号"), String(error), "failed");
      }
    } finally {
      if (!silent) setLoading(false);
    }
  };

  useEffect(() => {
    void loadSnapshot(false);
  }, []);

  const filteredAccounts = useMemo(() => {
    const list = snapshot?.accounts ?? [];
    const q = searchQuery.trim().toLowerCase();
    if (!q) return list;
    return list.filter(
      (acc) =>
        acc.email.toLowerCase().includes(q) ||
        (acc.label && acc.label.toLowerCase().includes(q)) ||
        (acc.accountName && acc.accountName.toLowerCase().includes(q)) ||
        (acc.planType && acc.planType.toLowerCase().includes(q)),
    );
  }, [snapshot?.accounts, searchQuery]);

  const totalPages = Math.max(1, Math.ceil(filteredAccounts.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const pagedAccounts = useMemo(() => {
    const start = (currentPage - 1) * pageSize;
    return filteredAccounts.slice(start, start + pageSize);
  }, [filteredAccounts, currentPage]);

  const relayOptions = useMemo(() => {
    return [
      { value: "", label: t("不绑定（纯官方登录）") },
      ...relayProfiles.map((p) => ({
        value: p.id,
        label: p.name || p.id,
      })),
    ];
  }, [relayProfiles]);

  const handleStartOAuth = async () => {
    try {
      const res = await invoke<CommandEnvelope<OAuthLoginStartResponse | null>>(
        "start_official_oauth_login",
      );
      if (res.status !== "ok") {
        void notify(t("OAuth 登录"), res.message || t("启动失败"), "failed");
        return;
      }
      const session = unwrapPayload(res);
      if (!session) return;
      setOauthSession(session);
      setOauthWaiting(true);
      void openExternalUrl(session.authUrl);

      const completeRes = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "complete_official_oauth_login",
        {
          request: {
            loginId: session.loginId,
            label: newAccountLabel.trim() || null,
            setActive: true,
          },
        },
      );
      setOauthWaiting(false);
      if (completeRes.status === "ok") {
        setOauthSession(null);
        setNewAccountLabel("");
        setSnapshot(unwrapPayload(completeRes));
        void notify(t("官方账号"), completeRes.message, "ok");
      }
    } catch (error) {
      setOauthWaiting(false);
      void notify(t("官方账号"), String(error), "failed");
    }
  };

  const handleSubmitManualCallback = async () => {
    if (!manualCallbackUrl.trim()) {
      void notify(t("手动回调验证"), t("请粘贴浏览器回调 URL 或授权 code"), "failed");
      return;
    }
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "submit_official_oauth_callback",
        {
          request: {
            callbackUrl: manualCallbackUrl.trim(),
            label: newAccountLabel.trim() || null,
            setActive: true,
          },
        },
      );
      if (res.status === "ok") {
        setOauthSession(null);
        setOauthWaiting(false);
        setManualCallbackUrl("");
        setNewAccountLabel("");
        setSnapshot(unwrapPayload(res));
        void notify(t("官方账号"), res.message, "ok");
      } else {
        void notify(t("手动回调验证"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("手动回调验证"), String(error), "failed");
    }
  };

  const handleCancelOAuth = async () => {
    await invoke("cancel_official_oauth_login").catch(() => {});
    setOauthSession(null);
    setOauthWaiting(false);
  };

  const handleImportFromLocal = async () => {
    setImporting(true);
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "import_official_account_from_local",
        { label: null },
      );
      setSnapshot(unwrapPayload(res));
      void notify(t("导入本机账号"), res.message, res.status);
    } catch (error) {
      void notify(t("导入本机账号"), String(error), "failed");
    } finally {
      setImporting(false);
    }
  };

  const handleImportFromText = async () => {
    if (!importText.trim()) {
      void notify(t("导入官方账号"), t("请粘贴 JSON、auth.json 内容或 Refresh Token"), "failed");
      return;
    }
    setImporting(true);
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "import_official_accounts",
        {
          request: {
            input: importText.trim(),
            label: importLabel.trim() || null,
          },
        },
      );
      setSnapshot(unwrapPayload(res));
      if (res.status === "ok") {
        setImportText("");
        setImportLabel("");
        setImportOpen(false);
      }
      void notify(t("导入官方账号"), res.message, res.status);
    } catch (error) {
      void notify(t("导入官方账号"), String(error), "failed");
    } finally {
      setImporting(false);
    }
  };

  const handleExportAccounts = async () => {
    try {
      const res = await invoke<CommandEnvelope<string>>("export_official_accounts", {
        accountIds: null,
      });
      if (res.status === "ok") {
        const text = unwrapPayload(res);
        setExportJsonText(text);
        await navigator.clipboard.writeText(text).catch(() => {});
        void notify(t("导出官方账号"), t("已导出全部官方账号 JSON 并复制到剪贴板。"), "ok");
      } else {
        void notify(t("导出官方账号"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("导出官方账号"), String(error), "failed");
    }
  };

  const handleSwitchAccount = async (accountId: string, restartCodex = false) => {
    setSwitchingId(accountId);
    try {
      const res = await invoke<CommandEnvelope<{ accounts: OfficialAccountsSnapshot } | null>>(
        "switch_official_account",
        { accountId, restartCodex },
      );
      if (res.status === "ok") {
        const payload = unwrapPayload(res);
        if (payload?.accounts) {
          setSnapshot(payload.accounts);
        } else {
          await loadSnapshot(true);
        }
        await onSettingsSynced();
        void notify(t("切换官方账号"), res.message, "ok");
      } else {
        void notify(t("切换官方账号"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("切换官方账号"), String(error), "failed");
    } finally {
      setSwitchingId(null);
    }
  };

  const handleBindRelayForAccount = async (account: OfficialAccountView, nextRelayId: string) => {
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "update_official_account",
        {
          request: {
            accountId: account.id,
            label: account.label ?? null,
            boundRelayProfileId: nextRelayId || null,
            proxyMode: account.proxyMode,
            egressProxyUrl: account.egressProxyUrl ?? null,
          },
        },
      );
      setSnapshot(unwrapPayload(res));
      if (res.status === "ok") {
        if (account.isActive) {
          await handleSwitchAccount(account.id, false);
        } else {
          void notify(t("绑定供应商"), t("已保存该官方账号绑定的供应商。"), "ok");
        }
      } else {
        void notify(t("绑定供应商"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("绑定供应商"), String(error), "failed");
    }
  };

  const handleSaveAccountEdit = async (account: OfficialAccountView) => {
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "update_official_account",
        {
          request: {
            accountId: account.id,
            label: editLabelDraft,
            boundRelayProfileId: account.boundRelayProfileId ?? null,
            proxyMode: account.proxyMode,
            egressProxyUrl: account.egressProxyUrl ?? null,
          },
        },
      );
      setSnapshot(unwrapPayload(res));
      if (res.status === "ok") {
        setEditingAccountId(null);
        void notify(t("账号备注"), t("已更新官方账号备注。"), "ok");
      } else {
        void notify(t("账号备注"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("账号备注"), String(error), "failed");
    }
  };

  const handleRefreshQuota = async (accountId?: string) => {
    setRefreshingQuotaId(accountId ?? "__all__");
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "refresh_official_account_quota",
        { accountId: accountId ?? null },
      );
      setSnapshot(unwrapPayload(res));
      void notify(t("刷新配额"), res.message, res.status);
    } catch (error) {
      void notify(t("刷新配额"), String(error), "failed");
    } finally {
      setRefreshingQuotaId(null);
    }
  };

  const handleDeleteAccount = async (account: OfficialAccountView) => {
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "delete_official_account",
        { accountId: account.id },
      );
      const nextSnap = unwrapPayload(res);
      setSnapshot(nextSnap);
      if (!nextSnap?.accounts?.length) {
        setPage(1);
      }
      await onSettingsSynced();
      void notify(t("删除官方账号"), res.message, res.status);
    } catch (error) {
      void notify(t("删除官方账号"), String(error), "failed");
    }
  };

  const activeRelayProfile = relayProfiles.find((p) => p.id === activeRelayId);
  const activeOfficialAccount = snapshot?.accounts.find((a) => a.isActive) ?? null;
  const activeModeSummary = activeOfficialAccount
    ? (() => {
        const bound = relayProfiles.find((r) => r.id === activeOfficialAccount.boundRelayProfileId);
        return bound
          ? `${t("官方账号")} + ${bound.name}`
          : `${t("纯官方登录")} (${activeOfficialAccount.email})`;
      })()
    : activeRelayProfile
      ? `${t("纯 API 模式")} (${activeRelayProfile.name})`
      : t("未选择");
  return (
    <div className="mg-page relay-management-page">
      <PageHeader
        icon={<Users className="h-5 w-5" />}
        title={t("官方账号")}
        actions={
          <div className="toolbar" style={{ marginTop: 0 }}>
            <Button onClick={() => void handleStartOAuth()} disabled={oauthWaiting} size="sm">
              {oauthWaiting ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              {oauthWaiting ? t("等待授权…") : t("OAuth 登录")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void handleImportFromLocal()}
              disabled={importing}
            >
              <Download className="h-4 w-4" />
              {t("读取本机登录")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setImportOpen((prev) => !prev)}
            >
              <Upload className="h-4 w-4" />
              {t("导入")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void handleExportAccounts()}
              disabled={!snapshot?.accounts.length}
            >
              <Copy className="h-4 w-4" />
              {t("导出")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void handleRefreshQuota()}
              disabled={refreshingQuotaId !== null || !snapshot?.accounts.length}
            >
              <RefreshCw
                className={`h-4 w-4 ${refreshingQuotaId === "__all__" ? "animate-spin" : ""}`}
              />
              {t("刷新全部配额")}
            </Button>
          </div>
        }
      />
      <div className="mg-content">
        <Card className="panel relay-management-panel">
          <CardContent>
            <div className="metric-list">
              <div>
                <span>{t("本机当前登录")}</span>
                <strong>{snapshot?.liveAccountEmail || t("未检测到官方登录")}</strong>
              </div>
              <div>
                <span>{t("已保存账号")}</span>
                <strong>{String(snapshot?.accounts.length ?? 0)}</strong>
              </div>
              <div>
                <span>{t("当前生效模式")}</span>
                <strong>{activeModeSummary}</strong>
              </div>
            </div>

            {oauthSession ? (
              <div className="official-inline-box">
                <div className="official-account-title-row" style={{ justifyContent: "space-between" }}>
                  <strong>
                    {t("正在等待浏览器完成授权")}（<code>127.0.0.1:{oauthSession.callbackPort}</code>）
                  </strong>
                  <div className="toolbar" style={{ marginTop: 0 }}>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => {
                        void navigator.clipboard.writeText(oauthSession.authUrl);
                        void notify(t("授权链接"), t("已复制授权链接到剪贴板。"), "ok");
                      }}
                    >
                      <Copy className="h-4 w-4" />
                      {t("复制授权链接")}
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => void openExternalUrl(oauthSession.authUrl)}
                    >
                      <ExternalLink className="h-4 w-4" />
                      {t("打开浏览器")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => void handleCancelOAuth()}>
                      {t("取消")}
                    </Button>
                  </div>
                </div>
                <div className="form-row">
                  <Label className="field">
                    <span>{t("账号备注（可选）")}</span>
                    <Input
                      placeholder={t("例如：主力 Plus / Team")}
                      value={newAccountLabel}
                      onChange={(e) => setNewAccountLabel(e.currentTarget.value)}
                    />
                  </Label>
                  <Label className="field">
                    <span>{t("手动回调链接（远程浏览器未自动跳回时粘贴）")}</span>
                    <Input
                      placeholder="http://localhost:1455/auth/callback?code=..."
                      value={manualCallbackUrl}
                      onChange={(e) => setManualCallbackUrl(e.currentTarget.value)}
                    />
                  </Label>
                </div>
                <div className="toolbar" style={{ marginTop: 0 }}>
                  <Button size="sm" onClick={() => void handleSubmitManualCallback()}>
                    {t("提交手动回调")}
                  </Button>
                </div>
              </div>
            ) : null}

            {importOpen ? (
              <div className="official-inline-box">
                <Label className="field">
                  <span>{t("备注名称（可选）")}</span>
                  <Input
                    placeholder={t("例如：备用 Pro 账号")}
                    value={importLabel}
                    onChange={(e) => setImportLabel(e.currentTarget.value)}
                  />
                </Label>
                <Label className="field">
                  <span>{t("粘贴 auth.json、导出 JSON 数组、Refresh Token 或 Access Token")}</span>
                  <Textarea
                    rows={4}
                    placeholder='{"auth_mode":"chatgpt","tokens":{"access_token":"...","refresh_token":"..."}}'
                    value={importText}
                    onChange={(e) => setImportText(e.currentTarget.value)}
                  />
                </Label>
                <div className="toolbar" style={{ marginTop: 0 }}>
                  <Button size="sm" disabled={importing} onClick={() => void handleImportFromText()}>
                    {importing ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
                    {t("确认导入")}
                  </Button>
                  <Button size="sm" variant="secondary" onClick={() => setImportOpen(false)}>
                    {t("收起")}
                  </Button>
                </div>
              </div>
            ) : null}

            {exportJsonText ? (
              <div className="official-inline-box">
                <div className="official-account-title-row" style={{ justifyContent: "space-between" }}>
                  <strong>{t("账号导出 JSON（已复制到剪贴板）")}</strong>
                  <Button size="sm" variant="ghost" onClick={() => setExportJsonText("")}>
                    <X className="h-4 w-4" />
                  </Button>
                </div>
                <Textarea className="log-view" rows={4} readOnly value={exportJsonText} />
              </div>
            ) : null}

            <div className="official-Search-toolbar">
              <div className="mg-search">
                <Search size={15} />
                <input
                  aria-label={t("搜索邮箱、备注或计划")}
                  value={searchQuery}
                  placeholder={t("搜索邮箱、备注或计划")}
                  onChange={(e) => {
                    setSearchQuery(e.currentTarget.value);
                    setPage(1);
                  }}
                />
                {searchQuery ? (
                  <button
                    type="button"
                    className="mg-icon-button"
                    aria-label={t("清除搜索")}
                    onClick={() => setSearchQuery("")}
                  >
                    <X size={13} />
                  </button>
                ) : null}
              </div>
            </div>

            {loading ? (
              <div className="empty">{t("正在读取官方账号…")}</div>
            ) : pagedAccounts.length === 0 ? (
              <div className="provider-aggregate-empty">
                <div>
                  <strong>{t("还没有保存的官方账号")}</strong>
                  <span>{t("点击右上角「OAuth 登录」或「读取本机登录」即可添加官方账号，并与任意第三方 API 供应商自由绑定。")}</span>
                </div>
                <Button onClick={() => void handleStartOAuth()}>{t("OAuth 登录")}</Button>
              </div>
            ) : (
              <div className="relay-profile-list">
                {pagedAccounts.map((acc) => {
                  const isEditing = editingAccountId === acc.id;
                  const planLabel = formatPlanDisplayName(acc.planType || acc.quota?.planType);
                  const quotaWindows = buildAccountQuotaWindows(acc.quota);
                  const boundRelay = relayProfiles.find((r) => r.id === acc.boundRelayProfileId);
                  const subDetails = [
                    acc.accountName || "",
                    acc.hasRefreshToken ? t("可自动续期") : t("仅 Access Token"),
                    boundRelay ? `${t("已绑定")}：${boundRelay.name}` : t("纯官方模式"),
                  ]
                    .filter(Boolean)
                    .join(" · ");

                  return (
                    <div
                      key={acc.id}
                      className={`relay-profile-card official-account-card ${acc.isActive ? "active" : ""}`}
                    >
                      <div className="official-account-main">
                        <span className="relay-index" title={acc.email}>
                          {accountInitial(acc.email)}
                        </span>
                        <span className="relay-summary">
                          <span className="official-account-title-row">
                            <strong>{acc.email}</strong>
                            {planLabel ? <UiBadge variant="secondary">{planLabel}</UiBadge> : null}
                            {acc.label ? <UiBadge variant="outline">{acc.label}</UiBadge> : null}
                          </span>
                          <small>{subDetails}</small>
                        </span>
                        <span className="relay-card-actions">
                          <Button
                            className={`relay-use-button ${acc.isActive ? "active" : ""}`}
                            disabled={switchingId === acc.id}
                            onClick={() => setUseConfirmTarget(acc)}
                            size="sm"
                            variant={acc.isActive ? "secondary" : "outline"}
                          >
                            {switchingId === acc.id ? (
                              <Loader2 className="h-4 w-4 animate-spin" />
                            ) : (
                              <CheckCircle2 className="h-4 w-4" />
                            )}
                            {acc.isActive ? t("使用中") : t("使用")}
                          </Button>
                          <span className="relay-card-extra">
                            {onLaunchMultiInstance ? (
                              <Button
                                disabled={multiLaunchPendingId === `official:${acc.id}`}
                                onClick={() => onLaunchMultiInstance(acc)}
                                size="icon"
                                title={t("启动独立 Codex 实例")}
                                variant="ghost"
                              >
                                <Rocket className="h-4 w-4" />
                              </Button>
                            ) : null}
                            <Button
                              disabled={refreshingQuotaId === acc.id}
                              onClick={() => void handleRefreshQuota(acc.id)}
                              size="icon"
                              title={t("刷新配额")}
                              variant="ghost"
                            >
                              <RefreshCw
                                className={`h-4 w-4 ${refreshingQuotaId === acc.id ? "animate-spin" : ""}`}
                              />
                            </Button>
                            <Button
                              onClick={() => {
                                if (isEditing) {
                                  setEditingAccountId(null);
                                } else {
                                  setEditingAccountId(acc.id);
                                  setEditLabelDraft(acc.label || "");
                                }
                              }}
                              size="icon"
                              title={t("编辑备注")}
                              variant="ghost"
                            >
                              <Edit3 className="h-4 w-4" />
                            </Button>
                            <Button
                              onClick={() => setDeleteConfirmTarget(acc)}
                              size="icon"
                              title={t("删除账号")}
                              variant="ghost"
                            >
                              <Trash2 className="h-4 w-4" />
                            </Button>
                          </span>
                        </span>
                      </div>

                      <div className="official-account-footer">
                        <div>
                          {acc.quota?.error ? (
                            <small className="field-hint">{acc.quota.error}</small>
                          ) : quotaWindows.length > 0 ? (
                            <div className="official-quota-list">
                              {quotaWindows.map((win) => (
                                <div className="official-quota-item" key={win.id}>
                                  <div className="official-quota-head">
                                    <span className="official-quota-label">{win.label}</span>
                                    <span className="official-quota-meta">
                                      <span className="official-quota-pct">{win.percentage}%</span>
                                      {win.resetText ? (
                                        <span className="official-quota-reset">{win.resetText}</span>
                                      ) : null}
                                    </span>
                                  </div>
                                  <div className="official-quota-track">
                                    <div
                                      className={`official-quota-bar ${win.tone}`}
                                      style={{ width: `${Math.max(0, Math.min(100, win.percentage))}%` }}
                                    />
                                  </div>
                                </div>
                              ))}
                            </div>
                          ) : (
                            <small className="field-hint">
                              {t("尚未查询配额，点击右侧刷新按钮获取实时配额。")}
                            </small>
                          )}
                        </div>

                        <div className="official-binding-control">
                          <span>{t("绑定供应商")}</span>
                          <PortalSelect
                            ariaLabel={t("绑定供应商")}
                            value={acc.boundRelayProfileId || ""}
                            options={relayOptions}
                            onChange={(nextRelayId) => void handleBindRelayForAccount(acc, nextRelayId)}
                          />
                        </div>
                      </div>

                      {isEditing ? (
                        <div className="official-inline-box" style={{ marginBottom: 0 }}>
                          <Label className="field">
                            <span>{t("账号备注")}</span>
                            <Input
                              value={editLabelDraft}
                              onChange={(e) => setEditLabelDraft(e.currentTarget.value)}
                              placeholder={t("例如：主力 Plus")}
                            />
                          </Label>
                          <div className="toolbar" style={{ marginTop: 0 }}>
                            <Button size="sm" onClick={() => void handleSaveAccountEdit(acc)}>
                              {t("保存")}
                            </Button>
                            <Button
                              size="sm"
                              variant="secondary"
                              onClick={() => setEditingAccountId(null)}
                            >
                              {t("取消")}
                            </Button>
                          </div>
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            )}

            {filteredAccounts.length > 0 ? (
              <div className="official-pagination">
                <span>
                  {t("共")} {filteredAccounts.length} {t("个账号")} · {t("第")} {currentPage} / {totalPages} {t("页")}
                </span>
                <div className="toolbar" style={{ marginTop: 0 }}>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={currentPage <= 1}
                    onClick={() => setPage((p) => Math.max(1, p - 1))}
                  >
                    {t("上一页")}
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={currentPage >= totalPages}
                    onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                  >
                    {t("下一页")}
                  </Button>
                </div>
              </div>
            ) : null}
          </CardContent>
        </Card>
      </div>
      {useConfirmTarget ? (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="modal-card confirm-modal">
            <div className="modal-head">
              <div>
                <h2>{t("切换官方账号")}</h2>
              </div>
              <button
                className="toast-close"
                onClick={() => setUseConfirmTarget(null)}
                type="button"
              >
                ×
              </button>
            </div>
            <div className="confirm-modal-body">
              <p className="modal-message">
                {t("请选择切换官方账号")}「{useConfirmTarget.email}」{t("后的生效方式：")}
              </p>
            </div>
            <div className="toolbar confirm-modal-actions">
              <Button onClick={() => setUseConfirmTarget(null)} variant="outline">
                {t("取消")}
              </Button>
              <Button
                variant="secondary"
                onClick={() => {
                  const target = useConfirmTarget;
                  setUseConfirmTarget(null);
                  void handleSwitchAccount(target.id, false);
                }}
              >
                <CheckCircle2 className="h-4 w-4" />
                {t("仅使用（重启生效）")}
              </Button>
              <Button
                onClick={() => {
                  const target = useConfirmTarget;
                  setUseConfirmTarget(null);
                  void handleSwitchAccount(target.id, true);
                }}
              >
                <Rocket className="h-4 w-4" />
                {t("使用并重启 Codex")}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
      {deleteConfirmTarget ? (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="modal-card confirm-modal">
            <div className="modal-head">
              <div>
                <h2>{t("删除官方账号")}</h2>
              </div>
              <button
                className="toast-close"
                onClick={() => setDeleteConfirmTarget(null)}
                type="button"
              >
                ×
              </button>
            </div>
            <div className="confirm-modal-body">
              <p className="modal-message">
                {t("确定删除官方账号")}「{deleteConfirmTarget.email}」{t("吗？删除后将从本地账号列表中移除。")}
              </p>
            </div>
            <div className="toolbar confirm-modal-actions">
              <Button onClick={() => setDeleteConfirmTarget(null)} variant="secondary">
                {t("取消")}
              </Button>
              <Button
                onClick={() => {
                  const target = deleteConfirmTarget;
                  setDeleteConfirmTarget(null);
                  void handleDeleteAccount(target);
                }}
              >
                <Trash2 className="h-4 w-4" />
                {t("确认删除")}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

// ============================================================================
// 全局网络代理设置面板（嵌入在「设置」页 SettingsScreen 中，参考 cockpit-tools）
// ============================================================================

export function GlobalProxySettingsPanel({
  notify,
}: {
  notify: (title: string, message: string, status?: string) => Promise<void> | void;
}) {
  const [proxyMode, setProxyMode] = useState("inherit");
  const [proxyUrl, setProxyUrl] = useState("");
  const [proxyResources, setProxyResources] = useState<OfficialProxyResource[]>([]);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ProxyConnectivityResult | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>("load_official_accounts");
        const data = unwrapPayload(res);
        setProxyMode(data.globalProxyMode || "inherit");
        setProxyUrl(data.globalProxyUrl || "");
        setProxyResources(data.proxyResources || []);
      } catch {
        // 忽略静默初始化失败
      }
    })();
  }, []);

  const handleSave = async (nextMode = proxyMode, nextUrl = proxyUrl) => {
    setSaving(true);
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>("save_global_launch_proxy", {
        request: {
          globalProxyMode: nextMode,
          globalProxyUrl: nextUrl,
          proxyResources,
        },
      });
      if (res.status === "ok") {
        const snap = unwrapPayload(res);
        setProxyMode(snap.globalProxyMode);
        setProxyUrl(snap.globalProxyUrl);
        void notify(t("网络代理"), res.message, "ok");
      } else {
        void notify(t("网络代理"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("网络代理"), String(error), "failed");
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    if (!proxyUrl.trim()) {
      void notify(t("网络代理"), t("请先输入代理地址"), "failed");
      return;
    }
    setTesting(true);
    try {
      const res = await invoke<CommandEnvelope<ProxyConnectivityResult | null>>("test_launch_proxy", {
        proxyUrl: proxyUrl.trim(),
      });
      const data = unwrapPayload(res);
      if (data) setTestResult(data);
      void notify(t("代理连通性测试"), res.message, res.status);
    } catch (error) {
      void notify(t("代理连通性测试"), String(error), "failed");
    } finally {
      setTesting(false);
    }
  };

  const customEnabled = proxyMode === "custom";

  return (
    <Card className="panel">
      <CardHeader className="panel-head">
        <CardTitle>{t("网络代理")}</CardTitle>
        <CardDescription>
          {t("全局网络代理设置，作用于官方账号 OAuth 登录、配额查询、插件市场及启动 Codex 进程时的代理注入。")}
        </CardDescription>
      </CardHeader>
      <CardContent className="settings-content">
        <div className="settings-block">
          <label className="switch-row">
            <input
              checked={customEnabled}
              onChange={(event) => {
                const nextMode = event.currentTarget.checked ? "custom" : "inherit";
                setProxyMode(nextMode);
                if (!event.currentTarget.checked) {
                  void handleSave(nextMode, proxyUrl);
                }
              }}
              type="checkbox"
            />
            <span>
              <strong>{t("启用全局网络代理")}</strong>
              <small>
                {t("开启后使用下方配置的 HTTP / HTTPS / SOCKS5 代理地址，并在启动 Codex 时自动注入代理参数与本地回环白名单。")}
              </small>
            </span>
            <span className="toggle-visual" aria-hidden="true">
              <span className="toggle-thumb" />
            </span>
          </label>

          <Label className="field">
            <span>{t("代理模式")}</span>
            <PortalSelect
              ariaLabel={t("代理模式")}
              value={proxyMode}
              onChange={(val) => {
                setProxyMode(val);
                if (val !== "custom") {
                  void handleSave(val, proxyUrl);
                }
              }}
              options={[
                { value: "inherit", label: t("跟随系统环境（默认）") },
                { value: "custom", label: t("自定义全局代理") },
                { value: "direct", label: t("强制直连（绕过系统代理）") },
              ]}
            />
          </Label>

          {proxyMode === "custom" ? (
            <Label className="field">
              <span>{t("代理服务器地址")}</span>
              <Input
                value={proxyUrl}
                onChange={(event) => setProxyUrl(event.currentTarget.value)}
                placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
              />
            </Label>
          ) : null}

          {testResult ? (
            <p className="field-hint">
              {testResult.reachable
                ? `${t("连通正常")}（${testResult.latencyMs ?? 0} ms）`
                : `${t("连接失败")}：${testResult.detail}`}
            </p>
          ) : null}
        </div>

        <div className="toolbar">
          <Button disabled={saving} onClick={() => void handleSave(proxyMode, proxyUrl)}>
            {t("保存代理设置")}
          </Button>
          {proxyMode === "custom" ? (
            <Button
              variant="secondary"
              disabled={testing || !proxyUrl.trim()}
              onClick={() => void handleTest()}
            >
              <TestTube className="h-4 w-4" />
              {testing ? t("测试中…") : t("测试连通性")}
            </Button>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

// ============================================================================
// 嵌入在供应商编辑页 (`RelayProfileEditor`) 中的「官方账号绑定」区域
// ============================================================================

export function RelayOfficialBindingSection({
  relayProfileId,
  notify,
}: {
  relayProfileId: string;
  notify: (title: string, message: string, status?: string) => Promise<void> | void;
}) {
  const [snapshot, setSnapshot] = useState<OfficialAccountsSnapshot | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>("load_official_accounts");
        setSnapshot(unwrapPayload(res));
      } catch {
        // 忽略静默加载异常
      }
    })();
  }, [relayProfileId]);

  if (!snapshot) return null;

  const boundAccountId = snapshot.relayProfileBoundAccounts?.[relayProfileId] || "";

  const accountOptions = [
    { value: "", label: t("不绑定指定官方账号（保持当前登录态）") },
    ...snapshot.accounts.map((acc) => ({
      value: acc.id,
      label: acc.label ? `${acc.email} (${acc.label})` : acc.email,
    })),
  ];

  const handleAccountBind = async (nextAccountId: string) => {
    try {
      const res = await invoke<CommandEnvelope<OfficialAccountsSnapshot>>(
        "bind_relay_official_account",
        {
          request: {
            relayProfileId,
            officialAccountId: nextAccountId || null,
          },
        },
      );
      setSnapshot(unwrapPayload(res));
      if (res.status === "ok") {
        void notify(t("绑定官方账号"), t("已更新此供应商绑定的官方账号。"), "ok");
      } else {
        void notify(t("绑定官方账号"), res.message, "failed");
      }
    } catch (error) {
      void notify(t("绑定官方账号"), String(error), "failed");
    }
  };

  return (
    <div className="relay-inline-connection">
      <div className="relay-inline-connection-head">
        <div>
          <strong>{t("绑定官方账号")}</strong>
          <span>{t("应用或切换到此供应商时，自动将绑定的官方账号 OAuth 登录凭据写入 auth.json。")}</span>
        </div>
      </div>
      <div className="relay-basic-fields">
        <Label className="field">
          <span>{t("绑定的 ChatGPT 官方账号")}</span>
          <PortalSelect
            ariaLabel={t("绑定的 ChatGPT 官方账号")}
            value={boundAccountId}
            options={accountOptions}
            onChange={(val) => void handleAccountBind(val)}
          />
        </Label>
      </div>
    </div>
  );
}
