---
title: Adapters 与本机 Bridge
type: explanation
status: current
owner: maintainers
audience: core, Tauri, and route/runtime contributors
source-of-truth: AgentAdapter, adapter planner/apply ports, bridge host code, and the sidecar proposal
updated: 2026-10-04
---

# Adapters 与本机 Bridge

本页解释两个内部角色：Adapter（每个 Agent 的对接代码）和 Bridge（界面上的“本机转发”）。路线怎么选见 [Connections、Routes 与绑定](connections-and-routing.md)；哪条来源能接到哪个目标见 [Route 兼容性](../reference/route-compatibility.md)；本机转发的接口见 [本机 Routes API](../reference/local-route-api.md)。

## Adapter 管什么

Adapter 只处理某个 Agent 特有的路径、配置、账号、启动命令和输出格式差异。锁、事务、备份、日志、能力门禁和进度由平台服务负责；页面不能绕过平台服务直接让 Adapter 写文件。

前端 adapter port（`src/lib/backend/contracts/adapter.ts`）的核心读写面：

```text
analyze(request) / plan(request)          预览，不写入
listProfiles(filter)                      已有的接法
apply(request) / remove(profileId)        写入与移除
startBridge / stopBridge / getBridgeStatus(profileId)
```

port 另有连接池、入口 Key 和路由记录等方法，以源码为准。analyze/plan 的返回不含密钥；`actions` 只写“要配置到哪里”和引用哪份登录，不序列化明文。

## 四种路线

| 路线 | 用途 | 是否常驻进程 |
| --- | --- | --- |
| `native_endpoint` | 来源 API 已能对上目标端点，只改地址/模型 | 否 |
| `config_sync` | 写进目标认的配置或 OAuth 位置 | 否；目标自己使用和续期 |
| `local_bridge` | 协议不同但有受测转换 | 是；只监听本机 |
| `unsupported` | 没有写入实现、转换器或允许的登录契约 | 否 |

`support`、`maturity` 和 `canApply` 分别表示矩阵信心、这条边的成熟度、今天能不能写。三者由 `AdapterRouteService::plan()` 算出；浏览器 mock 只查 golden 用例，未命中就 fail closed。细节见 [Adapter 路线内核](../architecture/adapter-route-kernel.md)。

## Profile 与自动生成的 Provider

Adapter profile 是一条“来源登录 → 目标 Agent”的受管记录，保存来源、目标、路线、模式、规则、状态、端口和 autoStart 等元数据，不保存登录信息。本机转发可能为目标生成一个 Provider 配置，它只引用真实登录：

- 不进入 Connections 登录列表；
- 不能当下一次 `bind` 的来源；
- 解绑时由 binding / 桌面端 saga（带回滚的多步写入）清理或恢复；
- 不代表用户新增了一份 API Key 或官方登录。

## 当前本机转发

- 运行在 Tauri 桌面进程内：`AppState` 持有 `BridgeRuntimeHost`、bridge controller 和控制协调器；core 的 bridge 服务负责准入、监听、传输、流和协议转换。
- 只绑定 `127.0.0.1` / `localhost` / `::1`。目标客户端拿到的是本机入口 Key，上游登录信息不写进目标配置。
- 每个目标 Agent / 接口一个默认连接池，共用一个本机端口；`GET /models` 与实际请求共用同一个模型解析器。
- 混合供应商复合路由和 Codex↔Grok 双向 Responses 转换是实验开关，默认关闭。

## Go 路由程序的当前边界

树中已有隔离目录下的 `agenthub-adapterd` Messages 切片（含连接池调度、Responses 与 Chat Completions 同协议转发，以及 Responses 到 OpenAI 兼容 Chat 的转换），以及开发态看板「Go 路由」条的启动/停止。应用可以在同一进程和端口上原子替换完整运行配置；桌面监督器只在 Go 确认配置哈希后更新崩溃恢复快照。它仍使用临时目录和非默认端口，还不是默认本机路由；当前生产监听仍在 Tauri 桌面进程内。

后续替换只把监听、协议转换和池内调度移到 Go。Account、Provider、Connection、ActiveBinding、数据库和 Agent 配置仍由 core service 管理；Go 不维护第二套产品状态，也不直接写数据库或本机配置。最小接入和分步切换见 [Go 路由替换方案](../proposals/adapter-sidecar.md)。

## 相关页面

- [Connections、Routes 与绑定](connections-and-routing.md)
- [Adapter 路线内核](../architecture/adapter-route-kernel.md)
- [Core and runtime](../architecture/core-runtime.md)
- [Frontend and backend boundary](../architecture/frontend-backend.md)
- [本机同口授权池（归档）](../archive/unified-loopback-pool.md)
