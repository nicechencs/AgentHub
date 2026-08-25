---
title: Core 与 Runtime
type: architecture
status: current
owner: maintainers
audience: core, Tauri, CLI, and runtime contributors
source-of-truth: crates/agenthub-core, src-tauri, and current adapter control/bridge code
updated: 2026-08-25
---

# Core 与 Runtime

## Core 组合

GUI 和 CLI 都组合同一个 `agenthub-core`。core 不依赖 Tauri；它拥有路径解析、SQLite、脱敏、进程、协议和领域服务，使同一条规则能被桌面和命令行复用并单测。

```text
入口壳（Tauri command / CLI）
  → core service
  → domain / adapter / platform port
  → repository + filesystem + process + HTTP
```

主要边界：

| 区域 | 责任 |
| --- | --- |
| `services` | 组合读写、锁、备份、current/binding 一致性和对外 use case |
| `domain/protocol_graph` | Agent 能力与协议路线的规划矩阵，不执行写入 |
| `adapters` / `integrations` | Agent 特有的检测、配置、账号、运行和流解析 |
| `platform` | Agent catalog、AgentKey、configuration、lifecycle、skills、usage 等可复用平台能力 |
| `storage` | SQLite 事务、迁移和 repository |
| `bridge` | loopback host、admission、stream 和协议转换 |
| `runtime` | Node/npm 等共享运行时的检测、引导和缓存 |

`ConnectionService` 协调 active binding 与旧 `is_current` 镜像；Account/Provider service 不能各自 best-effort 写 current。生产写入口走 Ticket/Connection 组合，避免页面或 adapter 各自拼接副作用。

## 典型写路径

### 连接绑定

```text
ticket.plan(source, target)
  → protocol graph + capability + private write gate
  → safe preview（route / maturity / changes / canApply）

ticket.bind(source, target)
  → re-plan and reject if not writable
  → native/config_sync: service + adapter write
  → local_bridge: desktop host saga
  → ConnectionService records active binding
```

`plan` 只读，`bind`/`unbind` 是产品写入口。`AdapterRouteService::plan()` 是路线、gate 和 `canApply` 的唯一决策者；browser mock 只解释它对冻结入参的 golden 投影。桥接 bind 不由 core 的普通 `TicketBindService` 偷开 listener，而由桌面 host 的 saga 负责启动、写目标配置和失败逆序恢复。详见 [Adapter 路线内核](adapter-route-kernel.md)。

### Agent 安装与运行

Agent 安装先由 runtime service 检测 Node/npm 等共享前置环境，再由 lifecycle/adapter 执行白名单命令并刷新 detect。Chat 运行由 `ChatService` 组合 `RunService`、adapter run spec、`StreamingProcessRunner` 和对应 stream parser；阻塞进程从 Tauri command 的异步边界隔离出去。

## local_bridge：当前态与方向

当前态：

```text
Tauri AppState
  ├─ BridgeRuntimeHost
  ├─ DesktopAdapterControl / saga coordinator
  └─ core services
       └─ 127.0.0.1 listener + protocol conversion
```

当前 listener 是 Tauri 进程内 `local_bridge`，只听 loopback；生成的本地 bearer 是运行时材料，不是用户登录，也不进入 Connections 登录列表。`native_endpoint`/`config_sync` 不依赖 bridge。

方向态（提案，不是当前部署）：

```text
Tauri/CLI control client
  → local IPC
  → agenthub-adapterd（每 canonical data dir 单实例）
  → AdapterRuntimeApplication + BridgeRuntimeHost
```

sidecar 只承接 `local_bridge` runtime 和它的 lifecycle；Account、Provider、Connection/ActiveBinding 仍由既有 core owner 管理，sidecar 不直接拥有表或 live 配置。IPC、schema lease、升级和恢复完成前，不能把 sidecar 文档当作已实施事实。

## Agent catalog 与能力

平台 registry 逐步以稳定 `AgentKey`（小写 kebab-case 字符串）为主路径；旧 `AgentId`/`AgentAdapter` façade 保留用于兼容。能力等级 `Full`、`Partial`、`Planned`、`Unsupported` 是调用门禁，不是商品白名单：Planned/Unsupported 必须返回 typed unsupported，不能伪装成可调用功能。

## 相关页面

- [Architecture overview](overview.md)
- [Adapter 路线内核](adapter-route-kernel.md)
- [Adapters and bridges](../concepts/adapters-and-bridges.md)
- [Chat and agents](../concepts/chat-and-agents.md)
- [Sidecar proposal](../proposals/adapter-sidecar.md)
- [Legacy document index](../archive/legacy-document-index.md)
