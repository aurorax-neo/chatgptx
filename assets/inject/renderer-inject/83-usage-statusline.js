  /**
   * 输入框下方用量状态栏：生成速度 / 缓存命中率 / 上下文占用。
   *
   * 数据全部来自 Codex 自己写的 rollout 记录（`event_msg.token_count`），中文档口径
   * 与 Codex 一致：缓存命中 = cached_input_tokens / input_tokens，上下文占用 =
   * last_token_usage.total_tokens / model_context_window。这里的 model_context_window
   * 正是 ChatGPTX 注入 model_catalog_json 后 Codex 认到的每模型窗口，所以按模型配置
   * 窗口的功能会直接反映在这一项上。
   *
   * 为什么不复用 `/thread-usage-history`：那条路由按会话整读 rollout 并解析全部行，
   * 真实会话的 rollout 可以到几百 MB，状态栏按秒级轮询扛不住。这里走
   * `/thread-usage-live`，宿主只读文件尾部。
   *
   * 节点挂在输入卡片（[data-composer-surface-variant]）之后、同一父容器内，也就是
   * 「输入框下方、卡片外」。节点带 data-chatgptx-ext，扫描调度器会把它排除在
   * 自喂循环之外（issue #1960）。
   */
  registerChatGPTXExtensionSelector('[data-chatgptx-ext="builtin-usage-statusline"]');
  window.__chatgptxUsageStatusLineRuntime?.dispose?.();

  const chatgptxUsageStatusLineId = "chatgptx-usage-statusline";
  const chatgptxUsageStyleId = "chatgptx-usage-statusline-style";
  /** 空闲轮询间隔；最近有 token 事件时切到活跃间隔。 */
  const chatgptxUsageIdleIntervalMs = 5000;
  /** 本轮在跑（最近有事件）时的间隔。 */
  const chatgptxUsageActiveIntervalMs = 1200;
  /** 已经拿到会话、但还没写出第一条 token 事件时的间隔。 */
  const chatgptxUsageWarmupIntervalMs = 2500;
  /** 最近一条 token 事件超过这个时间就认为本轮已经结束，速度值转为淡化显示。 */
  const chatgptxUsageStaleMs = 120000;
  /** 连续失败到这个次数后停掉轮询，避免桥接不可用时每秒写诊断日志。 */
  const chatgptxUsageFailureLimit = 5;

  const chatgptxUsageState = {
    disposed: false,
    timer: 0,
    node: null,
    parts: null,
    threadId: "",
    failures: 0,
    inFlight: false,
    lastFingerprint: "",
  };

  const chatgptxUsageSvgNs = "http://www.w3.org/2000/svg";

  function chatgptxUsageText() {
    const language = (document.documentElement?.lang || navigator.language || "").toLowerCase();
    return language.startsWith("zh")
      ? { speed: (value) => `${value} tok/s`, cache: (value) => `缓存命中 ${value}`, context: (value) => value, aria: "用量状态栏" }
      : { speed: (value) => `${value} tok/s`, cache: (value) => `cache hit ${value}`, context: (value) => value, aria: "Usage status line" };
  }

  function chatgptxUsageInstallStyle() {
    if (document.getElementById(chatgptxUsageStyleId)) return;
    const style = document.createElement("style");
    style.id = chatgptxUsageStyleId;
    style.dataset.chatgptxExt = "builtin-usage-statusline";
    style.textContent = `
      /* 颜色只用一层透明度：主题的 --color-token-text-secondary 本身就是 65% 透明，
         再叠容器 opacity 会掉到 0.3 出头，实测在 #141414 上几乎看不清。
         这里以主文字色取 82%，既够亮又略低于输入框文字。 */
      /* 三项作为一组居中，和 DSH 那条一致；两端对齐会把「上下文占用」甩到最右边，
         和速度、缓存拉得太开，读起来是两段信息。 */
      #${chatgptxUsageStatusLineId}{display:flex;align-items:center;justify-content:center;gap:24px;box-sizing:border-box;width:100%;margin:6px 0 0;padding:0 10px;font:inherit;font-size:12px;line-height:17px;color:var(--color-token-text-secondary,rgba(205,205,215,.95));color:color-mix(in srgb,var(--color-token-text-primary,#dfdfdf) 82%,transparent);user-select:none;cursor:default}
      #${chatgptxUsageStatusLineId}[data-hidden="true"]{display:none}
      #${chatgptxUsageStatusLineId} .chatgptx-usage-item{display:inline-flex;align-items:center;gap:5px;white-space:nowrap;font-variant-numeric:tabular-nums;transition:opacity .2s}
      #${chatgptxUsageStatusLineId} .chatgptx-usage-value{font-weight:500}
      #${chatgptxUsageStatusLineId} .chatgptx-usage-item svg{flex:0 0 auto;opacity:.8}
      /* 本轮结束超过阈值只淡化速度：缓存命中与上下文占用仍然是当前值，不该跟着变灰。 */
      #${chatgptxUsageStatusLineId}[data-stale="true"] .chatgptx-usage-item[data-usage="speed"]{opacity:.55}
    `;
    document.head?.appendChild(style);
  }

  /** 描边图标，尺寸与颜色都跟随文字，避免在浅色主题下露白。 */
  function chatgptxUsageIcon(shapes) {
    const svg = document.createElementNS(chatgptxUsageSvgNs, "svg");
    svg.setAttribute("viewBox", "0 0 16 16");
    svg.setAttribute("width", "13");
    svg.setAttribute("height", "13");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.4");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    const created = shapes.map((shape) => {
      const node = document.createElementNS(chatgptxUsageSvgNs, shape.tag);
      Object.entries(shape.attrs).forEach(([key, value]) => node.setAttribute(key, value));
      svg.appendChild(node);
      return node;
    });
    return { svg, created };
  }

  /** 上下文用进度环，环长 2πr；r=6 时周长约 37.7。 */
  const chatgptxUsageRingCircumference = 37.7;

  function chatgptxUsageRingIcon() {
    const icon = chatgptxUsageIcon([
      { tag: "circle", attrs: { cx: "8", cy: "8", r: "6", "stroke-opacity": ".3" } },
      { tag: "circle", attrs: { cx: "8", cy: "8", r: "6", "stroke-dasharray": `0 ${chatgptxUsageRingCircumference}`, transform: "rotate(-90 8 8)" } },
    ]);
    return { svg: icon.svg, ring: icon.created[1] };
  }

  function chatgptxUsageItem(node, key, label, icon) {
    const item = document.createElement("span");
    item.className = "chatgptx-usage-item";
    item.dataset.usage = key;
    item.appendChild(icon);
    const value = document.createElement("span");
    value.className = "chatgptx-usage-value";
    value.textContent = "--";
    item.appendChild(value);
    node.appendChild(item);
    return { item, value, label };
  }

  /** DOM 只建一次；之后每轮只改文本与环长，避免每秒重建节点触发扫描。 */
  function chatgptxUsageBuild(node) {
    node.textContent = "";
    const ring = chatgptxUsageRingIcon();
    const parts = {
      speed: chatgptxUsageItem(node, "speed", "speed", chatgptxUsageIcon([
        { tag: "path", attrs: { d: "M2 12.5a6 6 0 0 1 12 0" } },
        { tag: "path", attrs: { d: "M8 12.5 10.8 8.6" } },
      ]).svg),
      cache: chatgptxUsageItem(node, "cache", "cache", chatgptxUsageIcon([
        { tag: "path", attrs: { d: "M8 2.2c2.5 0 4.5.8 4.5 1.8S10.5 5.8 8 5.8 3.5 5 3.5 4 5.5 2.2 8 2.2Z" } },
        { tag: "path", attrs: { d: "M3.5 4v8c0 1 2 1.8 4.5 1.8s4.5-.8 4.5-1.8V4" } },
        { tag: "path", attrs: { d: "M3.5 8c0 1 2 1.8 4.5 1.8S12.5 9 12.5 8" } },
      ]).svg),
      context: chatgptxUsageItem(node, "context", "context", ring.svg),
      ring: ring.ring,
    };
    chatgptxUsageState.parts = parts;
    return parts;
  }

  function chatgptxUsageComposerCard() {
    const cards = [...document.querySelectorAll("[data-composer-surface-variant]")];
    let fallback = null;
    for (const card of cards) {
      if (!visibleElement(card) || card.closest('[data-chatgptx-ext]')) continue;
      fallback = card;
      // 首页可能同时挂着多个 composer 外壳；带真实输入框的那个才是当前会话。
      if (card.querySelector('.ProseMirror[contenteditable="true"], textarea:not(:disabled):not([readonly]), [role="textbox"][contenteditable="true"]')) {
        return card;
      }
    }
    return fallback;
  }

  /**
   * 当前会话 id。
   *
   * `[data-above-composer-conversation-id]` 是 Codex 自己挂在 composer 上的会话标识，
   * 比侧边栏选中行更贴近「这个输入框属于哪个会话」。侧边栏只作兜底，而且**只在
   * placement=thread 时用**：首页/新会话草稿的 composer 是 placement=home，此时侧边栏
   * 仍选中着上一个 thread，兜底会把上一个会话的读数挂到新会话上。
   * 新会话草稿的 id 形如 `local:client-new-thread:<uuid>`，还没有 rollout，直接当作无会话。
   */
  function chatgptxUsageThreadId(card) {
    const composerRoot = card?.closest("[data-codex-composer-root]");
    const fromComposer = composerRoot
      ?.querySelector("[data-above-composer-conversation-id]")
      ?.getAttribute("data-above-composer-conversation-id");
    const inThread = composerRoot?.getAttribute("data-composer-placement") === "thread";
    const fromSidebar = inThread
      ? document
        .querySelector('[data-app-action-sidebar-thread-selected="true"][data-app-action-sidebar-thread-id]')
        ?.getAttribute("data-app-action-sidebar-thread-id")
      : null;
    const id = String(fromComposer || fromSidebar || "").replace(/^local:/, "").trim();
    if (!id || id === "__proto__" || id === "prototype" || id === "constructor") return "";
    return /^[A-Za-z0-9_.-]{8,128}$/.test(id) ? id : "";
  }

  function chatgptxUsageEnsureNode(card) {
    const host = card?.parentElement;
    if (!host) return null;
    let node = chatgptxUsageState.node;
    if (node?.isConnected && node.previousElementSibling === card) return node;
    if (!node) {
      node = document.createElement("div");
      node.id = chatgptxUsageStatusLineId;
      node.setAttribute("data-chatgptx-ext", "builtin-usage-statusline");
      node.setAttribute("role", "status");
      chatgptxUsageState.node = node;
    }
    card.insertAdjacentElement("afterend", node);
    return node;
  }

  function chatgptxUsageHide() {
    const node = chatgptxUsageState.node;
    if (node) {
      // 只切属性、不摘节点：新会话在第一条 token 事件写出前会反复走空数据路径，
      // 每秒增删一次节点既浪费又会让 Codex 的 MutationObserver 空转。
      node.dataset.hidden = "true";
    }
    chatgptxUsageState.lastFingerprint = "";
  }

  function chatgptxUsageShow(node) {
    if (node.dataset.hidden !== "true") return;
    node.dataset.hidden = "false";
    chatgptxUsageState.lastFingerprint = "";
  }

  function chatgptxUsagePercent(numerator, denominator) {
    if (!(denominator > 0) || !(numerator >= 0)) return null;
    return Math.min(100, Math.max(0, (numerator / denominator) * 100));
  }

  /**
   * 生成速度（token/s）。
   *
   * Codex 只在每次模型调用结束时写一条 token_count，事件间隔里同时包含生成和工具
   * 执行时间，所以这是「吞吐」而不是纯生成速度，会比真实解码速度低。用同一轮内
   * 相邻事件的累计输出差除以时间差得到区间速度，再取最近几个区间平均，避免单次
   * 抖动。换轮时 turn_output_tokens 会回落，靠这个切段。
   */
  function chatgptxUsageRate(events) {
    const rates = [];
    for (let index = 1; index < events.length; index += 1) {
      const previous = events[index - 1];
      const current = events[index];
      const previousOutput = Number(previous.turn_output_tokens) || 0;
      const currentOutput = Number(current.turn_output_tokens) || 0;
      if (currentOutput <= previousOutput) continue;
      const startedAt = Date.parse(previous.t);
      const endedAt = Date.parse(current.t);
      const seconds = (endedAt - startedAt) / 1000;
      // 间隔过短说明是同一批写入，过长说明中间停了很久，都不适合当速度样本。
      if (!(seconds >= 0.5) || seconds > 120) continue;
      rates.push((currentOutput - previousOutput) / seconds);
    }
    if (!rates.length) return null;
    const recent = rates.slice(-3);
    return recent.reduce((total, rate) => total + rate, 0) / recent.length;
  }

  function chatgptxUsageSnapshot(events) {
    if (!Array.isArray(events) || !events.length) return null;
    const latest = events[events.length - 1];
    const observedAt = Date.parse(latest.t);
    return {
      speed: chatgptxUsageRate(events),
      cache: chatgptxUsagePercent(Number(latest.cached_input_tokens), Number(latest.input_tokens)),
      context: chatgptxUsagePercent(Number(latest.total_tokens), Number(latest.context_window)),
      stale: Number.isFinite(observedAt) ? Date.now() - observedAt > chatgptxUsageStaleMs : false,
      modelContextWindow: Number(latest.context_window) || 0,
    };
  }

  /** `snapshot` 为 null 时渲染占位符：会话已确定但还没有第一条 token 事件。 */
  function chatgptxUsageRender(snapshot) {
    const node = chatgptxUsageState.node;
    if (!node?.isConnected) return;
    const parts = chatgptxUsageState.parts || chatgptxUsageBuild(node);
    const text = chatgptxUsageText();
    const format = (value) => (value === null || value === undefined || Number.isNaN(value) ? "--" : value);
    const speed = snapshot?.speed == null ? "--" : String(Math.max(0, Math.round(snapshot.speed)));
    const cache = snapshot?.cache == null ? "--" : `${snapshot.cache.toFixed(1)}%`;
    const context = snapshot?.context == null ? "--" : `${Math.round(snapshot.context)}%`;
    const stale = snapshot?.stale === true;
    const contextWindow = snapshot?.modelContextWindow || 0;
    // 指纹一样就不写 DOM：每秒一次无意义赋值会让 Codex 的 MutationObserver 空转。
    const fingerprint = [speed, cache, context, stale, contextWindow].join("|");
    if (fingerprint === chatgptxUsageState.lastFingerprint) return;
    chatgptxUsageState.lastFingerprint = fingerprint;

    parts.speed.value.textContent = text.speed(speed);
    parts.cache.value.textContent = text.cache(cache);
    parts.context.value.textContent = text.context(context);
    if (parts.ring) {
      const filled = snapshot?.context == null ? 0 : (snapshot.context / 100) * chatgptxUsageRingCircumference;
      parts.ring.setAttribute("stroke-dasharray", `${filled.toFixed(2)} ${chatgptxUsageRingCircumference}`);
    }
    node.dataset.stale = String(stale);
    node.setAttribute("title", [
      `生成速度 ${format(speed)} tok/s`,
      `缓存命中 ${format(cache)}`,
      `上下文占用 ${format(context)}`,
      contextWindow > 0 ? `窗口 ${contextWindow}` : "",
    ].filter(Boolean).join(" · "));
    node.setAttribute("aria-label", `${text.aria}：${text.speed(speed)}，${text.cache(cache)}，${context}`);
  }

  function chatgptxUsageSchedule(delay) {
    if (chatgptxUsageState.disposed) return;
    clearTimeout(chatgptxUsageState.timer);
    chatgptxUsageState.timer = window.setTimeout(chatgptxUsageTick, delay);
  }

  function chatgptxUsageEnabled() {
    return !chatgptxUsageState.disposed
      && chatgptxBackendSettingsLoaded
      && chatgptxSettings().usageStatusLine === true;
  }

  async function chatgptxUsageRefresh(threadId) {
    if (chatgptxUsageState.inFlight) return null;
    chatgptxUsageState.inFlight = true;
    try {
      const result = await postJson("/thread-usage-live", { session_id: threadId });
      chatgptxUsageState.failures = 0;
      if (result?.status !== "ok" || !Array.isArray(result.events)) return null;
      return result.events;
    } finally {
      chatgptxUsageState.inFlight = false;
    }
  }

  async function chatgptxUsageTick() {
    if (chatgptxUsageState.disposed) return;
    if (!chatgptxUsageEnabled()) {
      chatgptxUsageHide();
      chatgptxUsageSchedule(chatgptxUsageIdleIntervalMs);
      return;
    }
    // 页面不可见时不拉数据，但保留轮询，切回来下一拍就会刷新。
    if (document.hidden) {
      chatgptxUsageSchedule(chatgptxUsageIdleIntervalMs);
      return;
    }
    const card = chatgptxUsageComposerCard();
    const threadId = chatgptxUsageThreadId(card);
    if (!card || !threadId) {
      chatgptxUsageHide();
      chatgptxUsageState.threadId = "";
      chatgptxUsageSchedule(chatgptxUsageIdleIntervalMs);
      return;
    }
    chatgptxUsageInstallStyle();
    const node = chatgptxUsageEnsureNode(card);
    if (!node) {
      chatgptxUsageSchedule(chatgptxUsageIdleIntervalMs);
      return;
    }
    // 换会话时清掉上一个会话的读数，避免把旧数字挂在新会话上。
    if (chatgptxUsageState.threadId !== threadId && chatgptxUsageState.node === node) {
      chatgptxUsageState.threadId = threadId;
      chatgptxUsageState.lastFingerprint = "";
    }
    let events = null;
    try {
      events = await chatgptxUsageRefresh(threadId);
    } catch (error) {
      chatgptxUsageState.failures += 1;
      if (chatgptxUsageState.failures === chatgptxUsageFailureLimit) {
        sendChatGPTXDiagnostic("usage_statusline_bridge_failed", {
          errorName: error?.name || "",
          errorMessage: error?.message || String(error),
        });
      }
    }
    if (chatgptxUsageState.disposed) return;
    const snapshot = events ? chatgptxUsageSnapshot(events) : null;
    if (snapshot) {
      chatgptxUsageShow(node);
      chatgptxUsageRender(snapshot);
      // 刚有事件说明本轮在跑，加密轮询；本轮已经结束就拉长间隔。
      chatgptxUsageSchedule(snapshot.stale ? chatgptxUsageIdleIntervalMs : chatgptxUsageActiveIntervalMs);
      return;
    }
    if (chatgptxUsageState.failures >= chatgptxUsageFailureLimit) {
      // 桥接连续失败：整条隐藏，退到慢轮询，等宿主恢复。
      chatgptxUsageHide();
      chatgptxUsageSchedule(30000);
      return;
    }
    // 会话已经确定、只是还没写出 token 事件（新会话第一轮）：先显示占位符，
    // 用较慢的间隔等第一条数据，避免空转。
    chatgptxUsageShow(node);
    chatgptxUsageRender(null);
    chatgptxUsageSchedule(chatgptxUsageWarmupIntervalMs);
  }

  chatgptxUsageInstallStyle();
  chatgptxUsageSchedule(600);
  window.__chatgptxUsageStatusLineRuntime = {
    version: 1,
    sync: () => chatgptxUsageSchedule(0),
    dispose() {
      chatgptxUsageState.disposed = true;
      clearTimeout(chatgptxUsageState.timer);
      chatgptxUsageState.timer = 0;
      // 卸载时把节点整个摘掉：注入会被重跑，留一个隐藏节点等于每次重注入多挂一个。
      chatgptxUsageState.node?.remove();
      chatgptxUsageState.node = null;
      chatgptxUsageState.parts = null;
      chatgptxUsageState.lastFingerprint = "";
      globalThis.__CHATGPTX_USAGE_STATUSLINE_SYNC__ = undefined;
    },
  };
  globalThis.__CHATGPTX_USAGE_STATUSLINE_SYNC__ = () => chatgptxUsageSchedule(0);
