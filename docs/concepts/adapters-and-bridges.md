---
title: Adapters 与本机 Bridge
type: explanation
status: current
owner: maintainers
audience: core, Tauri, and route/runtime contributors
source-of-truth: AgentAdapter, adapter planner/apply ports, bridge host code, and the sidecar proposal
updated: 2026-08-25
---

# Adapters 与本机 Bridge

## Adapter 解决什么问题

Adapter 把 Agent 特有的路径、配置、账号、运行命令和流输出差异贡献给平台能力。它不是一个让每个页面直接写文件的万能服务。平台服务拥有锁、事务、备份、日志、能力门禁和进度；Adapter 只处理具体 Agent 的差异。

前端 adapter port 的核心读写面是：

```text
analyze(source, target)
plan(source, target)
listProfiles(filter)
apply(source, target)
remove(profile)
startBridge(profile) / stopBridge(profile) / getBridgeStatus(profile)
```

返回的 analysis/plan 不含 secret。`actions` 只表达“将配置到哪里”或引用哪份 Connection；任何 secret action 都是引用而不是序列化明文。

## 路线与状态

路线的厂商、协议和凭据兼容性以 [Route compatibility reference](../reference/route-compatibility.md) 为准；本页只解释 Adapter 与 Bridge 的职责，不复制兼容矩阵。

| wire/实现路线 | 用途 | 是否常驻进程 |
| --- | --- | --- |
| `native_endpoint` | source API 已能说 target 端点，只改地址/模型 | 否 |
| `config_sync` | 写入 target 认的配置或 OAuth 槽 | 否；由目标自己使用/刷新 |
| `local_bridge` | source 与 target 协议不相同，但存在受测转换 | 是；仅 loopback |
| `unsupported` | 没有 writer、转换器或允许的认证契约 | 否 |

`support`、`maturity` 和 `canApply` 分别表达矩阵信心、边的成熟度和今天能否写。预览可存在但不能因此偷偷执行写入。这些字段由 `AdapterRouteService::plan()` 决定；browser mock 只查 golden，未命中 fail-closed。见 [Adapter 路线内核](../architecture/adapter-route-kernel.md)。产品三路说明见 [connections-and-routing](connections-and-routing.md)。

## Profile 与 generated Provider

Adapter profile 是一条“source connection → target Agent”的受管投影，保存 source/target、route、mode、rule、状态、端口和 autoStart 等元数据；它不保存 credential。bridge 可能生成一个 Provider 作为 target 的配置投影，但这个 Provider 只引用真实 Connection secret：

- 不进入 Connections 登录列表；
- 不可作为下一次 `bind` 的 source；
- 解绑时由 binding/host saga 清理或恢复；
- 不代表用户新增了一份 API Key 或 OAuth 登录。

## 当前 local_bridge

当前实现是 Tauri 进程内 host：`AppState` 持有 `BridgeRuntimeHost`、bridge controller 和控制协调器，core 的 bridge 服务负责 admission、listener、transport、stream 与协议转换。运行面只绑定 `127.0.0.1`/`localhost`/`::1`，目标客户端使用本地 bearer；上游 secret 不写进目标配置。

本机入口包括 Messages、Responses、Chat Completions；同协议直接转发，或转成上游协议。完整 endpoint 与转换表在 [本机路由 API](../reference/local-route-api.md)，本文不重复维护厂商端点清单。

## sidecar 是方向，不是现状

未来可以把 `local_bridge` runtime 移到用户级 `agenthub-adapterd`：Tauri/CLI control client 通过本地 IPC 请求它，sidecar 成为 listener、drain、恢复和 bridge mutation 的单一 owner。该方向不改变领域边界：Account、Provider、Connection、ActiveBinding 仍由 core service 管理，sidecar 不直接写数据库表或 live 配置。

在 IPC handshake、schema lease、single-instance、升级/恢复和 host unavailable 语义落地前，不得宣称已存在 sidecar，也不能让 GUI 和 sidecar 同时拥有 bridge saga。

## 相关页面

- [Connections and routing](connections-and-routing.md)
- [Adapter 路线内核](../architecture/adapter-route-kernel.md)
- [Core and runtime](../architecture/core-runtime.md)
- [Frontend and backend boundary](../architecture/frontend-backend.md)
- [Sidecar proposal](../proposals/adapter-sidecar.md)
