---
title: 官方扩展与 Go 路由切片深度评估
type: proposal
status: proposed
owner: maintainers
updated: 2026-10-03
---

# 官方扩展与 Go 路由切片深度评估

本页是 2026-10-03 对[模块化 §8](modularity.md#8-功能模块与官方扩展)、[路由官方扩展](adapter-sidecar.md)和 [A 阶段纸面契约](route-extension-phase-a-contract.md) 的深度评估与 go / no-go。不是现行产品契约，也不授权默认网关切换。

对照树：`HEAD` 含隔离 Go Messages 切片 `go/agenthub-adapterd` 与 `scripts/route-runtime-probe/messages-isolated.sh`。调查基线 tag `baseline/routes-before-extension-20261003`（`7c2b6fe2`）不得移动。

## 1. 两类对象

| 对象 | 用户看到的位置 | 现在怎样 |
|---|---|---|
| 厂商插件 | `/plugins` 页面 | 各 Agent 自己的 plugin / extension 包。Claude / Grok 列表和启停已落地；更新、Codex / Pi 写入仍是提案。见[插件管理](plugin-management.md)。 |
| 官方扩展 | 随应用交付、固定注册 | 扩展 AgentHub 自身。第一个候选是路由，标识 `agenthub.routes`。不是插件商店，不加载第三方 ABI。 |

二者不是同一套接口。厂商插件不接到官方扩展机制上；官方扩展也不改各家 `/plugins` 的归属。

## 2. 当前基线与第一候选

本机转发仍是 Tauri 进程内 `local_bridge`。路由页面仍编进应用。

路由是第一个官方扩展候选。除路由外，登录与连接、会话 / 历史 / 用量、技能 / MCP、各 Agent 对接、数据迁移与更新均为 **design-only**，本评估不授权实现。

树中已落地的只是隔离 scratch 切片，不是 live sidecar：

- 控制：`Handshake`、`Status`、`AcquireOrRenewOwner`（acquire / renew）
- 探测专用 `ActivateProbeListen`（不是产品 `CommitDesired` / `PrepareDesired` / `BootstrapDesired`）
- 合成 Key 的 `POST /v1/messages` JSON / SSE
- 脚本：`scripts/route-runtime-probe/messages-isolated.sh`
- 默认网关仍是进程内 Rust 转发

## 3. 开源对照

对照的是「进程外数据面 + 宿主拥有配置」和「固定注册 vs 商店插件」，不是要把这些项目搬进 AgentHub。

| 项目 | 链接 | 可借鉴 | 不照搬 |
|---|---|---|---|
| HashiCorp go-plugin | https://github.com/hashicorp/go-plugin | 独立进程、握手、协议版本、插件崩溃不拖垮宿主；明确拒绝进程内共享库 ABI（Vault 等安全场景） | 通用插件 SDK、任意第三方二进制发现、跨语言 gRPC 商店 |
| Kubernetes CSI sidecar | https://kubernetes-csi.github.io/docs/ 与 [sidecar 容器](https://kubernetes-csi.github.io/docs/sidecar-containers.html) | 核心不内嵌驱动；节点侧用 Unix socket 注册；sidecar 只做辅助、不拥有集群真相 | 多 sidecar 组合、Kubernetes API 监视、外置驱动商店 |
| VS Code Extension Host / Agent Host | https://code.visualstudio.com/api/advanced-topics/extension-host 、[Agent Host](https://code.visualstudio.com/blogs/2026/08/26/agent-host-architecture) | UI 与扩展分进程；工作台拥有窗口与生命周期；扩展经 RPC，不能直接改核心 | 第三方扩展市场、每个窗口一套 host、把官方路由做成可下载扩展 |
| Envoy sidecar 模式 | https://www.envoyproxy.io/docs/envoy/latest/intro/arch_overview/intro/intro | 数据面独立进程，控制面下发配置；故障隔离 | 网格、公网网关、把 AgentHub 拆成微服务 |

结论与现有提案一致：官方路由扩展应随包固定注册、进程外转发、core 仍拥有登录和写入；不要动态 ABI、不要第二套领域库、不要插件商店。

## 4. 未完成的官方扩展工作

下列项仍未做，且本评估 **no-go** 实现。

- 通用官方扩展平台、动态插件 ABI、插件商店 / SDK、事件总线、第二套领域库
- 登录 / 连接、会话、技能 / MCP、各 Agent 对接、迁移 的独立进程或可停用插件化
- 把 `/plugins` 厂商包接到 `agenthub.routes`

可继续的只有提案与文档对齐（已在本分支做）。没有可执行的官方扩展功能文件。

## 5. 未完成的 Go 路由工作

已落地隔离切片见第 2 节。尚未授权、因此 **no-go** 的：

- 整份 A 仍未通过。仍缺：按旧实现复现并分类的故障运行日志、`revision` 存储及数据库迁移、A 交付后的独立审查、Windows 本地 IPC 只读实验
- 隔离 probe 尚缺：三协议、进程 / 端口 / IPC 故障、撤销、日志脱敏、打包黑盒；现有 `messages-isolated.sh` 只覆盖进程与合成 Key Messages
- B 只读原型（固定注册接到产品、权限与实例身份、默认网关旁的 IPC）
- C/D：真实配置写入、测试 Agent 写入整合、产品 `CommitDesired` / `PrepareDesired` / `BootstrapDesired`
- E/F：打包签名升级、完整后台、live/default 网关切换
- Responses、Chat Completions、官方登录刷新、写 `~/.agenthub`

隔离切片目录内没有未完成的 TODO；没有本评估批准的新 Go 功能文件。

## 6. go / no-go

**NO-GO（改造）**：不进入 B–F，不切默认网关，不实现产品 `CommitDesired` / `PrepareDesired` / `BootstrapDesired`，不写真实应用目录，不做插件商店或动态 ABI。OBJECTIVE 要求「方案没有问题才改造」；A 阶段门槛仍缺运行日志、库表迁移、独立审查和 Windows IPC，改造会越权。

**GO（本轮）**：只完成评估、剩余清单、过时表述归档或改写、文档检查，以及把已有文档提交推送。不新增测试文件，不跑 Rust 测试。

独立审查（A 退出项）仍缺：本页是方案评估，不是 A 阶段独立审查闭环。

## 相关页面

- [模块化与边界收紧 §8](modularity.md#8-功能模块与官方扩展)
- [路由官方扩展与独立运行](adapter-sidecar.md)
- [A 阶段纸面契约](route-extension-phase-a-contract.md)
- [插件管理](plugin-management.md)
- [产品边界](../decisions/product-boundaries.md)
