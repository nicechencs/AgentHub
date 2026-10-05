---
title: Core 与 Runtime
type: architecture
status: current
owner: maintainers
audience: core, Tauri, CLI, and runtime contributors
source-of-truth: crates/agenthub-core, src-tauri, and current adapter control/bridge code
updated: 2026-10-05
---

# Core 与 Runtime

本页说明 `agenthub-core` 怎样被桌面和命令行复用、主要写路径怎么走，以及本机转发跑在哪个进程里。

## Core 组合

桌面壳和 CLI 组合同一个 `agenthub-core`。core 不依赖 Tauri，所以同一条规则能被两端复用并单测。启动组合在 `crates/agenthub-core/src/startup.rs`，由 `AgentHub::open` 调用。

```text
入口壳（Tauri command / CLI）
  → core service
  → domain / adapter / platform
  → repository + 文件 + 进程 + HTTP
```

| 区域 | 负责 |
| --- | --- |
| `services` | 对外 use case：读写组合、锁、备份、current 指针一致性 |
| `domain/protocol_graph` | Agent 能力与路线的规划矩阵，只算不写 |
| `adapters` / `integrations` | 每个 Agent 的检测、配置、账号、运行和流解析 |
| `platform` | Agent 目录与 `AgentKey`、配置、安装与生命周期、技能、用量等平台能力 |
| `storage` | SQLite 事务、迁移和 repository |
| `bridge` | 本机转发：loopback 监听、准入、流和协议转换 |
| `adapter_control` | 本机转发写入的串行门（`AdapterSagaCoordinator`） |
| `runtime` | Node/npm 等共享运行时的检测、引导和缓存 |

「current 指针」指每个 Agent 当前生效的登录或供应商。它只由 `ConnectionService` 写入：同一事务里写 `accounts` / `providers` 的旧 `is_current` 字段和 `agent_active_bindings` 表。Account / Provider service 先写本机配置，成功后再调 `ConnectionService`，不各自尽力写 current。

## 典型写路径

### 连接绑定

「绑定」指把一份登录接到某个 Agent。

```text
ticket.plan(source, target)
  → AdapterRouteService::plan()：路线矩阵 + 能力 + 私有写入门
  → 只读预览（route / maturity / changes / canApply）

ticket.bind(source, target)
  → 重新 plan，不可写则拒绝
  → native_endpoint / config_sync：TicketBindService + adapter 写配置
  → local_bridge：桌面 host 的 saga
  → ConnectionService 记录生效连接
```

`plan` 只读，`bind` / `unbind` 是唯一写入口。`AdapterRouteService::plan()` 是路线和 `canApply` 的唯一决策者，详见 [Adapter 路线内核](adapter-route-kernel.md)。

`local_bridge` 的 bind 不由 `TicketBindService` 启动监听，而由桌面 host 负责：`src-tauri/src/adapter_control_host.rs` 接收请求，`src-tauri/src/adapter_bridge_controller.rs` 启动监听，再调 core 的 `adapter_bridge_service`（含 `persist_saga.rs`）写配置；失败时逆序恢复。

### Agent 安装与运行

- 安装：runtime service 先检测 Node/npm 等前置环境，再由 lifecycle / adapter 执行白名单命令并刷新检测结果。
- Chat 旧发送方式：`ChatService` 组合 `RunService`、adapter 的运行参数、`StreamingProcessRunner` 和对应流解析器。
- Chat 持续对话：`services/chat_runtime` 为每个会话保持一个 Agent 进程（如 Codex app-server），命令串行执行，事件先写 SQLite 再给页面读快照。哪些 Agent 走哪种方式见 [Chat 与 Agent](../concepts/chat-and-agents.md)。

阻塞进程都在 Tauri command 的异步边界之外运行。

## 本机转发的进程边界

```text
Tauri AppState
  ├─ RouteRuntimeManager（运行实现的唯一进程级持有者）
  │    ├─ BridgeRuntimeHost（当前固定默认）
  │    └─ Go 隔离运行监督器（开发/探针，不是 Product 后端）
  ├─ DesktopAdapterControl + AdapterSagaCoordinator
  └─ AgentHub（core services）
       └─ 127.0.0.1 监听 + 协议转换
```

当前监听跑在 Tauri 进程内，只听 loopback。`RouteRuntimeManager` 统一持有 Rust 监听和现有 Go 隔离运行监督器，并收口状态观察、退出影响与关闭；它还没有可持久的运行实现选择，产品默认仍固定为 Rust。授权池、本机令牌和 Codex / Grok 共用 Responses 入口等行为见 [Connections、Routes 与绑定](../concepts/connections-and-routing.md#登录列表与-routes)。`native_endpoint` / `config_sync` 不依赖本机转发，也不会自动入池。

把监听拆到独立 sidecar 进程（`agenthub-adapterd`）仍是[提案](../proposals/adapter-sidecar.md)，不是当前部署。开发态已有隔离监督器：core 可从保存结果生成 loopback 路由及官方 Anthropic API Key 的 Messages 路由配置，经 stdin 启动非默认端口上的 Go 进程，并展示状态、排空退出和有限恢复。另一条独立 probe 只在 scratch Claude 目录用合成登录运行 core bind/unbind 与持久化失败补偿，覆盖真实文件写入，不覆盖 Tauri 监督器、Go 进程、桌面端到端或真实上游；两者都不替代上述进程内监听。

## Agent 目录与能力

平台 registry 逐步以 `AgentKey`（小写 kebab-case 字符串）为主；旧 `AgentId` / `AgentAdapter` 保留兼容。

能力等级 `CapabilityLevel` 有四档：`Full`、`Partial`、`Planned`、`Unsupported`。它是调用门禁，不是商品白名单：`Planned` / `Unsupported` 必须返回明确的不支持错误，不能伪装成可用。

## 相关页面

- [架构总览](overview.md)
- [Adapter 路线内核](adapter-route-kernel.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [Chat 与 Agent](../concepts/chat-and-agents.md)
- [Sidecar 提案](../proposals/adapter-sidecar.md)
- [本机同口授权池（归档）](../archive/unified-loopback-pool.md)
