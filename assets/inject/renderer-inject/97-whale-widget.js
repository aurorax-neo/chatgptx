  // 完整版鲸鱼由上游原始引擎 + Codex 数据适配层组成；默认关闭。
  window.__chatgptxWhaleWidgetRuntime?.dispose?.();
  const chatgptxWhaleState = { disposed: false, runtime: null, profile: "", missingSource: false, failedUntil: 0, notice: null };
  function chatgptxWhaleEnabled() {
    return !chatgptxWhaleState.disposed && chatgptxBackendSettingsLoaded && chatgptxSettings().whaleWidget === true;
  }
  function chatgptxWhaleStop() {
    chatgptxWhaleState.runtime?.dispose();
    chatgptxWhaleState.runtime = null;
    chatgptxWhaleState.notice?.remove(); chatgptxWhaleState.notice = null;
  }
  function chatgptxWhaleInitializationFailed(runtime) {
    runtime?.dispose(); chatgptxWhaleState.runtime = null; chatgptxWhaleState.failedUntil = Date.now() + 10000;
    if (chatgptxWhaleState.notice) return;
    const notice = document.createElement("div"); notice.setAttribute("data-chatgptx-ext", "whale-widget"); notice.setAttribute("role", "status");
    notice.style.cssText = "position:fixed;right:20px;bottom:20px;z-index:2147483600;max-width:320px;padding:12px;border-radius:10px;background:#fff5ec;color:#7c3218;font:12px/1.6 system-ui;box-shadow:0 3px 16px #0002";
    notice.textContent = "鲸鱼挂件暂时无法加载，10 秒后自动重试。可在 ChatGPTX 增强设置关闭后重新开启。";
    document.body.appendChild(notice); chatgptxWhaleState.notice = notice;
  }
  function chatgptxWhaleProfileFingerprint() {
    const profile = codexRemoteSessionActiveProfile?.();
    const settings = chatgptxBackendSettings || {};
    return JSON.stringify([profile?.id || "", profile?.relayMode || "", settings.codexAppWhaleBalanceProtocol,
      settings.codexAppWhaleBalancePath, settings.codexAppWhaleBalanceField, settings.codexAppWhaleBalanceCurrency,
      settings.codexAppWhaleBalanceScale]);
  }
  function syncChatGPTXWhaleWidget() {
    if (!chatgptxWhaleEnabled()) { chatgptxWhaleStop(); chatgptxWhaleState.failedUntil = 0; return; }
    const profile = chatgptxWhaleProfileFingerprint();
    if (chatgptxWhaleState.profile !== profile) { chatgptxWhaleStop(); chatgptxWhaleState.failedUntil = 0; }
    chatgptxWhaleState.profile = profile;
    if (chatgptxWhaleState.runtime) {
      chatgptxWhaleState.runtime.context();
      chatgptxWhaleState.runtime.subscribe();
      chatgptxWhaleState.runtime.pause(document.hidden === true);
      return;
    }
    // 隐藏窗口等恢复可见后再挂载，不在后台构造音频或轮询会话日志。
    if (document.hidden) return;
    if (Date.now() < chatgptxWhaleState.failedUntil) return;
    if (typeof window.__CHATGPTX_WHALE_ENGINE__ !== "function") {
      if (!chatgptxWhaleState.missingSource) console.warn("[ChatGPTX] 完整鲸鱼运行时未随启动器加载，请重新构建启动器。");
      chatgptxWhaleState.missingSource = true; chatgptxWhaleInitializationFailed(null); return;
    }
    const runtime = createChatGPTXWhaleFullRuntime();
    chatgptxWhaleState.runtime = runtime;
    registerChatGPTXExtensionSelector("[data-chatgptx-ext=whale-widget]");
    void runtime.initialize().then(() => {
      if (runtime.active() && chatgptxWhaleState.runtime === runtime && chatgptxWhaleEnabled()) {
        window.__CHATGPTX_WHALE_ENGINE__(...runtime.environment);
        if (runtime.state.initializationError || !runtime.state.controls) throw new Error("完整鲸鱼初始化未完成");
        chatgptxWhaleState.notice?.remove(); chatgptxWhaleState.notice = null;
      }
    }).catch((error) => {
      if (!runtime.active()) return;
      if (chatgptxWhaleState.runtime === runtime) chatgptxWhaleInitializationFailed(runtime);
    });
  }
  window.__chatgptxWhaleWidgetRuntime = {
    version: 3,
    sync: syncChatGPTXWhaleWidget,
    dispose() { chatgptxWhaleStop(); chatgptxWhaleState.disposed = true; },
    get state() { return chatgptxWhaleState.runtime?.state || null; },
  };
