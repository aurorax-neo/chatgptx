  function installStyle() {
    const existingStyle = document.getElementById(styleId);
    if (existingStyle?.dataset.codexDeleteStyleVersion === codexDeleteStyleVersion) return;
    existingStyle?.remove();
    const style = document.createElement("style");
    style.id = styleId;
    style.dataset.codexDeleteStyleVersion = codexDeleteStyleVersion;
    style.textContent = `
      .${actionGroupClass} {
        position: absolute;
        right: var(--codex-session-actions-right, 28px);
        top: 50%;
        transform: translateY(-50%);
        z-index: 20;
        opacity: 0;
        pointer-events: none;
        display: inline-flex;
        align-items: center;
        gap: 2px;
        background: transparent;
      }
      .${actionButtonClass} {
        width: 20px;
        height: 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--codex-session-action-color, var(--color-token-text-tertiary, var(--chatgptx-text-tertiary, rgba(255,255,255,.5))));
        font: 14px/1 system-ui, sans-serif;
        padding: 0;
        cursor: default;
        text-align: center;
      }
      .${actionButtonClass} svg {
        display: block;
        width: 16px;
        height: 16px;
      }
      .${actionButtonClass}:hover,
      .${actionButtonClass}:focus-visible {
        background: var(--codex-session-action-hover-background, transparent);
        color: var(--codex-session-action-hover-color, var(--codex-session-action-color, var(--color-token-text-primary, #f4f4f5)));
        outline: none;
      }
      .${moreMenuClass} {
        position: fixed;
        z-index: 2147483201;
        min-width: 104px;
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
        padding: 4px;
      }
      .${moreMenuClass}[hidden] { display: none !important; }
      .${moreMenuClass}.codex-session-more-menu-open-up {
        transform: translateY(calc(-100% - 34px));
      }
      .codex-session-more-menu-item {
        width: 100%;
        border: 0;
        border-radius: var(--border-radius-sm, 6px);
        background: transparent;
        color: inherit;
        cursor: default;
        display: flex;
        align-items: center;
        gap: 8px;
        font: inherit;
        font-size: 13px;
        line-height: 18px;
        padding: 6px 8px;
        text-align: left;
      }
      .codex-session-more-menu-item:hover,
      .codex-session-more-menu-item:focus-visible {
        background: var(--chatgptx-bg-hover);
        outline: none;
      }
      .codex-session-more-menu-icon {
        width: 16px;
        text-align: center;
      }
      .${threadIdBadgeClass} {
        flex: 0 0 auto;
        display: inline-flex;
        align-items: center;
        max-width: 152px;
        margin-right: 8px;
        color: var(--text-secondary, var(--color-token-text-secondary, var(--chatgptx-text-secondary, rgba(142,142,160,.95))));
        font: 11px/1.1 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
        letter-spacing: .01em;
        opacity: .9;
        white-space: nowrap;
        user-select: text;
      }
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] {
        display: inline-flex;
        align-items: center;
        min-width: 0;
        max-width: 100%;
      }
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] ${selectors.threadTitle},
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] .truncate.select-none,
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] .truncate.text-base {
        min-width: 0;
      }
      .codex-archive-row-button {
        border: 1px solid var(--color-token-border-light, var(--color-token-border-default, rgba(0,0,0,.12)));
        border-radius: var(--border-radius-sm, 6px);
        background: var(--color-token-bg-secondary, transparent);
        color: var(--color-token-text-secondary, inherit);
        font: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 3px 8px;
        cursor: pointer;
      }
      .codex-archive-row-button.${buttonClass} {
        border-color: var(--color-border-danger, #dc2626);
        background: var(--color-background-danger-soft, rgba(220,38,38,.1));
        color: var(--color-text-danger, #dc2626);
      }
      .codex-archive-row-button.${exportButtonClass} {
        border-color: var(--color-token-border-light, var(--color-token-border-default, rgba(0,0,0,.12)));
        background: var(--color-token-bg-secondary, transparent);
        color: var(--color-token-text-primary, inherit);
      }
      .${sessionCopyMenuItemClass} {
        cursor: pointer;
      }
      .${sessionShareButtonClass} {
        position: static;
        flex: 0 0 auto;
        pointer-events: auto;
        -webkit-app-region: no-drag;
        margin-left: 2px;
        z-index: 2147483001;
        min-height: var(--height-button-composer, 32px);
        border-radius: var(--border-radius-lg, 8px);
        font: inherit;
        font-size: 13px;
        line-height: 18px;
        cursor: pointer;
        box-shadow: none;
      }
      .${sessionShareButtonClass}:hover,
      .${sessionShareButtonClass}:focus-visible {
        background: var(--color-token-list-hover-background, var(--chatgptx-bg-hover));
        color: var(--color-token-text-primary, var(--chatgptx-text));
        outline: none;
      }
      .${sessionShareButtonClass}[aria-busy="true"] {
        cursor: wait;
        opacity: .65;
      }
      [data-codex-delete-row="true"]:hover .${actionGroupClass} {
        opacity: 1;
        pointer-events: auto;
      }
      [data-codex-delete-row="true"]:hover ${selectors.threadTitle},
      [data-codex-delete-row="true"]:focus-within ${selectors.threadTitle},
      [data-codex-delete-row="true"].codex-session-more-open ${selectors.threadTitle} {
        flex: 0 1 auto;
        width: var(--codex-session-title-max-width, auto);
        max-width: var(--codex-session-title-max-width, 100%);
        overflow: hidden;
      }
      [data-codex-delete-row="true"].codex-session-more-open .${actionGroupClass} {
        opacity: 1;
        pointer-events: auto;
        z-index: 2147483201;
      }
      [data-codex-delete-row="true"].codex-archive-confirm-visible .${actionGroupClass} {
        right: max(66px, var(--codex-session-actions-right, 28px));
      }
      .${actionTooltipClass} {
        position: fixed;
        z-index: 2147483201;
        max-width: min(220px, calc(100vw - 32px));
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-md, 6px);
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 6px 8px;
        box-shadow: var(--tooltip-box-shadow, var(--shadow-200, 0 4px 12px rgba(0,0,0,.14)));
        pointer-events: none;
        white-space: nowrap;
      }
      .codex-archive-delete-all {
        border: 1px solid var(--color-border-danger, #dc2626);
        border-radius: var(--border-radius-sm, 6px);
        background: var(--color-background-danger-soft, rgba(220,38,38,.1));
        color: var(--color-text-danger, #dc2626);
        font: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 3px 8px;
        cursor: pointer;
      }
      .codex-archive-action-bar {
        position: fixed;
        right: 28px;
        top: 86px;
        z-index: 2147482999;
        box-shadow: 0 8px 24px rgba(0,0,0,.18);
      }
      .codex-delete-toast {
        position: fixed;
        right: 18px;
        bottom: 18px;
        z-index: 2147483000;
        padding: 10px 12px;
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
        pointer-events: none;
      }
      .codex-delete-toast button { margin-left: 10px; pointer-events: auto; }
      /* 拓展与内置提示共用的类型配色。不传 type 时保持上面的默认外观。 */
      .codex-delete-toast[data-toast-type="success"] { border-color: var(--chatgptx-success, #2f9e63); }
      .codex-delete-toast[data-toast-type="warn"] { border-color: var(--chatgptx-warn, #b7791f); }
      .codex-delete-toast[data-toast-type="error"] { border-color: var(--chatgptx-error, #c53030); }
      .codex-delete-confirm-overlay {
        position: fixed;
        inset: 0;
        z-index: 2147483200;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--modal-backdrop-dim-shadow, rgba(0,0,0,.32));
        backdrop-filter: blur(1px);
      }
      .codex-delete-confirm-content {
        width: min(420px, calc(100vw - 48px));
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-xl, 12px);
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 14px;
        box-shadow: var(--shadow-400, 0 16px 48px rgba(0,0,0,.2));
        padding: 20px;
      }
      .codex-delete-confirm-title { font-size: 16px; font-weight: 650; }
      .codex-delete-confirm-message { margin-top: 8px; color: var(--chatgptx-text-secondary); line-height: 1.45; }
      .codex-delete-confirm-actions {
        display: flex;
        justify-content: flex-end;
        gap: 10px;
        margin-top: 18px;
      }
      .codex-delete-confirm-actions button {
        min-height: 32px;
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        padding: 5px 12px;
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      .codex-delete-confirm-actions button:hover,
      .codex-delete-confirm-actions button:focus-visible {
        background: var(--chatgptx-bg-hover);
        outline: none;
      }
      .codex-delete-confirm-actions [data-codex-delete-confirm="true"] {
        border-color: var(--color-border-danger, #dc2626);
        background: var(--color-background-danger-solid, #dc2626);
        color: var(--color-text-danger-solid, #fff);
      }
      .chatgptx-modal-overlay {
        position: fixed;
        top: 0;
        left: 0;
        /*
         * overlay 自身带 zoom（见 applyChatGPTXZoom），而它的 inset: 0 与 100vw
         * 都按未缩放的视口算，再乘 zoom 就溢出（实测 zoom=1.2 时 100vw 得到 2072px，
         * 视口只有 1727px）。用 calc(100vw / var(--chatgptx-zoom)) 抵消；zoom 缺失
         * 时分母回落到 1，行为与改造前一致。
         * 内部子元素用百分比即可——它们在缩放空间里，百分比本来就对。
         * 注意：本段在 JS 模板字符串里，注释中不能出现反引号，否则会提前闭合。
         */
        width: calc(100vw / var(--chatgptx-zoom, 1));
        height: calc(100vh / var(--chatgptx-zoom, 1));
        z-index: 2147483646;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--modal-backdrop-dim-shadow, rgba(0,0,0,.32));
        backdrop-filter: blur(1px);
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .chatgptx-modal-content {
        width: min(520px, calc(100% - 48px));
        max-height: min(680px, calc(100% - 40px));
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-xl, 12px);
        background: var(--chatgptx-bg-primary);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 14px;
        box-shadow: var(--shadow-400, 0 16px 48px rgba(0,0,0,.2));
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .chatgptx-modal-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 16px 20px 8px;
        flex: 0 0 auto;
        -webkit-app-region: no-drag;
      }
      .chatgptx-modal-title { display: flex; align-items: center; gap: 8px; font-size: 18px; font-weight: 600; }
      .chatgptx-backend-indicator { width: 8px; height: 8px; border-radius: 999px; background: var(--chatgptx-text-tertiary); display: inline-block; }
      .chatgptx-backend-indicator[data-status="ok"] { background: var(--chatgptx-success); }
      .chatgptx-backend-indicator[data-status="failed"] { background: var(--chatgptx-danger); }
      .chatgptx-backend-indicator[data-status="checking"] { background: var(--chatgptx-warning); }
      .chatgptx-backend-indicator[data-status="degraded"] { background: var(--chatgptx-warning); }
      #${chatgptxSidebarNavId} {
        position: relative;
        flex: 0 0 auto;
      }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-icon {
        width: 20px;
        height: 20px;
        flex: 0 0 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
      }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-icon svg {
        width: 19px;
        height: 19px;
        display: block;
      }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status {
        width: 7px;
        height: 7px;
        margin-left: auto;
        border-radius: 999px;
        background: #a1a1aa;
        opacity: .9;
      }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="ok"] {
        background: #34d399;
        box-shadow: 0 0 7px rgba(52,211,153,.7);
      }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="failed"] { background: #ef4444; }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="checking"] { background: #fbbf24; }
      #${chatgptxSidebarNavId} button[data-active="true"] {
        background: var(--chatgptx-bg-selected);
        color: var(--chatgptx-text);
      }
      .chatgptx-plugin-sidebar { position: relative; flex: 0 0 auto; }
      .chatgptx-plugin-sidebar .chatgptx-sidebar-nav-icon { width: 20px; height: 20px; flex: 0 0 20px; display: inline-flex; align-items: center; justify-content: center; }
      .chatgptx-plugin-sidebar .chatgptx-sidebar-nav-icon svg { width: 19px; height: 19px; }
      .chatgptx-plugin-sidebar-label { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; min-width: 0; }
      .chatgptx-plugin-sidebar button[data-active="true"] { background: var(--chatgptx-bg-selected); color: var(--chatgptx-text); }
      .chatgptx-plugin-market-panel { max-width: 1140px; width: 100%; margin: 0 auto; box-sizing: border-box; }
      .chatgptx-plugin-intro { margin: 0 0 18px; color: var(--chatgptx-text-secondary); font-size: 13px; line-height: 1.6; }
      .chatgptx-plugin-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-bottom: 14px; }
      .chatgptx-plugin-sources { display: flex; gap: 4px; flex: 1 1 auto; }
      .chatgptx-plugin-sources button { border: 1px solid var(--chatgptx-border); border-radius: 7px; padding: 7px 12px; color: var(--chatgptx-text-secondary); background: transparent; cursor: pointer; }
      .chatgptx-plugin-sources button[data-active="true"] { background: var(--chatgptx-bg-selected); color: var(--chatgptx-text); }
      .chatgptx-plugin-search { flex: 1 1 240px; min-width: 120px; border: 1px solid var(--chatgptx-border); border-radius: 8px; background: var(--chatgptx-bg-secondary); color: var(--chatgptx-text); padding: 9px 12px; font-size: 14px; }
      .chatgptx-plugin-filter { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--chatgptx-text-secondary); }
      .chatgptx-plugin-status { font-size: 12px; color: var(--chatgptx-text-secondary); white-space: pre-wrap; margin-bottom: 14px; }
      .chatgptx-plugin-results { display: flex; flex-direction: column; gap: 10px; }
      .chatgptx-plugin-card { display: flex; align-items: flex-start; gap: 18px; padding: 16px; border: 1px solid var(--chatgptx-border); border-radius: 10px; background: var(--chatgptx-bg-secondary); }
      .chatgptx-plugin-card-info { flex: 1 1 auto; min-width: 0; }
      .chatgptx-plugin-name { font-size: 15px; font-weight: 600; overflow-wrap: anywhere; }
      .chatgptx-plugin-id, .chatgptx-plugin-meta { color: var(--chatgptx-text-tertiary); font-size: 11px; margin-top: 4px; overflow-wrap: anywhere; }
      .chatgptx-plugin-description { color: var(--chatgptx-text-secondary); font-size: 13px; line-height: 1.55; margin: 7px 0; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
      .chatgptx-plugin-tags { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 8px; }
      .chatgptx-plugin-tags span { border-radius: 5px; padding: 2px 6px; background: var(--chatgptx-bg-selected); color: var(--chatgptx-text-secondary); font-size: 10px; overflow-wrap: anywhere; }
      .chatgptx-plugin-install { flex: 0 0 auto; white-space: nowrap; }
      .chatgptx-plugin-market-page button:disabled { opacity: .55; cursor: default; }
      .chatgptx-plugin-job-message { font-size: 12px; color: var(--chatgptx-text-secondary); margin-top: 8px; white-space: pre-wrap; overflow-wrap: anywhere; }
      .chatgptx-plugin-job-message[data-status="failed"] { color: var(--chatgptx-danger); }
      .chatgptx-plugin-pagination { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 20px; font-size: 12px; color: var(--chatgptx-text-secondary); }
      .chatgptx-plugin-pagination > span { margin-right: auto; }
      .chatgptx-plugin-empty { padding: 40px 16px; text-align: center; color: var(--chatgptx-text-secondary); }
      @media (max-width: 600px) { .chatgptx-plugin-card { flex-direction: column; gap: 10px; } .chatgptx-plugin-install { align-self: flex-end; } }
      /*
       * 新版导航图标栏里的 ChatGPTX / 拓展 / 推荐内容入口：原生按钮只放图标，
       * 这里对齐它的尺寸。
       *
       * 用 [data-chatgptx-rail] 而不是逐个列 id——早先按 id 写，加第三个入口时
       * 漏掉了对应的选择器，那个图标容器就没有 20px 约束、撑成整个按钮宽，
       * 表现为图标偏左不居中。
       */
      [data-chatgptx-rail] {
        position: relative;
        flex: 0 0 auto;
        display: flex;
        align-items: center;
        justify-content: center;
      }
      [data-chatgptx-rail] > button {
        position: relative;
      }
      [data-chatgptx-rail] .chatgptx-rail-icon {
        width: 20px;
        height: 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
      }
      [data-chatgptx-rail] .chatgptx-rail-icon svg {
        width: 19px;
        height: 19px;
        display: block;
      }
      /* 图标栏是纯图标，状态点挂在按钮右上角，不占布局。 */
      #${chatgptxRailNavId} .chatgptx-sidebar-nav-status {
        position: absolute;
        top: 2px;
        right: 2px;
        margin-left: 0;
        width: 6px;
        height: 6px;
      }
      /*
       * 我们的页面是叠加在 Codex 之上的，Codex 并不知道，所以它自己那个
       * destination 的选中态会一直留着，看起来像 rail 上同时亮两个。
       * 页面打开时给根节点打标记，用 CSS 把原生选中项压成未选中；
       * 关掉页面即移除标记，原生状态自动恢复——比改它的按钮属性稳，
       * 不会和 React 的重渲染打架。
       * 颜色取自当前主题下真实未选中项（见 syncChatGPTXRailNativeSelection），
       * 主题切换时下次同步会重算。
       */
      html[data-chatgptx-page-open] nav[data-app-navigation-rail] [data-sidebar-destination][aria-current="page"] {
        color: var(--chatgptx-rail-dim, var(--chatgptx-text-tertiary, currentColor)) !important;
      }
      html[data-chatgptx-page-open] nav[data-app-navigation-rail] [data-sidebar-destination][aria-current="page"]::before {
        opacity: 0 !important;
      }
      html[data-chatgptx-page-open] nav[data-app-navigation-rail] [data-sidebar-destination][aria-current="page"] * {
        color: var(--chatgptx-rail-dim, var(--chatgptx-text-tertiary, currentColor)) !important;
      }
      /*
       * 页面 overlay 的 left 由 positionChatGPTXPage 按图标栏右边界算好写进来。
       *
       * 关键：写进来的必须是**布局坐标**（视觉值 / zoom），因为 overlay 自己在缩放
       * 空间里布局，宽度也要用同一个空间的量。width 的 calc(100vw / zoom - left - right)
       * 把右边贴到原生内容区的右边缘；三个量都除过 zoom，缩放后才正好补齐。
       * 注意：本段在 JS 模板字符串里，注释里不能出现反引号，否则会提前闭合。
       *
       * 右/下各留一圈槽（原生是 4px）并给四角加圆角，对齐官方面板的观感：
       * 官方页面面板 [_PageSurface_] 的 rect 与我们这块 overlay 完全相同（都是
       * [52, 44, 1663, 940]），四角 12px；它外面套了一个 padding: 0 4px 4px 0 的槽，
       * 右、下能看到底色。我们原先 right/bottom 贴 0 且无圆角，于是方角并且比官方
       * 多占 4px，用户反馈「少一个圆角」就是这个。
       * 圆角值由 positionChatGPTXPage 量官方面板写入，四个角一起用，不做左右区分。
       * overflow: hidden 让圆角真正裁到内容；内容都在 .chatgptx-modal-content 内，
       * 没有依赖溢出显示的全屏浮层，切掉是安全的。
       */
      .${chatgptxPageClass} {
        position: fixed;
        top: var(--chatgptx-page-top, 0px);
        right: var(--chatgptx-page-right, 0px);
        bottom: var(--chatgptx-page-bottom, 0px);
        left: var(--chatgptx-page-left, 0px);
        width: calc(100vw / var(--chatgptx-zoom, 1) - var(--chatgptx-page-left, 0px) - var(--chatgptx-page-right, 0px));
        height: calc(100vh / var(--chatgptx-zoom, 1) - var(--chatgptx-page-top, 0px) - var(--chatgptx-page-bottom, 0px));
        border-radius: var(--chatgptx-page-radius, 0px);
        overflow: hidden;
        z-index: 2147483644;
        display: block;
        background: var(--chatgptx-bg-primary, #fff);
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .${chatgptxPageClass} .chatgptx-modal-content {
        width: auto;
        height: 100%;
        max-height: none;
        border: 0;
        border-radius: 0;
        background: var(--chatgptx-bg-primary, #fff);
        box-shadow: none;
      }
      .${chatgptxPageClass} .chatgptx-modal-header {
        width: 100%;
        margin: 0;
        padding: 16px 24px 10px;
      }
      .${chatgptxPageClass} .chatgptx-modal-body {
        width: 100%;
        margin: 0;
        padding: 4px 32px 32px;
      }
      /* 两栏：左侧自己的面板（导航 / 列表），右侧内容区。观感对齐 Codex 原生页面。 */
      .${chatgptxPageClass} .chatgptx-page-layout {
        display: flex;
        flex: 1 1 auto;
        min-height: 0;
        width: 100%;
      }
      .${chatgptxPageClass} .chatgptx-page-nav {
        width: 260px;
        flex: 0 0 260px;
        display: flex;
        flex-direction: column;
        min-height: 0;
        border-right: 1px solid var(--chatgptx-border-subtle);
      }
      .${chatgptxPageClass} .chatgptx-page-nav-header {
        flex: 0 0 auto;
        padding: 4px 16px 10px;
      }
      .${chatgptxPageClass} .chatgptx-page-nav-title {
        font-size: 15px;
        font-weight: 600;
        color: var(--chatgptx-text);
      }
      .${chatgptxPageClass} .chatgptx-page-nav-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        overscroll-behavior: contain;
        padding: 4px 10px 16px;
        scrollbar-width: thin;
        scrollbar-color: rgba(255,255,255,.28) transparent;
        /*
         * 左面板是导航/列表，不是内容：拖动时不该把条目文字或分组标题选蓝
         * （列表项本来就是整行可点，选中态由 data-active 表达）。
         * 右侧详情区不设，那里的描述文字要能复制。
         */
        user-select: none;
        -webkit-user-select: none;
      }
      .${chatgptxPageClass} .chatgptx-page-main {
        flex: 1 1 auto;
        min-width: 0;
        display: flex;
        flex-direction: column;
        min-height: 0;
      }
      .chatgptx-page-nav-item {
        display: flex;
        align-items: center;
        gap: 8px;
        width: 100%;
        padding: 7px 10px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--chatgptx-text-secondary);
        font: inherit;
        font-size: 13px;
        text-align: left;
        cursor: pointer;
      }
      .chatgptx-page-nav-item:hover { background: var(--chatgptx-bg-hover); }
      .chatgptx-page-nav-item[data-active="true"] {
        background: var(--chatgptx-bg-selected);
        color: var(--chatgptx-text);
      }
      .chatgptx-page-nav-item-text { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
      .chatgptx-page-nav-item-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .chatgptx-page-nav-item-meta { font-size: 13px; color: var(--chatgptx-text-tertiary); }
      .chatgptx-page-nav-item-state {
        flex: 0 0 auto;
        width: 7px;
        height: 7px;
        border-radius: 999px;
        background: var(--chatgptx-text-tertiary);
      }
      .chatgptx-page-nav-item-state[data-state="on"] { background: #34d399; }
      /* 拓展条目：三行结构（名称 / 简介 / 作者+操作），对齐 VSCode 扩展列表的 .extension-list-item */
      .chatgptx-page-nav-item .chatgptx-extensions-item-body {
        flex: 1 1 auto;
        min-width: 0;
        display: flex;
        flex-direction: column;
        /* VSCode 的 .details 垂直居中，条目高矮不一时文字块不贴顶 */
        justify-content: center;
        overflow: hidden;
      }
      .chatgptx-extensions-item-header {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 0;
      }
      .chatgptx-extensions-icon {
        flex: 0 0 auto;
        display: flex;
        align-items: flex-start;
        padding-top: 10px;
        color: var(--chatgptx-text-secondary);
      }
      .chatgptx-extensions-icon svg { width: 40px; height: 40px; display: block; }
      /* 市场清单给的图标：正方形等比缩放，圆角与 VSCode 的扩展图标一致 */
      .chatgptx-extensions-icon .chatgptx-extensions-icon-img {
        width: 40px;
        height: 40px;
        display: block;
        object-fit: contain;
        border-radius: 6px;
      }
      .chatgptx-page-nav-item[data-active="true"] .chatgptx-extensions-icon { color: var(--chatgptx-text); }
      /* 名称：VSCode 用 semiBold，且 hover 才加下划线 */
      .chatgptx-extensions-item-name {
        flex: 1 1 auto;
        min-width: 0;
        font-weight: 600;
        color: var(--chatgptx-text);
        white-space: nowrap;
        text-overflow: ellipsis;
        overflow: hidden;
      }
      .chatgptx-page-nav-item:hover .chatgptx-extensions-item-name { text-decoration: underline; }
      .chatgptx-extensions-item-description {
        margin-top: 2px;
        padding-right: 8px;
        color: var(--chatgptx-text-tertiary);
        font-size: 13px;
        line-height: normal;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
      }
      /* 底部行：作者在左、操作在右，VSCode 的 .footer 是 24px 高 */
      .chatgptx-extensions-item-footer {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: 6px;
        min-height: 24px;
        padding-top: 2px;
      }
      .chatgptx-extensions-item-publisher {
        flex: 1 1 auto;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
        font-weight: 600;
        color: var(--chatgptx-text-tertiary);
      }
      .chatgptx-extensions-item-actions { flex: 0 0 auto; display: inline-flex; align-items: center; gap: 6px; }
      /* 行内按钮：默认低调，hover 整行时才提亮，避免列表花掉 */
      .chatgptx-extensions-item-button {
        padding: 2px 8px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 6px;
        background: transparent;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
        line-height: 16px;
        white-space: nowrap;
      }
      .chatgptx-page-nav-item:hover .chatgptx-extensions-item-button {
        background: var(--chatgptx-bg-selected);
        color: var(--chatgptx-text);
      }
      /* 启用状态点挪到名称右侧，用 VSCode 那种 14px 徽标尺寸 */
      .chatgptx-page-nav-item[data-active="true"] .chatgptx-extensions-item-state {
        background: currentColor;
      }
      .chatgptx-extensions-item-state {
        flex: 0 0 auto;
        width: 7px;
        height: 7px;
        border-radius: 999px;
        background: var(--chatgptx-text-tertiary);
      }
      .chatgptx-extensions-item-state[data-state="on"] { background: #34d399; }
      .chatgptx-extensions-icon-badge {
        flex: 0 0 auto;
        width: 14px;
        height: 14px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 999px;
        background: #10a37f;
        color: #fff;
        font-size: 13px;
        line-height: 1;
      }
      .chatgptx-page-nav-empty { padding: 8px 10px; color: var(--chatgptx-text-tertiary); font-size: 13px; }
      /* 拓展页：搜索框 + 分组标题 + 市场条目的「安装」按钮 */
      .chatgptx-page-search { padding: 0 10px 8px; }
      .chatgptx-page-search-input {
        width: 100%;
        box-sizing: border-box;
        padding: 6px 9px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 8px;
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        outline: none;
      }
      .chatgptx-page-search-input:focus { border-color: var(--chatgptx-border-subtle); background: var(--chatgptx-bg-elevated); }
      .chatgptx-page-search-input::placeholder { color: var(--chatgptx-text-tertiary); }
      /* 左面板条目图标：市场里没有图标字段，统一用 VSCode 的默认扩展字形 */
      .chatgptx-page-nav-item-icon {
        flex: 0 0 auto;
        width: 22px;
        height: 22px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 5px;
        background: var(--chatgptx-bg-hover);
        color: var(--chatgptx-text-secondary);
      }
      .chatgptx-page-nav-item-icon svg { width: 14px; height: 14px; display: block; }
      .chatgptx-page-nav-item[data-active="true"] .chatgptx-page-nav-item-icon {
        color: var(--chatgptx-text);
      }
      /* 右上角详情：形态对齐 VSCode 的扩展详情页 */
      .chatgptx-extensions-detail { padding: 4px 4px 24px; }
      .chatgptx-extensions-detail-empty {
        padding: 40px 8px;
        color: var(--chatgptx-text-tertiary);
        font-size: 13px;
        text-align: center;
      }
      .chatgptx-extensions-detail-head {
        display: flex;
        align-items: flex-start;
        gap: 14px;
        padding-bottom: 14px;
        border-bottom: 1px solid var(--chatgptx-border-subtle);
      }
      .chatgptx-extensions-detail-icon {
        flex: 0 0 auto;
        width: 64px;
        height: 64px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 12px;
        background: var(--chatgptx-bg-hover);
        color: var(--chatgptx-text-secondary);
      }
      .chatgptx-extensions-detail-icon svg { width: 40px; height: 40px; display: block; }
      .chatgptx-extensions-detail-heading { flex: 1 1 auto; min-width: 0; }
      .chatgptx-extensions-detail-title {
        font-size: 19px;
        font-weight: 600;
        color: var(--chatgptx-text);
        line-height: 1.3;
      }
      .chatgptx-extensions-detail-meta {
        margin-top: 3px;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
      }
      .chatgptx-extensions-detail-sep { margin: 0 6px; color: var(--chatgptx-text-tertiary); }
      /* issue #2359：原 #fbbf24 在浅色主题下对比度仅 1.56:1，改为随主题走的警告色。 */
      .chatgptx-extensions-detail-update { margin-top: 5px; color: var(--chatgptx-warning); font-size: 13px; }
      .chatgptx-extensions-detail-actions {
        flex: 0 0 auto;
        display: inline-flex;
        align-items: center;
        gap: 8px;
      }
      .chatgptx-extensions-detail-button {
        padding: 5px 12px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 8px;
        background: transparent;
        color: var(--chatgptx-text-secondary);
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      .chatgptx-extensions-detail-button:hover { background: var(--chatgptx-bg-hover); color: var(--chatgptx-text); }
      .chatgptx-extensions-detail-primary {
        background: #10a37f;
        border-color: #10a37f;
        color: #fff;
      }
      .chatgptx-extensions-detail-primary:hover { background: #0e8f70; color: #fff; }
      .chatgptx-extensions-detail-description {
        margin-top: 14px;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
        line-height: 1.6;
      }
      .chatgptx-extensions-detail-tags {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
        margin-top: 12px;
      }
      .chatgptx-extensions-detail-tags span {
        padding: 2px 8px;
        border-radius: 999px;
        background: var(--chatgptx-bg-hover);
        color: var(--chatgptx-text-tertiary);
        font-size: 13px;
      }
      .chatgptx-extensions-detail-section { margin-top: 16px; }
      .chatgptx-extensions-detail-section-title {
        margin-bottom: 6px;
        font-weight: 600;
        font-size: 13px;
        color: var(--chatgptx-text);
      }
      .chatgptx-extensions-detail-section ul {
        margin: 0;
        padding-left: 18px;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
        line-height: 1.7;
      }
      .chatgptx-extensions-detail-link { margin-top: 16px; font-size: 13px; }
      .chatgptx-extensions-detail-link a { color: var(--chatgptx-success); word-break: break-all; }
      .chatgptx-extensions-detail-error {
        margin-top: 14px;
        padding: 8px 10px;
        border-radius: 8px;
        background: var(--chatgptx-danger-bg);
        color: var(--chatgptx-danger);
        font-size: 13px;
      }
      .chatgptx-page-nav-group { margin-bottom: 10px; }
      .chatgptx-page-nav-group-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 4px 10px;
        color: var(--chatgptx-text-tertiary);
        font-size: 13px;
        text-transform: uppercase;
        letter-spacing: .04em;
      }
      .chatgptx-page-nav-group-count {
        min-width: 16px;
        padding: 0 5px;
        border-radius: 999px;
        background: var(--chatgptx-bg-hover);
        text-align: center;
        font-size: 13px;
      }
      .chatgptx-page-nav-group-tail { display: inline-flex; align-items: center; gap: 6px; }
      .chatgptx-page-nav-group-action {
        border: 0;
        background: transparent;
        color: var(--chatgptx-text-tertiary);
        font: inherit;
        font-size: 13px;
        cursor: pointer;
        padding: 0 2px;
      }
      .chatgptx-page-nav-group-action:hover { color: var(--chatgptx-text); }
      .chatgptx-page-nav-item-action {
        flex: 0 0 auto;
        padding: 2px 8px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 6px;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
      }
      .chatgptx-page-nav-item:hover .chatgptx-page-nav-item-action {
        background: var(--chatgptx-bg-selected);
        color: var(--chatgptx-text);
      }
      .chatgptx-modal-close {
        border: 0;
        background: transparent;
        color: #d1d5db;
        font-size: 20px;
        cursor: pointer;
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .chatgptx-modal-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        overscroll-behavior: contain;
        scrollbar-gutter: stable;
        padding: 4px 20px 16px;
        scrollbar-width: thin;
        scrollbar-color: rgba(255,255,255,.28) transparent;
      }
      .chatgptx-modal-body::-webkit-scrollbar { width: 10px; }
      .chatgptx-modal-body::-webkit-scrollbar-track { background: transparent; }
      .chatgptx-modal-body::-webkit-scrollbar-thumb {
        border: 2px solid transparent;
        border-radius: 999px;
        background: rgba(255,255,255,.28);
        background-clip: padding-box;
      }
      .chatgptx-modal-body::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,.38); background-clip: padding-box; }
      .chatgptx-row {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
        padding: 10px 0;
        border-top: 1px solid rgba(255,255,255,.1);
      }
      .chatgptx-row:first-child { border-top: 0; }
      .chatgptx-row-title { font-weight: 550; line-height: 1.35; }
      .chatgptx-row-description { margin-top: 2px; color: #a1a1aa; font-size: 13px; line-height: 1.4; }
      .chatgptx-model-compat-warning { margin-top: 6px; color: #fbbf24; font-size: 13px; line-height: 1.45; }
      .chatgptx-toggle {
        width: 42px;
        height: 24px;
        border: 0;
        border-radius: 999px;
        background: #52525b;
        padding: 2px;
      }
      .chatgptx-toggle span {
        display: block;
        width: 20px;
        height: 20px;
        border-radius: 999px;
        background: white;
        transition: transform .12s ease;
      }
      .chatgptx-toggle,
      .chatgptx-action-button,
      .chatgptx-issue-button,
      .chatgptx-backend-status {
        flex-shrink: 0;
        align-self: center;
      }
      .chatgptx-toggle[data-enabled="true"] { background: #10a37f; }
      .chatgptx-toggle[data-enabled="true"] span { transform: translateX(18px); }
      .chatgptx-toggle[data-pending="true"],
      .chatgptx-toggle:disabled { cursor: not-allowed; opacity: .55; }
      .chatgptx-width-control { display: flex; align-items: center; justify-content: flex-end; gap: 8px; min-width: 176px; align-self: center; }
      .chatgptx-width-input {
        width: 78px;
        height: 26px;
        box-sizing: border-box;
        border: 1px solid rgba(255,255,255,.18);
        border-radius: 7px;
        background: rgba(255,255,255,.08);
        color: #f3f4f6;
        font-size: 13px;
        font-family: inherit;
        padding: 0 8px;
      }
      .chatgptx-width-input:disabled { opacity: .55; cursor: not-allowed; }
      .chatgptx-typing-effect-select {
        flex-shrink: 0;
        align-self: center;
        min-width: 112px;
        height: 32px;
        padding: 0 8px;
        font-size: 13px;
      }
      .chatgptx-typing-effect-select:disabled { opacity: .55; cursor: not-allowed; }
      .chatgptx-service-tier-control { display: grid; gap: 6px; min-width: 316px; justify-items: end; align-self: center; }
      .chatgptx-service-tier-status { color: #a1a1aa; font-size: 13px; line-height: 1.3; text-align: right; }
      .chatgptx-service-tier-status[data-status="ok"] { color: #34d399; }
      .chatgptx-service-tier-status[data-status="failed"] { color: #f87171; }
      .chatgptx-service-tier-status[data-status="unsupported"] { color: #fbbf24; }
      .chatgptx-service-tier-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
      .chatgptx-service-tier-thread-actions { opacity: .88; align-items: center; }
      .chatgptx-service-tier-thread-label { color: #a1a1aa; font-size: 13px;
        line-height: 1.2;
        font-family: inherit; white-space: nowrap; }
      .chatgptx-service-tier-button { border: 1px solid rgba(255,255,255,.18); border-radius: 7px; background: #3f3f46; color: #f3f4f6; font-size: 13px;
        font-family: inherit; padding: 5px 8px; white-space: nowrap; }
      .chatgptx-service-tier-button[data-active="true"] { border-color: #10a37f; background: rgba(16,163,127,.22); color: #6ee7b7; }
      .chatgptx-service-tier-button:disabled { opacity: .55; cursor: not-allowed; }
      .${codexServiceTierBadgeClass} {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 auto;
        height: 24px;
        min-width: 54px;
        box-sizing: border-box;
        border: 1px solid rgba(148,163,184,.28);
        border-radius: 999px;
        background: rgba(148,163,184,.12);
        color: #d4d4d8;
        font-size: 13px;
        font-weight: 600;
        line-height: 1;
        font-family: inherit;
        padding: 0 8px;
        white-space: nowrap;
        cursor: pointer;
      }
      .${codexServiceTierBadgeClass}:hover { border-color: rgba(16,163,127,.44); background: rgba(16,163,127,.13); }
      .${codexServiceTierBadgeClass}[data-tier="fast"] { border-color: rgba(16,163,127,.55); background: rgba(16,163,127,.18); color: #6ee7b7; }
      .${codexServiceTierBadgeClass}[data-tier="loading"] { color: #a1a1aa; }
      .${codexServiceTierBadgeClass}[data-tier="failed"] { border-color: rgba(248,113,113,.42); background: rgba(248,113,113,.12); color: #fca5a5; }
      .${codexServiceTierBadgeClass}[data-tier="unsupported"] { border-color: rgba(251,191,36,.48); background: rgba(251,191,36,.13); color: #fbbf24; }
      .${codexServiceTierBadgeClass}[data-disabled="true"] { cursor: not-allowed; opacity: .78; }
      .chatgptx-about { color: #a1a1aa; line-height: 1.5; }
      .chatgptx-panel[hidden] { display: none; }
      .chatgptx-action-button,
      .chatgptx-issue-button { border: 1px solid rgba(255,255,255,.18); border-radius: 7px; background: #3f3f46; color: #f3f4f6; font-size: 13px;
        font-family: inherit; padding: 6px 8px; }
      .chatgptx-form-field {
        display: grid;
        gap: 4px;
        margin-top: 10px;
        /* issue #2359：原来是写死的 #d4d4d8，浅色主题下对比度只有 1.38:1（≈看不见）。
           标签文字改跟主题走。 */
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
        font-family: inherit;
        text-align: left;
      }
      .chatgptx-form-field input {
        width: min(520px, 72vw);
        border: 1px solid rgba(255,255,255,.18);
        border-radius: 8px;
        background: #18181b;
        color: #f4f4f5;
        padding: 8px 10px;
        font: 13px ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      }
      .chatgptx-form-message {
        min-height: 18px;
        margin-top: 10px;
        color: #a1a1aa;
        font-size: 13px;
        font-family: inherit;
        text-align: left;
      }
      .chatgptx-form-message[data-status="ok"] { color: #34d399; }
      .chatgptx-form-message[data-status="failed"] { color: #f87171; }
      .chatgptx-form-message[data-status="loading"] { color: #fbbf24; }
      .chatgptx-backend-status { display: grid; gap: 4px; min-width: 132px; justify-items: end; }
      .chatgptx-backend-label { color: #a1a1aa; font-size: 13px; }
      .chatgptx-backend-label[data-status="ok"] { color: #34d399; }
      .chatgptx-backend-label[data-status="failed"] { color: #f87171; }
      .chatgptx-backend-label[data-status="degraded"] { color: #fbbf24; }
      .chatgptx-sponsor-text { color: #d1d5db; font-size: 13px; line-height: 1.55; margin: 4px 0 12px; }
      /*
       * 推荐内容：网格卡片。
       *
       * 一张卡 = 图标 + 名称/简介 + 右上箭头 + 底部优惠条。用 auto-fill + minmax
       * 让列数随宽度自适应（宽屏 4 列、窄屏递减），卡片等高对齐。
       */
      .chatgptx-ad-list {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(268px, 1fr));
        gap: 12px;
      }
      .chatgptx-ad-card {
        display: flex;
        flex-direction: column;
        gap: 12px;
        border: 1px solid rgba(255,255,255,.08);
        border-radius: 12px;
        background: rgba(255,255,255,.02);
        color: inherit;
        text-decoration: none;
        padding: 16px;
        transition: background .12s ease, border-color .12s ease;
      }
      .chatgptx-ad-card:hover,
      .chatgptx-ad-card:focus-visible {
        border-color: rgba(255,255,255,.18);
        background: rgba(255,255,255,.05);
        outline: none;
      }
      .chatgptx-ad-main { display: flex; align-items: flex-start; gap: 10px; min-width: 0; }
      .chatgptx-ad-icon {
        flex: 0 0 auto;
        width: 36px;
        height: 36px;
        border-radius: 9px;
        object-fit: contain;
        background: rgba(255,255,255,.06);
      }
      /* 清单没给图时用名称首字占位，比空一块整齐。 */
      .chatgptx-ad-icon-fallback {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: #f3f4f6;
        font-size: 15px;
        font-weight: 600;
      }
      .chatgptx-ad-text { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
      .chatgptx-ad-title { overflow: hidden; color: #f8fafc; font-size: 14px; font-weight: 600; line-height: 1.3; text-overflow: ellipsis; white-space: nowrap; }
      /* 简介不再压成一行：卡片按内容撑高，最多 3 行，超出才省略。
         nowrap 会让「提供 Claude 与 ...」这类较长简介只露前几个字。 */
      .chatgptx-ad-description { display: -webkit-box; overflow: hidden; color: #a1a1aa; font-size: 13px; line-height: 1.4; -webkit-box-orient: vertical; -webkit-line-clamp: 3; }
      .chatgptx-ad-arrow { flex: 0 0 auto; width: 14px; height: 14px; margin-top: 2px; color: #71717a; }
      .chatgptx-ad-arrow svg { width: 14px; height: 14px; display: block; }
      .chatgptx-ad-card:hover .chatgptx-ad-arrow,
      .chatgptx-ad-card:focus-visible .chatgptx-ad-arrow { color: #f3f4f6; }
      /* 底部优惠条：撑满卡片宽度，长文本截断。 */
      .chatgptx-ad-promo {
        display: block;
        overflow: hidden;
        border-radius: 8px;
        background: rgba(255,255,255,.05);
        color: #f5a97f;
        font-size: 13px;
        line-height: 1.35;
        padding: 7px 10px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .chatgptx-ad-empty { border: 1px dashed rgba(255,255,255,.16); border-radius: 12px; color: #9ca3af; font-size: 13px; padding: 12px; text-align: center; }
      /*
       * 注入面板的语义色板：前景/背景必须取自同一套 token，且都要能被主题切换带走。
       *
       * issue #2359（Windows 浅色主题下黑底黑字）：旧写法在中间塞了一整层
       * --token-* 回退名（--token-bg-primary / --token-text-primary /
       * --token-border / --token-list-hover-background / --token-text-default /
       * --token-bg-fog / --token-text-tertiary …）。实测 Codex 客户端（26.9xx）
       * 的产物里这些名字**一个都不存在**——真正在用的一套是 --color-token-*。
       * 于是三级回退链的中间那级永远落空，浅色主题下只靠最后一级写死的深色值
       * 兜底，前景与背景各自独立兜底时明度撞在一起，就成了黑底黑字。
       * 这里把死名删掉，直接接到确实存在、且**由 Codex 自己按主题重算**的
       * --color-token-* 上；末级兜底仅作最后保险，不再承担主题判断。
       */
      :root {
        --chatgptx-bg-primary: var(--color-token-bg-primary, var(--color-token-main-surface-primary, #fff));
        --chatgptx-bg-secondary: var(--color-token-bg-secondary, var(--chatgptx-bg-primary));
        --chatgptx-bg-elevated: var(--color-token-dropdown-background, var(--color-token-bg-secondary, var(--chatgptx-bg-primary)));
        --chatgptx-bg-hover: var(--color-token-interactive-bg-secondary-hover, var(--color-token-list-hover-background, rgba(0,0,0,.06)));
        --chatgptx-bg-selected: var(--color-token-interactive-bg-secondary-selected, var(--chatgptx-bg-hover));
        --chatgptx-text: var(--color-token-text-primary, var(--color-token-foreground, #171717));
        --chatgptx-text-secondary: var(--color-token-text-secondary, var(--chatgptx-text));
        --chatgptx-text-tertiary: var(--color-token-text-tertiary, var(--chatgptx-text-secondary));
        --chatgptx-border: var(--color-token-border-light, var(--color-token-border, var(--color-token-border-default, rgba(0,0,0,.12))));
        --chatgptx-border-subtle: var(--chatgptx-border);
        --chatgptx-focus: var(--color-token-focus-border, var(--color-border-focus, currentColor));
        --chatgptx-danger: var(--color-text-danger, #dc2626);
        --chatgptx-danger-bg: var(--color-background-danger-soft, rgba(220,38,38,.1));
        --chatgptx-success: var(--color-text-success, #15803d);
        --chatgptx-warning: var(--color-text-warning, #a16207);
      }
      :where(.${moreMenuClass}, .${actionTooltipClass}, .codex-delete-toast, .codex-delete-confirm-overlay, .chatgptx-modal-overlay, .${chatgptxPageClass}) {
        color: var(--chatgptx-text);
        font-family: inherit;
      }
      .${moreMenuClass} {
        border-color: var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
      }
      .codex-session-more-menu-item { border-radius: var(--border-radius-sm, 6px); font-family: inherit; }
      .codex-session-more-menu-item:hover,
      .codex-session-more-menu-item:focus-visible { background: var(--chatgptx-bg-hover); }
      .${actionButtonClass} {
        width: var(--h-token-button-composer-sm, 28px);
        height: var(--h-token-button-composer-sm, 28px);
        border-radius: var(--border-radius-lg, 8px);
        color: var(--codex-session-action-color, var(--chatgptx-text-tertiary));
        font-family: inherit;
      }
      .${actionButtonClass}:hover,
      .${actionButtonClass}:focus-visible {
        background: var(--codex-session-action-hover-background, var(--chatgptx-bg-hover));
        color: var(--codex-session-action-hover-color, var(--chatgptx-text));
      }
      .${sessionShareButtonClass}:hover,
      .${sessionShareButtonClass}:focus-visible {
        background: var(--chatgptx-bg-hover);
        color: var(--chatgptx-text);
      }
      .${actionTooltipClass} {
        border-color: var(--chatgptx-border);
        border-radius: var(--border-radius-md, 6px);
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        font-family: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 6px 8px;
        box-shadow: var(--tooltip-box-shadow, var(--shadow-200, 0 4px 12px rgba(0,0,0,.14)));
      }
      .codex-delete-confirm-overlay,
      .chatgptx-modal-overlay { background: var(--color-background-surface-under, rgba(0,0,0,.32)); backdrop-filter: blur(1px); }
      .codex-delete-confirm-content,
      .chatgptx-modal-content {
        border-color: var(--chatgptx-border);
        border-radius: var(--border-radius-xl, 12px);
        background: var(--chatgptx-bg-primary);
        color: var(--chatgptx-text);
        font-family: inherit;
        box-shadow: var(--shadow-400, 0 16px 48px rgba(0,0,0,.2));
      }
      .codex-delete-confirm-message,
      .chatgptx-row-description,
      .chatgptx-about,
      .chatgptx-service-tier-thread-label,
      .chatgptx-backend-label,
      .chatgptx-form-message,
      .chatgptx-sponsor-text { color: var(--chatgptx-text-secondary); }
      .codex-delete-confirm-actions button,
      .chatgptx-action-button,
      .chatgptx-issue-button,
      .chatgptx-service-tier-button {
        min-height: 32px;
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        line-height: 18px;
        padding: 5px 10px;
      }
      .codex-delete-confirm-actions button:hover,
      .codex-delete-confirm-actions button:focus-visible,
      .chatgptx-action-button:hover,
      .chatgptx-action-button:focus-visible,
      .chatgptx-issue-button:hover,
      .chatgptx-issue-button:focus-visible,
      .chatgptx-service-tier-button:hover,
      .chatgptx-service-tier-button:focus-visible,
      .codex-delete-confirm-actions [data-codex-delete-confirm="true"] {
        border-color: var(--color-border-danger, #dc2626);
        background: var(--color-background-danger-solid, #dc2626);
        color: var(--color-text-danger-solid, #fff);
      }
      .chatgptx-modal-close { border-color: var(--chatgptx-border); color: var(--chatgptx-text-secondary); border-radius: var(--border-radius-lg, 8px); }
      .chatgptx-modal-close:hover,
      .chatgptx-modal-close:focus-visible { background: var(--chatgptx-bg-hover); color: var(--chatgptx-text); outline: none; }
      .${chatgptxPageClass},
      .${chatgptxPageClass} .chatgptx-modal-content {
        background: var(--chatgptx-bg-primary) !important;
        color: var(--chatgptx-text) !important;
      }
      .${chatgptxPageClass} .chatgptx-modal-content { border-color: transparent; }
      .chatgptx-modal-body { scrollbar-color: var(--chatgptx-text-tertiary) transparent; }
      .chatgptx-modal-body::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--chatgptx-text-tertiary) 45%, transparent); background-clip: padding-box; }
      .chatgptx-modal-body::-webkit-scrollbar-thumb:hover { background: var(--chatgptx-text-tertiary); background-clip: padding-box; }
      .chatgptx-row { border-top-color: var(--chatgptx-border-subtle); }
      .chatgptx-toggle { background: var(--color-background-secondary-solid, var(--chatgptx-text-tertiary)); }
      .chatgptx-toggle span { background: var(--color-token-bg-primary, #fff); box-shadow: var(--switch-thumb-shadow, 0 1px 2px rgba(0,0,0,.16)); }
      .chatgptx-toggle[data-enabled="true"] { background: var(--color-background-primary-solid, var(--color-background-success-solid, #10a37f)); }
      .chatgptx-width-input,
      .chatgptx-typing-effect-select,
      .chatgptx-form-field input {
        border: 1px solid var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text);
        font-family: inherit;
      }
      .chatgptx-width-input:focus,
      .chatgptx-typing-effect-select:focus,
      .chatgptx-form-field input:focus { border-color: var(--chatgptx-focus); outline: 2px solid color-mix(in srgb, var(--chatgptx-focus) 25%, transparent); outline-offset: 0; }
      .chatgptx-service-tier-button[data-active="true"] {
        border-color: var(--color-border-primary, var(--chatgptx-focus));
        background: var(--color-background-primary-soft, var(--chatgptx-bg-selected));
        color: var(--color-text-primary, var(--chatgptx-text));
      }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status,
      .chatgptx-backend-indicator { box-shadow: none; }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="ok"],
      .chatgptx-backend-indicator[data-status="ok"] { background: var(--chatgptx-success); }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="failed"],
      .chatgptx-backend-indicator[data-status="failed"] { background: var(--chatgptx-danger); }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="checking"],
      .chatgptx-backend-indicator[data-status="checking"] { background: var(--chatgptx-warning); }
      #${chatgptxSidebarNavId} .chatgptx-sidebar-nav-status[data-status="degraded"],
      .chatgptx-backend-indicator[data-status="degraded"] { background: var(--chatgptx-warning); }
      .${codexServiceTierBadgeClass} {
        height: 24px;
        border-color: var(--chatgptx-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text-secondary);
        font-family: inherit;
      }
      .${codexServiceTierBadgeClass}:hover { border-color: var(--chatgptx-focus); background: var(--chatgptx-bg-hover); }
      .${codexServiceTierBadgeClass}[data-tier="fast"] { border-color: var(--color-border-primary, var(--chatgptx-focus)); background: var(--color-background-primary-soft, var(--chatgptx-bg-selected)); color: var(--chatgptx-text); }
      .${codexServiceTierBadgeClass}[data-tier="failed"] { border-color: var(--color-border-danger, var(--chatgptx-danger)); background: var(--chatgptx-danger-bg); color: var(--chatgptx-danger); }
      .${codexServiceTierBadgeClass}[data-tier="unsupported"] { border-color: var(--color-border-warning, var(--chatgptx-border)); background: var(--color-background-warning-soft, var(--chatgptx-bg-hover)); color: var(--chatgptx-warning); }
      .chatgptx-ad-card { border-color: var(--chatgptx-border-subtle); background: var(--chatgptx-bg-secondary); }
      .chatgptx-ad-card:hover,
      .chatgptx-ad-card:focus-visible { border-color: var(--chatgptx-border); background: var(--chatgptx-bg-hover); }
      .chatgptx-ad-icon { background: var(--chatgptx-bg-hover); }
      .chatgptx-ad-icon-fallback { color: var(--chatgptx-text); }
      .chatgptx-ad-title { color: var(--chatgptx-text); }
      .chatgptx-ad-description { color: var(--chatgptx-text-secondary); }
      .chatgptx-ad-arrow { color: var(--chatgptx-text-tertiary); }
      .chatgptx-ad-card:hover .chatgptx-ad-arrow,
      .chatgptx-ad-card:focus-visible .chatgptx-ad-arrow { color: var(--chatgptx-text); }
      /*
       * 优惠条：暗底配橙色文字。
       *
       * 底色不能用 --chatgptx-danger-bg —— 它在当前主题下解析成浅粉（偏浅色主题
       * 的值），压在深色卡片上非常刺眼。改成用警告色按低透明度混出来，深浅主题
       * 都成立，也和原生「需要注意」的语义色同源。
       */
      .chatgptx-ad-promo {
        background: color-mix(in srgb, var(--chatgptx-warning) 14%, transparent);
        color: var(--chatgptx-warning);
      }
      .chatgptx-ad-empty { border-color: var(--chatgptx-border); color: var(--chatgptx-text-tertiary); }
      .chatgptx-form-message[data-status="ok"], .chatgptx-service-tier-status[data-status="ok"], .chatgptx-backend-label[data-status="ok"] { color: var(--chatgptx-success); }
      .chatgptx-form-message[data-status="failed"], .chatgptx-service-tier-status[data-status="failed"], .chatgptx-backend-label[data-status="failed"] { color: var(--chatgptx-danger); }
      .chatgptx-backend-label[data-status="degraded"] { color: var(--chatgptx-warning); }
      .chatgptx-form-message[data-status="loading"], .chatgptx-service-tier-status[data-status="unsupported"], .chatgptx-model-compat-warning { color: var(--chatgptx-warning); }
      /* ChatGPTX 设置主页：对齐原生设置页，保持其它页面与弹窗的布局独立。 */
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="home"] {
        background: var(--chatgptx-settings-page-bg, var(--chatgptx-bg-primary)) !important;
      }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="home"] .chatgptx-modal-body {
        box-sizing: border-box;
        padding: 72px 32px 64px;
      }
      .chatgptx-settings {
        width: 100%;
        max-width: 740px;
        min-width: 0;
        margin: 0 auto;
        container: chatgptx-settings / inline-size;
      }
      .chatgptx-settings > .chatgptx-modal-header {
        width: 100%;
        box-sizing: border-box;
        padding: 0;
        margin: 0 0 48px;
        gap: 16px;
      }
      .chatgptx-settings .chatgptx-modal-title {
        min-width: 0;
        font-size: 28px;
        font-weight: 600;
        line-height: 36px;
        letter-spacing: -.02em;
        overflow-wrap: anywhere;
      }
      .chatgptx-settings-section { margin: 0 0 48px; }
      .chatgptx-settings-section:last-child { margin-bottom: 0; }
      .chatgptx-settings-section-title {
        margin: 0 0 16px;
        color: var(--chatgptx-text);
        font-size: 14px;
        font-weight: 550;
        line-height: 20px;
      }
      .chatgptx-settings-card,
      .chatgptx-settings > [data-chatgptx-ext-menu] {
        box-sizing: border-box;
        min-width: 0;
        padding: 0 16px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 16px;
        background: var(--chatgptx-settings-card-bg, var(--chatgptx-bg-secondary));
      }
      .chatgptx-settings > [data-chatgptx-ext-menu] { margin-top: 48px; }
      .chatgptx-settings .chatgptx-row {
        box-sizing: border-box;
        min-height: 60px;
        align-items: center;
        gap: 24px;
        padding: 10px 0;
        font-size: 14px;
        line-height: 20px;
      }
      .chatgptx-settings .chatgptx-row[hidden] { display: none; }
      .chatgptx-settings .chatgptx-row > :first-child {
        flex: 1 1 auto;
        min-width: 0;
        overflow-wrap: anywhere;
      }
      .chatgptx-settings .chatgptx-row > :not(:first-child) { max-width: 48%; }
      .chatgptx-settings .chatgptx-row-title { font-weight: 500; line-height: 20px; }
      .chatgptx-settings .chatgptx-row-description,
      .chatgptx-settings .chatgptx-about {
        margin-top: 2px;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
        line-height: 18px;
        overflow-wrap: anywhere;
      }
      .chatgptx-settings .chatgptx-toggle {
        flex: 0 0 32px;
        width: 32px;
        height: 20px;
        min-height: 20px;
        box-sizing: border-box;
        padding: 2px;
        background: var(--chatgptx-switch-off, #3a3a3a);
        cursor: pointer;
      }
      .chatgptx-settings .chatgptx-toggle span {
        width: 16px;
        height: 16px;
        background: var(--chatgptx-switch-thumb, #fff);
      }
      .chatgptx-settings .chatgptx-toggle[data-enabled="true"] { background: var(--chatgptx-switch-on, #3485ff); }
      .chatgptx-settings .chatgptx-toggle[data-enabled="true"] span { transform: translateX(12px); }
      .chatgptx-settings .chatgptx-toggle[data-pending="true"],
      .chatgptx-settings .chatgptx-toggle:disabled { cursor: not-allowed; opacity: .55; }
      .chatgptx-settings .chatgptx-width-control {
        flex: 0 1 auto;
        min-width: 0;
        flex-wrap: wrap;
        gap: 8px;
      }
      .chatgptx-settings .chatgptx-service-tier-control {
        flex: 0 1 300px;
        min-width: 0;
      }
      .chatgptx-settings .chatgptx-service-tier-status { overflow-wrap: anywhere; }
      .chatgptx-settings .chatgptx-backend-status { overflow-wrap: anywhere; text-align: right; }
      .chatgptx-settings .chatgptx-action-button,
      .chatgptx-settings .chatgptx-issue-button,
      .chatgptx-settings .chatgptx-service-tier-button,
      .chatgptx-settings .chatgptx-width-input,
      .chatgptx-settings .chatgptx-typing-effect-select {
        box-sizing: border-box;
        min-height: 32px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 8px;
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text);
        font-family: inherit;
        font-size: 13px;
        line-height: 18px;
      }
      .chatgptx-settings .chatgptx-width-input,
      .chatgptx-settings .chatgptx-typing-effect-select { height: 32px; }
      .chatgptx-settings .chatgptx-width-input { width: 72px; min-width: 48px; }
      .chatgptx-settings .chatgptx-typing-effect-select { min-width: 0; }
      .chatgptx-settings .chatgptx-typing-effect-select[hidden] { display: none; }
      .chatgptx-settings .chatgptx-dropdown-trigger {
        display: inline-flex;
        flex: 0 0 auto;
        align-items: center;
        align-self: center;
        justify-content: space-between;
        gap: 8px;
        height: 28px;
        min-width: 112px;
        box-sizing: border-box;
        border: 1px solid var(--chatgptx-border);
        border-radius: 8px;
        padding: 3px 10px;
        background: var(--chatgptx-bg-secondary);
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        line-height: 20px;
        cursor: pointer;
      }
      .chatgptx-settings .chatgptx-dropdown-trigger > svg {
        flex: 0 0 16px;
        width: 16px;
        height: 16px;
        color: var(--chatgptx-text-secondary);
      }
      .chatgptx-settings .chatgptx-dropdown-trigger:hover:not(:disabled),
      .chatgptx-settings .chatgptx-dropdown-trigger[aria-expanded="true"] { background: var(--chatgptx-bg-hover); }
      .chatgptx-settings .chatgptx-dropdown-trigger:disabled { cursor: not-allowed; opacity: .55; }
      /* 菜单挂在 overlay 上，避免设置卡片或滚动容器裁切。 */
      .chatgptx-dropdown-menu {
        position: absolute;
        z-index: 2;
        overflow-y: auto;
        overscroll-behavior: contain;
        width: 220px;
        max-width: calc(100% - 16px);
        box-sizing: border-box;
        padding: 6px;
        border: 1px solid var(--chatgptx-border);
        border-radius: 14px;
        background: var(--chatgptx-bg-elevated);
        color: var(--chatgptx-text);
        font-family: inherit;
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
      }
      .chatgptx-dropdown-menu[hidden] { display: none; }
      .chatgptx-dropdown-option {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        width: 100%;
        min-height: 28px;
        box-sizing: border-box;
        border: 0;
        border-radius: 8px;
        padding: 4px 12px;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 14px;
        line-height: 20px;
        text-align: left;
        cursor: pointer;
      }
      .chatgptx-dropdown-option > svg {
        flex: 0 0 16px;
        width: 16px;
        height: 16px;
        visibility: hidden;
      }
      .chatgptx-dropdown-option[aria-selected="true"] > svg { visibility: visible; }
      .chatgptx-dropdown-option:hover,
      .chatgptx-dropdown-option:focus-visible {
        background: var(--chatgptx-bg-hover);
        outline: none;
      }
      .chatgptx-settings .chatgptx-action-button,
      .chatgptx-settings .chatgptx-issue-button { white-space: normal; overflow-wrap: anywhere; }
      .chatgptx-settings .chatgptx-action-button:hover,
      .chatgptx-settings .chatgptx-action-button:focus-visible,
      .chatgptx-settings .chatgptx-issue-button:hover,
      .chatgptx-settings .chatgptx-issue-button:focus-visible,
      .chatgptx-settings .chatgptx-service-tier-button:hover,
      .chatgptx-settings .chatgptx-service-tier-button:focus-visible {
        border-color: var(--chatgptx-border);
        background: var(--chatgptx-bg-hover);
        color: var(--chatgptx-text);
      }
      .chatgptx-settings .chatgptx-service-tier-button[data-active="true"] {
        border-color: var(--chatgptx-switch-on, #3485ff);
        background: color-mix(in srgb, var(--chatgptx-switch-on, #3485ff) 16%, var(--chatgptx-settings-card-bg, var(--chatgptx-bg-secondary)));
        color: var(--chatgptx-text);
      }
      .chatgptx-settings button:focus-visible,
      .chatgptx-settings input:focus-visible,
      .chatgptx-settings select:focus-visible {
        outline: 2px solid var(--chatgptx-switch-on, #3485ff);
        outline-offset: 3px;
      }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="home"] { container: chatgptx-settings-page / inline-size; }
      @container chatgptx-settings-page (max-width: 800px) {
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="home"] .chatgptx-modal-body { padding: 48px 24px; }
      }
      @container chatgptx-settings-page (max-width: 480px) {
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="home"] .chatgptx-modal-body { padding: 32px 16px; }
      }
      @container chatgptx-settings (max-width: 560px) {
        .chatgptx-settings .chatgptx-row { gap: 16px; }
        .chatgptx-settings .chatgptx-row[data-codex-service-tier-controls="true"] { flex-wrap: wrap; }
        .chatgptx-settings .chatgptx-row[data-codex-service-tier-controls="true"] > :first-child { flex-basis: 100%; }
        .chatgptx-settings .chatgptx-row[data-codex-service-tier-controls="true"] > .chatgptx-service-tier-control { flex-basis: 100%; max-width: 100%; }
      }
      /* 拓展与推荐内容使用原生插件页面的导航、标题和轻量列表布局。 */
      .${chatgptxPageClass} .chatgptx-modal-content:is([data-chatgptx-active-tab="extensions"], [data-chatgptx-active-tab="sponsor"]) {
        background: var(--chatgptx-settings-page-bg, var(--chatgptx-bg-primary)) !important;
        container: chatgptx-discovery-page / inline-size;
      }
      .${chatgptxPageClass} .chatgptx-modal-content:is([data-chatgptx-active-tab="extensions"], [data-chatgptx-active-tab="sponsor"]) > .chatgptx-modal-header { display: none; }
      .${chatgptxPageClass} .chatgptx-modal-content:is([data-chatgptx-active-tab="extensions"], [data-chatgptx-active-tab="sponsor"]) .chatgptx-modal-body {
        box-sizing: border-box;
        padding: 32px 32px 56px;
      }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav {
        box-sizing: border-box;
        width: 312px;
        flex-basis: 312px;
        background: var(--chatgptx-nav-bg, var(--chatgptx-bg-secondary));
      }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-item[data-active="true"] { background: var(--chatgptx-nav-selected-bg, var(--chatgptx-bg-selected)); }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-header { padding: 16px; }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-title { font-size: 18px; line-height: 24px; }
      .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-body { padding: 0 12px 20px; }
      .chatgptx-extensions-nav-entry {
        min-height: 36px;
        gap: 10px;
        box-sizing: border-box;
        padding: 8px 12px;
        font-size: 14px;
        font-weight: 400;
        line-height: 20px;
      }
      .chatgptx-extensions-nav-entry .chatgptx-page-nav-item-icon,
      .chatgptx-extensions-nav-entry .chatgptx-extensions-icon {
        flex: 0 0 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        padding: 0;
        background: transparent;
      }
      .chatgptx-extensions-nav-entry .chatgptx-page-nav-item-icon svg,
      .chatgptx-extensions-nav-entry .chatgptx-extensions-icon svg,
      .chatgptx-extensions-nav-entry .chatgptx-extensions-icon-img { width: 20px; height: 20px; }
      .chatgptx-extensions-nav-entry .chatgptx-extensions-icon-img { display: block; object-fit: contain; }
      .chatgptx-extensions-nav-entry .chatgptx-extensions-item-name { font-weight: 400; }
      .chatgptx-extensions-nav-entry + .chatgptx-page-nav-group { margin-top: 24px; }
      .chatgptx-extensions-nav-entry:hover .chatgptx-extensions-item-name { text-decoration: none; }
      .chatgptx-discovery {
        width: 100%;
        max-width: 900px;
        min-width: 0;
        margin: 0 auto;
        padding: 0;
        container: chatgptx-discovery / inline-size;
      }
      .chatgptx-discovery-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 24px;
      }
      .chatgptx-discovery-header > :first-child { flex: 1 1 auto; min-width: 0; }
      .chatgptx-discovery-title {
        margin: 0;
        color: var(--chatgptx-text);
        font-size: 26px;
        font-weight: 600;
        line-height: 34px;
        letter-spacing: -.02em;
        overflow-wrap: anywhere;
      }
      .chatgptx-discovery-description {
        margin: 12px 0 0;
        color: var(--chatgptx-text-secondary);
        font-size: 14px;
        line-height: 20px;
        overflow-wrap: anywhere;
      }
      .chatgptx-discovery-toolbar { display: flex; flex: 0 1 auto; align-items: center; gap: 8px; min-width: 0; }
      .chatgptx-discovery-search {
        display: flex;
        flex: 0 1 240px;
        align-items: center;
        gap: 8px;
        width: 240px;
        max-width: 100%;
        min-width: 0;
        box-sizing: border-box;
        border: 1px solid var(--chatgptx-border);
        border-radius: 999px;
        padding: 8px 12px;
        background: transparent;
        color: var(--chatgptx-text-secondary);
      }
      .chatgptx-discovery-search > svg { flex: 0 0 16px; width: 16px; height: 16px; }
      .chatgptx-discovery-search input {
        width: 100%;
        min-width: 0;
        border: 0;
        padding: 0;
        outline: none;
        background: transparent;
        color: var(--chatgptx-text);
        font: inherit;
        font-size: 13px;
        line-height: 18px;
      }
      .chatgptx-discovery-search input::placeholder { color: var(--chatgptx-text-tertiary); }
      .chatgptx-discovery-search:focus-within {
        outline: 2px solid var(--chatgptx-switch-on, #3485ff);
        outline-offset: 2px;
      }
      .chatgptx-discovery-icon-button {
        display: inline-flex;
        flex: 0 0 32px;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        box-sizing: border-box;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: var(--chatgptx-text-secondary);
        cursor: pointer;
      }
      .chatgptx-discovery-icon-button svg { width: 16px; height: 16px; }
      .chatgptx-discovery-icon-button:hover { background: var(--chatgptx-bg-hover); color: var(--chatgptx-text); }
      .chatgptx-discovery-icon-button:disabled { opacity: .55; cursor: not-allowed; }
      .chatgptx-extensions-filter { display: flex; align-items: center; gap: 8px; margin-top: 16px; }
      .chatgptx-extensions-filter button {
        border: 0;
        border-radius: 999px;
        padding: 8px 16px;
        background: transparent;
        color: var(--chatgptx-text-secondary);
        font: inherit;
        font-size: 13px;
        line-height: 20px;
        cursor: pointer;
      }
      .chatgptx-extensions-filter button[data-active="true"] { background: var(--chatgptx-bg-selected); color: var(--chatgptx-text); }
      .chatgptx-extensions-filter button:hover { background: var(--chatgptx-bg-hover); }
      .chatgptx-discovery-section-title {
        margin: 32px 0 16px;
        color: var(--chatgptx-text);
        font-size: 16px;
        font-weight: 550;
        line-height: 24px;
      }
      .chatgptx-extensions-grid,
      .chatgptx-discovery .chatgptx-ad-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 32px; }
      .chatgptx-extensions-grid > .chatgptx-extensions-detail-empty,
      .chatgptx-discovery .chatgptx-ad-empty { grid-column: 1 / -1; }
      .chatgptx-extension-card {
        display: flex;
        align-items: center;
        gap: 16px;
        min-width: 0;
        min-height: 56px;
        box-sizing: border-box;
        border: 0;
        padding: 8px 0;
        background: transparent;
      }
      .chatgptx-extension-card-select {
        display: flex;
        flex: 1 1 auto;
        align-items: center;
        gap: 12px;
        min-width: 0;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: var(--chatgptx-text);
        font: inherit;
        text-align: left;
        cursor: pointer;
      }
      .chatgptx-extension-card-icon {
        display: inline-flex;
        flex: 0 0 36px;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        box-sizing: border-box;
        border: 1px solid var(--chatgptx-border);
        border-radius: 10px;
        color: var(--chatgptx-text-secondary);
      }
      .chatgptx-extension-card-icon svg { width: 24px; height: 24px; }
      .chatgptx-extension-card-icon img { width: 32px; height: 32px; object-fit: contain; border-radius: 8px; }
      .chatgptx-extension-card-copy { display: flex; flex: 1 1 auto; flex-direction: column; gap: 2px; min-width: 0; }
      .chatgptx-extension-card-title {
        overflow: hidden;
        color: var(--chatgptx-text);
        font-size: 14px;
        font-weight: 500;
        line-height: 20px;
        white-space: nowrap;
        text-overflow: ellipsis;
      }
      .chatgptx-extension-card-description {
        display: block;
        overflow: hidden;
        color: var(--chatgptx-text-secondary);
        font-size: 13px;
        line-height: 18px;
        white-space: nowrap;
        text-overflow: ellipsis;
      }
      .chatgptx-extension-card-actions { display: inline-flex; flex: 0 0 auto; align-items: center; gap: 4px; }
      .chatgptx-discovery .chatgptx-extensions-detail { max-width: 900px; margin: 0 auto; padding: 0 0 24px; }
      .chatgptx-discovery .chatgptx-extensions-detail-head { align-items: center; border-bottom: 0; gap: 20px; padding-bottom: 20px; }
      .chatgptx-discovery .chatgptx-extensions-detail-title { font-size: 26px; line-height: 34px; overflow-wrap: anywhere; }
      .chatgptx-discovery .chatgptx-extensions-detail-icon img { width: 40px; height: 40px; object-fit: contain; border-radius: 8px; }
      .chatgptx-discovery .chatgptx-extensions-detail-description { font-size: 14px; line-height: 20px; overflow-wrap: anywhere; }
      .chatgptx-discovery .chatgptx-extensions-detail-actions { flex-wrap: wrap; }
      .chatgptx-discovery .chatgptx-extensions-detail-button { min-height: 32px; box-sizing: border-box; }
      .chatgptx-discovery .chatgptx-extensions-detail-primary {
        border-color: var(--chatgptx-text);
        background: var(--chatgptx-text);
        color: var(--chatgptx-settings-page-bg, var(--chatgptx-bg-primary));
      }
      .chatgptx-discovery .chatgptx-extensions-detail-primary:hover {
        background: color-mix(in srgb, var(--chatgptx-text) 90%, transparent);
        color: var(--chatgptx-settings-page-bg, var(--chatgptx-bg-primary));
      }
      .chatgptx-discovery .chatgptx-extensions-detail-link a { color: var(--color-text-link, var(--color-token-text-link, var(--chatgptx-switch-on, #3485ff))); }
      .chatgptx-discovery .chatgptx-toggle { flex: 0 0 32px; width: 32px; height: 20px; min-height: 20px; box-sizing: border-box; padding: 2px; background: var(--chatgptx-switch-off, #3a3a3a); cursor: pointer; }
      .chatgptx-discovery .chatgptx-toggle span { width: 16px; height: 16px; background: var(--chatgptx-switch-thumb, #fff); }
      .chatgptx-discovery .chatgptx-toggle[data-enabled="true"] { background: var(--chatgptx-switch-on, #3485ff); }
      .chatgptx-discovery .chatgptx-toggle[data-enabled="true"] span { transform: translateX(12px); }
      .chatgptx-discovery .chatgptx-toggle[data-pending="true"],
      .chatgptx-discovery .chatgptx-toggle:disabled { cursor: not-allowed; opacity: .55; }
      .chatgptx-extensions-back {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        margin: 0 0 24px;
        border: 0;
        border-radius: 8px;
        padding: 4px 0;
        background: transparent;
        color: var(--chatgptx-text-secondary);
        font: inherit;
        font-size: 13px;
        line-height: 20px;
        cursor: pointer;
      }
      .chatgptx-extensions-back svg { width: 16px; height: 16px; }
      .chatgptx-extensions-back:hover { color: var(--chatgptx-text); }
      .chatgptx-discovery button:focus-visible,
      .chatgptx-discovery a:focus-visible,
      .chatgptx-extensions-nav-entry:focus-visible { outline: 2px solid var(--chatgptx-switch-on, #3485ff); outline-offset: 3px; }
      .chatgptx-discovery .chatgptx-ad-card {
        flex-direction: row;
        align-items: center;
        gap: 16px;
        min-height: 72px;
        min-width: 0;
        box-sizing: border-box;
        border: 0;
        border-radius: 8px;
        padding: 8px 0;
        background: transparent;
      }
      .chatgptx-discovery .chatgptx-ad-card:hover,
      .chatgptx-discovery .chatgptx-ad-card:focus-visible { background: transparent; }
      .chatgptx-discovery .chatgptx-ad-main { flex: 1 1 auto; align-items: center; gap: 12px; }
      .chatgptx-discovery .chatgptx-ad-icon { flex: 0 0 36px; border: 1px solid var(--chatgptx-border); border-radius: 10px; background: transparent; box-sizing: border-box; }
      .chatgptx-discovery .chatgptx-ad-text { gap: 4px; }
      .chatgptx-discovery .chatgptx-ad-title { font-size: 14px; font-weight: 500; line-height: 20px; }
      .chatgptx-discovery .chatgptx-ad-description { font-size: 13px; line-height: 18px; -webkit-line-clamp: 2; overflow-wrap: anywhere; }
      .chatgptx-discovery .chatgptx-ad-promo { border-radius: 0; padding: 0; background: transparent; color: var(--chatgptx-text-secondary); font-size: 12px; line-height: 16px; white-space: normal; overflow-wrap: anywhere; }
      .chatgptx-discovery .chatgptx-ad-arrow { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; margin: 0; color: var(--chatgptx-text-secondary); }
      .chatgptx-discovery .chatgptx-ad-arrow svg { width: 16px; height: 16px; }
      @container chatgptx-discovery-page (max-width: 900px) {
        .chatgptx-discovery-header { flex-wrap: wrap; gap: 20px; }
        .chatgptx-discovery-toolbar { flex-basis: 100%; }
        .chatgptx-discovery .chatgptx-extensions-detail-head { flex-wrap: wrap; }
        .chatgptx-discovery .chatgptx-extensions-detail-actions { flex-basis: 100%; }
      }
      @container chatgptx-discovery (max-width: 600px) {
        .chatgptx-extensions-grid,
        .chatgptx-discovery .chatgptx-ad-list { grid-template-columns: minmax(0, 1fr); }
      }
      @container chatgptx-discovery-page (max-width: 700px) {
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav { width: 180px; flex-basis: 180px; }
        .${chatgptxPageClass} .chatgptx-modal-content:is([data-chatgptx-active-tab="extensions"], [data-chatgptx-active-tab="sponsor"]) .chatgptx-modal-body { padding: 24px 20px 40px; }
      }
      @container chatgptx-discovery-page (max-width: 480px) {
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav { width: 56px; flex-basis: 56px; }
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-header { display: none; }
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-body { padding: 12px 8px; }
        .chatgptx-extensions-nav-entry { justify-content: center; padding: 8px; }
        .chatgptx-extensions-nav-entry .chatgptx-page-nav-item-text,
        .chatgptx-extensions-nav-entry .chatgptx-page-nav-item-name,
        .chatgptx-extensions-nav-entry .chatgptx-extensions-item-name,
        .chatgptx-extensions-nav-entry .chatgptx-page-nav-item-state,
        .chatgptx-extensions-nav-entry .chatgptx-extensions-item-state { display: none; }
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-group-head,
        .${chatgptxPageClass} .chatgptx-modal-content[data-chatgptx-active-tab="extensions"] .chatgptx-page-nav-empty { display: none; }
        .${chatgptxPageClass} .chatgptx-modal-content:is([data-chatgptx-active-tab="extensions"], [data-chatgptx-active-tab="sponsor"]) .chatgptx-modal-body { padding: 24px 16px 40px; }
      }
    `;
    document.documentElement.appendChild(style);
  }

  function defaultChatGPTXSettings() {
    // capacityAutoRetry* 必须写成字面量：多个测试（typing-effects-settings、whale-widget、
    // session-share-runtime…）把 defaultChatGPTXSettings 切片到不带 prelude 的沙箱里执行，
    // 引用 prelude 常量会直接 ReferenceError。与 00-prelude.js 里
    // capacityAutoRetryDefaultMaxAttempts / capacityAutoRetryDefaultIntervalSeconds 的一致性
    // 由 capacity-auto-retry.test.ts 的防漂移用例守住。
    return { pluginMarketplaceUnlock: true, modelWhitelistUnlock: true, sessionDelete: true, markdownExport: true, sessionShare: true, pasteFix: false, typingEffect: "off", threadIdBadge: false, conversationView: false, whaleWidget: false, conversationViewMaxWidth: conversationViewDefaultWidth, threadScrollRestore: true, nativeMenuPlacement: true, serviceTierControls: false, petRealMouseLook: false, activityOverlayRight: false, stepwise: false, answerOutline: false, usageStatusLine: true, capacityAutoRetry: false, capacityAutoRetryMaxAttempts: 15, capacityAutoRetryIntervalSeconds: 0 };
  }

  const chatgptxBackendSettingMap = {
    pluginMarketplaceUnlock: "codexAppPluginMarketplaceUnlock",
    modelWhitelistUnlock: "codexAppModelWhitelistUnlock",
    sessionDelete: "codexAppSessionDelete",
    markdownExport: "codexAppMarkdownExport",
    sessionShare: "codexAppSessionShare",
    threadIdBadge: "codexAppThreadIdBadge",
    conversationView: "codexAppConversationView",
    whaleWidget: "codexAppWhaleWidgetEnabled",
    threadScrollRestore: "codexAppThreadScrollRestore",
    nativeMenuPlacement: "codexAppNativeMenuPlacement",
    serviceTierControls: "codexAppServiceTierControls",
    petRealMouseLook: "codexAppPetRealMouseLook",
    activityOverlayRight: "codexAppActivityOverlayRight",
    stepwise: "codexAppStepwiseEnabled",
    answerOutline: "codexAppAnswerOutlineEnabled",
    usageStatusLine: "codexAppUsageStatusLine",
    capacityAutoRetry: "codexAppCapacityAutoRetry",
    pasteFix: "codexAppPasteFix",
    typingEffect: "codexAppTypingEffect",
  };
  const chatgptxBackendMappedSettings = new Set(Object.keys(chatgptxBackendSettingMap));

  function backendChatGPTXSettings() {
    const settings = {};
    Object.entries(chatgptxBackendSettingMap).forEach(([localKey, backendKey]) => {
      const value = chatgptxBackendSettings[backendKey];
      if (typeof value === "boolean" || typeof value === "string" || (value && typeof value === "object" && !Array.isArray(value))) {
        settings[localKey] = value;
      }
    });
    return settings;
  }

  function chatgptxSettings() {
    if (chatgptxBackendSettings.enhancementsEnabled === false) {
      return {
        pluginMarketplaceUnlock: false,
        modelWhitelistUnlock: false,
        sessionDelete: false,
        markdownExport: false,
        sessionShare: false,
        pasteFix: false,
        typingEffect: "off",
        threadIdBadge: false,
        conversationView: false,
        whaleWidget: false,
        conversationViewMaxWidth: conversationViewDefaultWidth,
        threadScrollRestore: false,
        nativeMenuPlacement: false,
        serviceTierControls: false,
        petRealMouseLook: false,
        activityOverlayRight: false,
        stepwise: false,
        answerOutline: false,
        usageStatusLine: false,
        capacityAutoRetry: false,
        capacityAutoRetryMaxAttempts: 15,
        capacityAutoRetryIntervalSeconds: 0,
      };
    }
    try {
      const settings = { ...defaultChatGPTXSettings(), ...JSON.parse(localStorage.getItem(chatgptxSettingsKey) || "{}"), ...backendChatGPTXSettings() };
      delete settings.customLayout;
      return settings;
    } catch {
      const settings = { ...defaultChatGPTXSettings(), ...backendChatGPTXSettings() };
      return settings;
    }
  }


  function setChatGPTXSetting(key, value) {
    const backendKey = chatgptxBackendSettingMap[key];
    if (backendKey) {
      if (key === "stepwise") syncStepwisePanel(value);
      if (key === "answerOutline") syncStepwisePanel(undefined, value);
      void setBackendSetting(backendKey, value).then(() => {
        if (key === "stepwise" || key === "answerOutline") {
          Promise.resolve(window.__codexStepwisePanel?.loadSettings?.()).then(() => syncStepwisePanel());
        }
        // 容量错误自动重试：开关落库后立即启停轮询，不必等下一次心跳。
        if (key === "capacityAutoRetry") syncChatGPTXCapacityAutoRetry();
      }).catch(() => {
        void loadBackendSettings();
      });
      return;
    }
    let stored = {};
    try {
      stored = JSON.parse(localStorage.getItem(chatgptxSettingsKey) || "{}");
    } catch {
      stored = {};
    }
    const next = { ...stored, [key]: value };
    localStorage.setItem(chatgptxSettingsKey, JSON.stringify(next));
    if (key === "threadScrollRestore" && !value) {
      clearTimeout(window.__codexThreadScrollSaveTimer);
      window.__codexThreadScrollSaveTimer = null;
      window.__codexThreadScrollRestoreRevision = (window.__codexThreadScrollRestoreRevision || 0) + 1;
      window.__codexThreadScrollSyncRevision = (window.__codexThreadScrollSyncRevision || 0) + 1;
      (window.__codexThreadScrollRestoreTimers || []).forEach((timer) => clearTimeout(timer));
      window.__codexThreadScrollRestoreTimers = [];
      (window.__codexThreadScrollSyncTimers || []).forEach((timer) => clearTimeout(timer));
      window.__codexThreadScrollSyncTimers = [];
      window.__codexThreadScrollRuntime = null;
    }
    if (key === "serviceTierControls") {
      if (value) {
        void loadCodexServiceTierState();
      } else {
        removeCodexServiceTierBadges();
        refreshCodexServiceTierControls();
      }
    }
    if (key === "stepwise") syncStepwisePanel(value);
    renderChatGPTXMenu();
    scan();
  }

  function syncStepwisePanel(
    enabled = chatgptxSettings().stepwise,
    answerOutlineEnabled = chatgptxSettings().answerOutline
  ) {
    try {
      window.__codexStepwisePanel?.syncSettings?.({
        enabled: !!enabled,
        answerOutlineEnabled: !!answerOutlineEnabled,
      });
    } catch (error) {
      sendChatGPTXDiagnostic("stepwise_sync_failed", {
        errorName: error?.name || "",
        errorMessage: error?.message || String(error),
      });
    }
  }

  function normalizeConversationViewWidth(value) {
    if (value === null || value === undefined || String(value).trim() === "") return null;
    const number = Number(value);
    if (!Number.isFinite(number)) return null;
    return Math.max(conversationViewMinWidth, Math.min(conversationViewMaxAllowedWidth, Math.round(number)));
  }

  function conversationViewWidth() {
    const settingsWidth = normalizeConversationViewWidth(chatgptxSettings().conversationViewMaxWidth);
    if (settingsWidth) return settingsWidth;
    const legacyWidth = normalizeConversationViewWidth(localStorage.getItem(conversationViewLegacyWidthKey));
    return legacyWidth || conversationViewDefaultWidth;
  }

  function refreshConversationViewControls() {
    const enabled = !!chatgptxSettings().conversationView;
    const width = conversationViewWidth();
    document.querySelectorAll("[data-chatgptx-conversation-view-width]").forEach((input) => {
      input.value = String(width);
      input.disabled = !enabled;
    });
  }

  function setConversationViewWidth(value) {
    const width = normalizeConversationViewWidth(value);
    if (!width) return;
    setChatGPTXSetting("conversationViewMaxWidth", width);
  }
