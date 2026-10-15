<p align="center">
  <img src="docs/images/readme-preview/hero.svg" alt="ChatGPTX — 让 Codex 更顺手。多官方账号、供应商与模型、Codex 多开、工作流增强。" width="100%">
</p>

<p align="center">
  <strong>让 Codex 桌面应用更顺手。</strong><br>
  统一管理多官方账号、第三方 API 供应商与模型，支持独立实例多开与工作区增强。
</p>

<p align="center">
  <a href="https://github.com/aurorax-neo/chatgptx/releases"><img src="https://img.shields.io/github/v/release/aurorax-neo/chatgptx?style=flat-square&amp;color=7387ff&amp;label=release" alt="最新版本"></a>
  <a href="https://github.com/aurorax-neo/chatgptx/stargazers"><img src="https://img.shields.io/github/stars/aurorax-neo/chatgptx?style=flat-square&amp;color=e8b86d" alt="GitHub Stars"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-838da8?style=flat-square" alt="许可证 AGPL-3.0-only"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-838da8?style=flat-square" alt="支持 Windows 和 macOS">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20Tauri-838da8?style=flat-square" alt="使用 Rust 与 Tauri 构建">
</p>

<p align="center">
  <a href="https://github.com/aurorax-neo/chatgptx/releases"><strong>下载最新版 ↗</strong></a> &nbsp; · &nbsp;
  <a href="#功能一览">功能一览</a> &nbsp; · &nbsp;
  <a href="#快速上手">快速上手</a> &nbsp; · &nbsp;
  <a href="#官方账号与供应商模式">账号与供应商模式</a> &nbsp; · &nbsp;
  <a href="https://github.com/aurorax-neo/chatgptx/issues">反馈问题</a> &nbsp; · &nbsp;
  <a href="README_EN.md">English</a>
</p>

ChatGPTX 是面向 OpenAI Codex / ChatGPT 桌面应用的统一管理工具，基于 Rust + Tauri 构建，通过 CDP 与内置运行服务提供多官方账号管理与切换、第三方 API 供应商切换、协议转换、独立实例多开、会话管理和界面增强。它不修改官方应用的 `app.asar`，也不向安装目录写入补丁文件。

## 功能一览

<table>
  <tr>
    <td width="50%" valign="top">
      <h3>👤 多官方账号管理</h3>
      支持 OAuth PKCE 浏览器登录、手动回调解析、从本机 <code>auth.json</code> / macOS Keychain 导入，以及 JSON / Token 批量导入导出；支持实时查询 5 小时 / 周 / 月配额、Token 自动刷新与免重登一键切换，并可按账号独立绑定第三方 API 供应商。
    </td>
    <td width="50%" valign="top">
      <h3>🔌 纯 API 与聚合供应商</h3>
      支持纯 API 供应商及聚合供应商（故障转移 / 模型聚合），内置 Responses / Chat Completions 协议转换、模型连通测试与 Provider Doctor；点击「使用」可自由选择「仅使用（重启生效）」或「使用并重启 Codex」。
    </td>
  </tr>
  <tr>
    <td valign="top">
      <h3>🧵 Codex 多开（官方账号 &amp; 供应商）</h3>
      同时支持「官方账号」（纯官方或绑定了第三方 API 供应商）与「API 供应商」启动相互隔离的 Codex 实例：独立配置、独立会话和独立用户数据目录；运行中的实例集中列出，可单独重启或结束，与顶栏主 Codex 控制互不干扰。
    </td>
    <td valign="top">
      <h3>🧠 每个模型，各有空间</h3>
      按模型设置上下文窗口与自动压缩阈值（如 <code>deepseek-v4-pro[1M]</code>），自动生成 <code>model_catalog_json</code>；支持模型元数据导入，以及按供应商选择启用的 MCP、Skill 和 Plugin。
    </td>
  </tr>
  <tr>
    <td valign="top">
      <h3>💬 会话管理与数据维护</h3>
      本地会话扫描、删除与撤销、批量管理、Markdown 导出、Token 历史统计、项目移动、线程 ID 显示以及跨供应商历史会话同步修复。
    </td>
    <td valign="top">
      <h3>🧩 拓展市场与全局代理</h3>
      浏览与管理社区用户脚本（支持热重载）、MCP、Skills 与 Codex 插件市场；「设置」页内置全局网络代理配置（跟随系统 / 自定义代理 / 直连），统一作用于官方账号登录配额、插件市场与 Codex 启动。
    </td>
  </tr>
  <tr>
    <td valign="top">
      <h3>✨ 输入与阅读体验增强</h3>
      语音输入（兼容 API / 本地 SenseVoiceSmall 模型）、粘贴修复、对话视图宽度自适应、滚动位置恢复、下一步建议、回答大纲与光标打字特效。
    </td>
    <td valign="top">
      <h3>📊 用量状态栏与挂件</h3>
      输入框下方实时显示生成速度（tok/s）、缓存命中率和上下文占用；可选开启用量挂件与桌宠角色，查看会话用量与预算提醒。
    </td>
  </tr>
