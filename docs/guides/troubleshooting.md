---
title: 排障指南
description: 按启动、环境、登录、Routes、日志和测试症状定位问题。
type: guide
audience: user-and-contributor
status: current
updated: 2026-08-27
---

# 排障指南

先确认运行形态，再看 doctor 和日志。不要把浏览器 mock 的状态当成桌面运行时的真实状态，也不要在 issue 或日志中粘贴完整 key、token、prompt 或凭据文件。

## 启动失败或页面空白

1. 浏览器演示执行 `pnpm dev:mock`。
2. 真实功能执行 `pnpm tauri:dev`，确认 Rust/Tauri 系统依赖已安装。
3. 若运行 `pnpm dev` 后在浏览器中看到 unavailable，这是预期的：该命令选择 Tauri adapter，浏览器没有 Tauri runtime。
4. 端口 `5173` 被占用时释放旧进程；Vite 配置是 `strictPort`，不会自动换端口。

生产构建失败时先执行 `pnpm typecheck`，再看 module graph guard 报出的路径。`src/dev`、`src/test`、`*.test.*` 和 `*.spec.*` 不得进入 `pnpm build`。

## Agent 未检测到或安装失败

```text
cargo run -p agenthub-cli -- doctor
cargo run -p agenthub-cli -- env list
cargo run -p agenthub-cli -- agent list
```

检查 binary 是否在 PATH、Runtime 是否就绪以及 install channel 是否适用于宿主平台。缺少 Node/npm 等共享 Runtime 时，先用 `env install <runtime>` 或按输出的 remediation 手工安装；`agent install` 不应假装成功，也不会自动卸载共享 Runtime。

### Codex 安装

