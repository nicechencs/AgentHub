---
title: 日志参考
description: CLI、GUI、core 和本机 Routes 共用的日志文件、级别、字段与脱敏规则。
type: reference
audience: user-and-contributor
status: current
updated: 2026-08-29
---

# 日志参考

CLI 和 GUI 都调用 `agenthub-core::logging` 初始化同一套 tracing。生产排障以 Rust 文件日志为准；前端 `src/lib/logger.ts` 只在开发环境写 WebView console，生产静默。

## 文件和保留

- 目录：`{data_dir}/logs/`。
- 文件：`agenthub.YYYY-MM-DD.log`，按本地日期轮转。
- 默认保留 14 天；`log_retention_days` 可设为 `1..=365`，进程启动时清理。
- 文本日志无 ANSI，包含 level、target、字段和消息。

## 级别

| 级别 | 语义 |
|---|---|
| `error` | 操作失败、写入失败、OAuth 完成失败、协议译码失败、补偿失败 |
| `warn` | 本机认证失败、上游 4xx/5xx/超时、流式中断、能力降级、锁等待、协议拒绝 |
| `info` | 启动、设置变更、登录/切换成功、Route listener 里程碑、请求成功结束 |
| `debug` | 请求开始、锁/路径/转换步骤、health 成功；CLI `-v` 会启用 |
| `trace` | 极细内部事件；不记录正文或秘密 |

优先级：CLI `-v` 至少提升到 debug，其次是 SQLite settings 的 `log_level`，最后是默认 `info`。`config set log_level` 和 `log_retention_days` 在下次启动生效；`RUST_LOG`、`AGENTHUB_LOG` 和 `agenthub.toml` 日志覆盖未实现。

## 目标和字段

核心目标常量包括：`core.boot`、`core.storage`、`core.lock`、`core.provider`、`core.account`、`core.backup`、`core.install`、`core.detect`、`core.skill`、`core.chat`、`core.project`、`core.run`、`core.capability`、`core.settings`、`core.usage`、`core.oauth`、`core.adapter`、`cli`、`gui`。

本机 Routes 使用 `core.adapter` 和 `core.adapter.protocol`。部分 helper 通过 `module=` 字段记录逻辑模块，直接 tracing 调用通过 target 记录；排障时两者都搜。

推荐检索字段：

| 字段 | 含义 |
|---|---|
| `module` | 逻辑模块名 |
| `code` | 稳定错误码 |
| `op` | 短操作名 |
| `agent` | Agent id（若适用） |
| `path` | 切换写入的本机配置路径 |
| `last4` | 钥匙末四位（`**xxxx`），从不是明文 |
| `profile_id` | Route profile |
| `request_id` | 单次本机请求关联 id |
| `route` | `config_sync`、`native_endpoint` 或 `local_bridge` |
| `protocol` / `stream` / `model` | 上游协议、流式标志、改写后的模型 |
| `status` / `elapsed_ms` | HTTP 状态、耗时 |
| `upstream_detail` | 脱敏后的上游短原因，最多 512 字 |

登录相关操作（对照 GUI 排障）：

| `module` | `op` | 何时出现 |
|---|---|---|
| `gui` | `recognize` | 智能识别粘贴成功 |
| `gui` | `use_official` | 勾选或取消「使用官方服务」 |
| `gui` | `list_remote` | 拉取远程模型列表 |
| `gui` | `switch` / `switch_fail` | 连接页或连接流程切换登录 |
| `gui` | `bind` / `bind_fail` | 连接页跨 Agent 接入，或连接流程确认应用 |
| `gui` | `delete_connection` / `delete_connection_fail` | 连接页删除进回收站 |
| `gui` | `route_create` / `route_import` / `route_edit`（及对应 `_fail`） | 路由页新建 / 导入 / 编辑 |
| `gui` | `bridge_start` / `bridge_stop` / `bridge_remove` / `bridge_enroll`（及对应 `_fail`） | 路由页启动 / 停止 / 移除 / 纳入默认池 |
| `gui` | `switch_write` | 路由页「写入登录」成功；带 `last4` |
| `core.provider` | `recycle` | 登录被送进回收站 |
| `core.provider` | `switch_write` | 切换真正写了本机配置路径；带 `agent` 与 `last4`（`**xxxx`），从不写完整钥匙 |
| `core.provider` | `switch` | 切换结束；失败时带 `code=provider.switch.rollback` |
| `core.adapter` | `bind` / `unbind` | Ticket 绑定 / 解绑结束；成功带 `route` / `profile_id`，失败带 `code` |
| `core.adapter` | `apply_bridge` / `start` / `stop` | 本机转发应用 / 启动 / 停止里程碑；带 `profile_id` |
| `core.chat` | `send` | 对话一轮结束；Agent 失败时记 `send failed`，不记 `send ok` |
| `core.install` | `install_agent` | 安装结束；只打开官网时带 `code=setup_guide`，不是安装失败 |

前端 `logger.ts` 只打开发控制台。要进当天 `.log` 文件，GUI 事件必须走桌面后端（例如 `log_gui_event`）。`log_gui_event` 可选字段：`agent`、`last4`、`profile_id`、`route`、`code`；从不写明文钥匙。

## 必须脱敏

日志不记录请求/响应正文、prompt、工具参数、完整 API key、OAuth token、cookie 或 bearer。错误消息写入前经过 `redact_text`；JSON DTO 使用 `redact_json`。上游 HTML/raw body 不写日志，只记录状态、body 长度和 content type 等诊断字段。

本地 `401 invalid_api_key` 表示客户端到 Route 的 token 错误；上游认证耗尽后才记录上游 auth error。客户端看到的上游响应保持短且通用，详细原因只留在本地日志。

## 排障例子

```text
agenthub --verbose doctor
agenthub config get log_level
agenthub config path
```

先用 `request_id` 找到本机请求，再用 `profile_id` 和 `op` 区分本机认证、surface/协议转换、上游 HTTP 和流式阶段。日志参考不替代 [troubleshooting.md](../guides/troubleshooting.md)。