</table>

## 快速上手

### 1. 下载安装包

前往 [GitHub Releases](https://github.com/aurorax-neo/chatgptx/releases)，选择对应平台安装包：

| 平台与架构 | 安装包 |
| --- | --- |
| Windows · x64 | `ChatGPTX-*-windows-x64-setup.exe` |
| macOS · Apple Silicon / Intel (Universal) | `ChatGPTX-*-macos-universal.dmg` |

ChatGPTX 配合官方桌面应用使用，首次启动前请确认本机已安装对应的 Codex / ChatGPT 桌面应用。

### 2. 配置账号或供应商

打开 **ChatGPTX**：
- **使用官方账号**：进入「官方账号」页面，通过浏览器登录或从本机导入账号；可直接使用官方原生模型，或在账号卡片右下角「绑定供应商」下拉框中选择第三方 API 供应商结合使用。
- **使用纯 API 供应商**：进入「供应商配置」页面，新建或选择纯 API 供应商，填写 Base URL、API Key 与模型列表。
- **切换生效方式**：在任意账号或供应商卡片点击「使用」，在弹窗中选择「仅使用（重启生效）」或「使用并重启 Codex」。

### 3. 启动、停止与多开

- **主实例控制**：通过界面右上角「启动 Codex」、「停止 Codex」、「重启 Codex」控制主 Codex 实例。
- **多实例隔离运行**：进入「Codex多开」页面，可选择任意已登录官方账号或 API 供应商启动独立 Codex 实例，各实例互不共享会话与登录态。

## 官方账号与供应商模式

| 入口与模式 | 适用场景 | 凭证与配置隔离边界 |
| --- | --- | --- |
| **官方账号（不绑定供应商）** | 直接使用 ChatGPT / Codex 官方账号与订阅配额 | 写入官方 OAuth Token 至 `auth.json` 与 macOS Keychain，清理自定义模型覆盖，展示官方原生模型列表 |
| **官方账号（绑定第三方供应商）** | 保留官方账号登录状态与插件市场，模型请求走绑定的兼容 API | 同时写入该官方账号的 OAuth 凭证与所绑定供应商的 API 配置 |
| **供应商配置（纯 API）** | 仅使用自定义 Base URL 与 API Key，无需官方账号 | 自动停用官方账号 OAuth Token，写入独立 `OPENAI_API_KEY` 与 `config.toml`，与官方账号激活态严格互斥 |
| **聚合供应商** | 在多个成员间故障转移、按权重轮转，或将多成员模型合并为统一目录 | 支持「故障转移」与「模型聚合」两种类型，模型选择器支持按供应商分组折叠 |

### 按模型配置上下文窗口

在供应商的「模型列表」中使用窗口后缀声明单模型上下文窗口：

```text
deepseek-v4-pro[1M]
your-model[200K]
another-model[128000]
```

支持 `1M`、`200K` 或纯数字。ChatGPTX 会自动生成 `model_catalog_json` 并写入 `config.toml`，让 Codex 按当前模型识别对应窗口与自动压缩阈值；未带后缀的模型继续沿用供应商级默认窗口配置。

## 数据目录

设置了 `CODEX_HOME` 环境变量时，Codex 主目录以该变量为准；否则默认使用 `~/.codex`。

| 数据 | 路径 |
| --- | --- |
| Codex 配置文件 | `~/.codex/config.toml` |
| Codex 认证状态 | `~/.codex/auth.json`（macOS 同步写入 Keychain `"Codex Auth"`） |
| Codex 本地数据库 | 优先 `~/.codex/sqlite/*.db`，旧版回退 `~/.codex/state_5.sqlite` |
| ChatGPTX 状态与日志 | `~/.chatgptx/`（含 `settings.json`、`official-accounts.json`、`chatgptx.log`） |
| Codex 多开实例隔离目录 | `~/.chatgptx/codex-multi/<instance-id>/` |

## 开发与构建

```bash
# 1. 前端检查、测试与构建
cd apps/chatgptx-manager
npm ci
npm run check
npm test
npm run vite:build

# 2. Rust 测试与 Release 构建（回到仓库根目录）
cd ../..
cargo test --workspace --lib
cargo build --release -p chatgptx-manager --bin chatgptx
```

修改 `assets/inject/renderer-inject/` 下的分片后，需运行 `node scripts/assemble-renderer-inject.mjs` 重新组装 `assets/inject/renderer-inject.js`。

## 鸣谢与开源许可合规

### 1. 致谢（Credits）
- **[BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus)**（`AGPL-3.0-only`）：本项目基于该仓库衍生开发，继承其核心启动器架构、供应商切换、协议转换、模型目录生成与 Codex 界面增强机制。
- **[jlcodes99/cockpit-tools](https://github.com/jlcodes99/cockpit-tools)**（`CC-BY-NC-SA-4.0`）：多官方账号 OAuth PKCE 登录管理、Token 自动刷新、实时配额查询与全局网络代理交互设计参考自该项目。
- **[MeteorNOX/DeepSeek-Balance-Whale-Widget](https://github.com/MeteorNOX/DeepSeek-Balance-Whale-Widget)**（代码 `MIT`）与 **[Yang-huai406/Codex-Whale-Public](https://github.com/Yang-huai406/Codex-Whale-Public)**：内置用量挂件与鲸鱼角色相关代码及素材来源，详见 [上游 LICENSE](assets/inject/upstream/whale-widget/LICENSE) 与 [PROVENANCE.md](assets/inject/upstream/whale-widget/PROVENANCE.md)。

### 2. 许可证与合规声明（License & Compliance）
- **开源许可证**：本项目基于 [GNU Affero General Public License v3.0](LICENSE)（SPDX: `AGPL-3.0-only`）开源，完整第三方组件与来源声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
- **版权声明**：
  - `Copyright (C) 2026 BigPizzaV3 and contributors`（上游基础代码）
  - `Copyright (C) 2026 aurorax-neo`（ChatGPTX 衍生修改与新增实现）
- **使用边界与免责声明**：
  - 分发修改版或通过网络提供服务时，必须遵循 AGPL-3.0 第 5 条与第 13 条公开完整对应源代码并保留全部版权与许可声明；涉及 `cockpit-tools`（`CC-BY-NC-SA-4.0`）参考部分仅供个人学习、研究与非商业用途。
  - OpenAI、ChatGPT、Codex 相关商标与官方客户端归各自权利人所有，本项目与 OpenAI 无任何隶属或官方背书关系。

---

<p align="center">
  <img src="docs/images/chatgptx.png" alt="ChatGPTX" width="48"><br>
  <strong>ChatGPTX</strong> · 让 Codex 更顺手。<br>
  <sub><a href="https://github.com/aurorax-neo/chatgptx">github.com/aurorax-neo/chatgptx</a> · <a href="LICENSE">AGPL-3.0-only</a> · <a href="THIRD_PARTY_NOTICES.md">Third-Party Notices</a></sub>
</p>
