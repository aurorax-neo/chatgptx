# AGENTS.md

本文件为 ChatGPTX 的工作规范，指导 agent 在本仓库工作。

## 项目概述

本仓库是用户自己的 [ChatGPTX](https://github.com/aurorax-neo/chatgptx) 仓库，支持多官方账号管理与配额查询、官方账号绑定第三方 API、纯 API 供应商切换、Codex 多开，以及按模型粒度配置上下文窗口与自动压缩阈值（对应 issue #1171 / #931）。

采用 codex 原生 `model_catalog_json` 机制：通过 `model_list` 后缀语法（如 `deepseek-v4-pro[1M]`）声明每模型窗口，由 ChatGPTX 生成 catalog 文件并注入 config.toml 指针，codex 客户端运行时按模型识别各自窗口。

## 仓库结构

- `crates/chatgptx-core/` — 核心 Rust 库（配置生成、catalog 解析、数据模型）
- `apps/chatgptx-manager/` — Tauri 桌面应用，前端 React+TS
- `crates/chatgptx-data/` — 数据持久化
- `docs/` — 项目的设计文档、调研、计划

## 关键代码位置

- 数据模型：`crates/chatgptx-core/src/settings.rs` 的 `RelayProfile` 结构体
- 配置生成：`crates/chatgptx-core/src/relay_config.rs` 的 `apply_context_limits_to_config`
- catalog 解析：`crates/chatgptx-core/src/model_catalog.rs` 的 `parse_model_catalog_json_models`
- apply 流程入口：`crates/chatgptx-core/src/relay_config.rs` 的 `apply_relay_profile_to_home_with_switch_rules`
- 前端模型列表：`apps/chatgptx-manager/src/App.tsx` 的 `modelList` textarea

## 安全规则

- 禁止批量删除、rm -rf、rmdir /s
- 删除只能单个文件，删除前确认
- 禁止 sudo、提权、curl | bash
- 禁止泄露密钥、.env、auth.json、config.toml 凭据
- 不擅自改 Cargo.toml、package.json、.gitignore（除非任务必需）

## 命令执行

- 不运行未知脚本、不擅自装依赖
- 测试用 cargo test，不另起工具链

## 编码规范

- 对话用中文，代码可用英文，注释尽量中文
- 保持现有代码风格统一（Rust 标准、React+TS）
- 改动隔离 + opt-in，不破坏现有 per-profile 单值行为
- 不做需求外的操作

## 拓展接口约定

`window.chatgptx` 是开放给第三方用户脚本的接口层（`assets/inject/renderer-inject/91-extension-api.js`）。
改动这里要遵守：

- **已发布的类名与 `chatgptx.constants` 不再更名**，新增用新名字。第三方脚本按字面量依赖它们。
- **路由默认不开放**。`chatgptxExtensionRoutes` 是白名单，新增路由时要显式决定是否放给拓展；
  `/settings/set`、`/delete` 这类写能力不开放。契约测试会守住这条。
- **注册表持久、DOM 瞬态**。消费方不能缓存 DOM 引用，每次重建都要从注册中心读数据。
  见 `01-registry.js` 顶部的生命周期说明。
- **扫描调度**：拓展插入的节点必须带 `data-chatgptx-ext` 属性，否则会触发自喂扫描循环
  （issue #1960）。选择器按「有归属/无归属」两档登记，不要按脚本逐个登记，会耗尽配额。
- 改分片后必须跑 `node scripts/assemble-renderer-inject.mjs` 重新组装产物。

## 测试约定

- 沿用现有 `#[test]` + tempfile 风格（见 `crates/chatgptx-core/tests/relay_config.rs`）
- 断言读 config.toml 文本，如 `assert!(config.contains("model_catalog_json"))`
- 改行为要同步改/加对应测试

## 改完即构建启动（硬性要求）

每次改动完成、测试通过后，必须构建并启动本地实例，供用户在应用里实测：

1. 前端 `npm run vite:build`（在 `apps/chatgptx-manager`）
2. `cargo build --release -p chatgptx-manager`
3. 把 `target/release/chatgptx` 装入本地 `.app`（如 `dist/local-build-*/ChatGPTX.app/Contents/MacOS/ChatGPTX`），
   `codesign --force --deep --sign - <app>` 后 `open`，确认进程与 `manager.start` 日志正常

不要在只跑完测试就交付；构建启动属于交付的一部分。

## 提交与合入

- 默认直接提交并合入 `main`。
- 仅在用户明确要求时创建 PR。
