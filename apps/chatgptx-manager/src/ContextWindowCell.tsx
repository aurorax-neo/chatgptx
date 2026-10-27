import { useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Sparkles, SlidersHorizontal, X } from "lucide-react";
import { Input } from "@/components/ui/input";
import { usePopoverLayer } from "./use-popover-layer.ts";
import {
  CONTEXT_WINDOW_PRESETS,
  matchPresetIndex,
  matchContextWindowForModel,
  type BuiltinIndexLike,
} from "./model-limit-presets.ts";
import { t } from "./i18n.ts";

export type ContextWindowCellProps = {
  modelName: string;
  value: string;
  onChange: (value: string) => void;
  builtinIndex?: BuiltinIndexLike;
  placeholder?: string;
  onBlur?: () => void;
  className?: string;
};

export function ContextWindowCell({
  modelName,
  value,
  onChange,
  builtinIndex,
  placeholder = "1M",
  onBlur,
  className,
}: ContextWindowCellProps) {
  const [open, setOpen] = useState(false);
  const { rootRef, menuRef, menuStyle } = usePopoverLayer({
    open,
    onOpenChange: setOpen,
    align: "start",
    minWidth: 230,
    menuWidth: 240,
  });

  const matchedPresetIdx = useMemo(
    () => matchPresetIndex(CONTEXT_WINDOW_PRESETS, value),
    [value],
  );

  const recommendedWindow = useMemo(
    () => matchContextWindowForModel(modelName, builtinIndex),
    [modelName, builtinIndex],
  );

  return (
    <div className={`context-window-cell ${className ?? ""}`} ref={rootRef}>
      <Input
        aria-label={t("上下文窗口")}
        className="context-window-input"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.currentTarget.value)}
        onBlur={onBlur}
      />
      <button
        type="button"
        className="context-window-trigger"
        data-open={open}
        title={
          recommendedWindow
            ? `${t("常用档位与匹配")} (${t("推荐")}: ${recommendedWindow})`
            : t("常用档位与匹配")
        }
        aria-label={t("常用档位与匹配")}
        onClick={() => setOpen((prev) => !prev)}
      >
        {recommendedWindow ? (
          <Sparkles className="h-3 w-3 text-brand" />
        ) : (
          <SlidersHorizontal className="h-3 w-3" />
        )}
      </button>

      {open &&
        createPortal(
          <div
            className="context-window-popover"
            ref={menuRef}
            style={menuStyle}
            role="dialog"
            aria-label={t("上下文窗口档位与匹配")}
          >
            <div className="context-window-popover-header">
              <span>{t("常用预设档位")}</span>
              {recommendedWindow && (
                <span className="text-[10px] text-brand font-medium">
                  {t("匹配")}: {recommendedWindow}
                </span>
              )}
            </div>
            <div className="context-window-ladder" role="group" aria-label={t("常用预设档位")}>
              {CONTEXT_WINDOW_PRESETS.map((preset, idx) => {
                const isSelected = matchedPresetIdx === idx;
                return (
                  <button
                    key={preset.label}
                    type="button"
                    className="context-window-chip"
                    data-selected={isSelected}
                    aria-pressed={isSelected}
                    title={`${preset.label} (${preset.tokens.toLocaleString()} tokens)`}
                    onClick={() => {
                      onChange(preset.value);
                      setOpen(false);
                    }}
                  >
                    {preset.label}
                  </button>
                );
              })}
            </div>
            <div className="context-window-actions">
              <button
                type="button"
                className="context-window-action-btn match-btn"
                disabled={!recommendedWindow}
                title={
                  recommendedWindow
                    ? `${t("填入匹配值")}: ${recommendedWindow}`
                    : t("未在目录中匹配到该模型")
                }
                onClick={() => {
                  if (recommendedWindow) {
                    onChange(recommendedWindow);
                    setOpen(false);
                  }
                }}
              >
                <Sparkles className="h-3.5 w-3.5" />
                {recommendedWindow ? `${t("匹配填充")}: ${recommendedWindow}` : t("无匹配项")}
              </button>
              {value && (
                <button
                  type="button"
                  className="context-window-action-btn"
                  title={t("清空上下文窗口")}
                  onClick={() => {
                    onChange("");
                    setOpen(false);
                  }}
                >
                  <X className="h-3 w-3" />
                  {t("清空")}
                </button>
              )}
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}