| 现象 | 常见原因 | 处理 |
| --- | --- | --- |
| macOS/Linux 无「官方脚本」渠道 | OpenAI 仅发布 Windows PowerShell 安装脚本 | 使用 npm 全局安装：`npm i -g @openai/codex`，或先在 Agents 页一键装 Node/npm |
| `env.not_ready` / 无法一键安装 | 缺少 Node.js 或 npm | Agents 页打开环境面板，或 `agenthub agent install codex --install-deps` |
| 命令成功但仍显示未安装 | npm 全局 bin 不在 GUI 进程 PATH | **完全退出并重启 AgentHub**；即使当前 PATH 里没有 npm，也会读用户 npm 配置的全局目录并扫描常见位置 |
| `EACCES` / 权限错误 | 全局 npm 无写权限 | 配置用户级 npm prefix（如 `npm config set prefix ~/.npm-global`）后重装 |
| Chat 里看不到 Codex | Agent 被软隐藏 | Agents 卡显示「已隐藏」；取消隐藏即可（安装/检测不受影响） |
| 已装 VS Code 插件或桌面 App | 非缺陷：Chat 直接 spawn CLI | 需检测到 `codex` 二进制且 `~/.codex/auth.json` 有效；见 [Chat 与 Agent](../concepts/chat-and-agents.md#codex-外部安装) |

AgentHub 一键 npm 安装写到 `~/.npm-global`（Windows 为 `%APPDATA%\npm`），**不会**装进 `~/.agenthub`。那里若还有旧的 `npm` 目录，只是遗留，不能当启动路径。

需要完全退出时：若设置了「关闭到托盘」，关窗口不会退出，请从托盘选退出后再打开。

完整审查清单见 [Codex 安装与模块化审查](../status/codex-install-modularity-review.md)。

### WorkBuddy 安装

| 现象 | 常见原因 | 处理 |
| --- | --- | --- |
| 点安装后打开官网 | WorkBuddy 没有脚本安装，只打开官网安装页 | 这是指引，不是安装失败。在官网装完后，从托盘退出并重新打开 AgentHub |
| 失败面板全是下载进度 | 旧版把 npm HTTP 当正文 | 现行失败面板先显示诊断，下载进度会折叠 |

### Cursor / Kimi 登录

| 现象 | 常见原因 | 处理 |
| --- | --- | --- |
| 侧栏/连接页看不到 Cursor | dev 线 store-stamp 默认软隐藏 | Agents 管理页找到 Cursor Agent 卡片，点「取消隐藏」。兼容修复完成前产品面暂不开放 |
| Cursor 点「用这份登录」后出现中文错误 | Cursor 没有稳定的本机登录文件可写 | 用 Cursor 自己的登录，或设置 `CURSOR_API_KEY`。登录仍可保存在登录列表 |
| 保存第二张登录后第一张进了回收站 | 旧版会按同一把钥匙合并 | 现行不会因同一把钥匙悄悄删除另一张 |
| Kimi 对话失败、模型不在配置里 | 本机 `config.toml` 缺模型表 | 再切换一次该登录，会写出带 `kimi-k2` 的完整配置 |

## 登录或配置问题

- 先在 Connections 检查登录状态和 health，再尝试重新导入本机正在用的配置。
- 切换同一工具的登录会先备份、再写本机配置，成功 toast 会说明已写入本机配置。接到本机路由不会改对方工具的本机文件。失败应查看备份和日志，Cursor 会给出中文原因。
- 「使用官方服务」勾选后仍可粘贴做智能识别。高级编辑器不回显明文钥匙。
- CLI 使用 `account add-apikey --key -` 从 stdin 读取 key，避免把 key 放进 shell history。
- OAuth 浏览器流程以 GUI 为主；CLI 的 `account oauth-url` 只打印授权 URL，不能替代完整本机回调流程。
- 当前项目沿用既有凭据存储方案，不规划额外加密或国产 OAuth 转 API。

## Routes 返回错误

所有本机 Routes 只绑定 loopback，并要求本地 bearer：

| 现象 | 常见原因 | 处理 |
|---|---|---|
| `401 invalid_api_key` | 客户端没有发送或发送了错误的本地 token | 从该 Route 的配置读取脱敏后的本地凭据，重新配置客户端 |
| `404` / `surface_mismatch` | 请求路径和 Route 的 downstream surface 不匹配 | Responses、Messages、Chat Completions 使用各自的 Route surface |
| `400 listed_models_reject` 或 `model_unavailable` | 模型不在当前默认池可服务名单中 | 使用 `GET /v1/models` 查看当前 Route 的模型列表 |
| `429 bridge_overloaded` | 本机并发门限已满 | 等待正在运行的请求完成，或停止重复客户端 |
| `503 pool_exhausted` | 默认池当前没有可服务该请求的成员 | 检查 Routes 里已接入登录是否可用；有 `Retry-After` 时等待后再试 |
| `502 upstream_error` | 上游登录、网络或协议失败 | 看日志中的 `request_id`、`profile_id` 和脱敏 `upstream_detail` |
| `504 upstream_timeout` | 上游在超时窗口内未返回 | 检查上游 URL、网络和服务状态 |

完整 endpoint、请求头和响应形状见 [local-route-api.md](../reference/local-route-api.md)。

## 查看日志

CLI/GUI 共用 `{data_dir}/logs/agenthub.YYYY-MM-DD.log`，默认保留 14 天。路径可通过：

```text
cargo run -p agenthub-cli -- config path
```

CLI 临时提高级别：

```text
cargo run -p agenthub-cli -- -v doctor
```

日志中用 `request_id`、`profile_id`、`op`、`code` 检索。文件日志不记录请求正文、响应正文、prompt、工具参数或完整密钥。日志规范见 [logging.md](../reference/logging.md)。

## 测试失败

先用失败文件缩小范围：

```text
pnpm test -- --run path/to/failing.test.ts
cargo test -p agenthub-core --locked failing_test_name
```

若页面测试依赖真实 Tauri 或网络，通常是测试边界错误：改用固定 mock backend 和脱敏 fixture。若 `pnpm build` 报生产模块图错误，移除生产代码对 `src/dev`/`src/test` 的导入，而不是修改 guard。

