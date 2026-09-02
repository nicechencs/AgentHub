---
title: 产品边界与术语决策
type: decision
status: current
owner: maintainers
audience: product, design, frontend, and core contributors
source-of-truth: root AGENTS.md, current planner contracts, and connection/account services
updated: 2026-08-31
---

# 产品边界与术语决策

## 用户表面

- 页面说“登录”“Connections”“Routes”，不说“票”“钱包”“Binding”。
- `Ticket`、`TicketBinding`、`Wallet`、`Binding` 是实现/领域术语，不能外溢到普通 UI 文案。
- Connections 管通用登录，以及从 Connections 创建或导入的登录。行入口是「分享至连接池」；接到某个工具从 Dashboard「连接/切换」。**API Key 都可以分享**（含 WorkBuddy / ZCode 等上配置的）；**国产官方登录不能分享**。
- Routes 可以直接新增和管理“仅用于本机路由”的官方登录 / API Key。这类 login 使用 `home=route_pool`，可不出现在 Connections；其新增、编辑、删除生命周期由 Routes 管理。
- 从 Connections 选入连接池的登录，登录本身仍由 Connections 管理；Routes 只管理它在本机路由中的成员关系和运行配置。连接池页也可以「从连接同步」（所有 API Key 都可同步；国产官方登录不进入候选）。
- 本机路由仍是登录的一种使用方式；从连接池移除成员不等于删除 Connections 管理的登录。连接页与连接池各有独立回收站；删除和恢复只回到原来那一页。
- Routes 管理本机转发 runtime：入口、本机令牌、池成员、启停、自动恢复、失败详情和解绑。

## 三种接法

对一份 source 登录和一个目标 Agent，planner 按以下顺序给出路线：

1. 目标本来就是这份登录所属的 Agent：切换。
2. 目标认同一套订阅登录：写进目标认的登录槽。
3. API Key 已符合目标 wire：直接改配置。
4. 以上都不通但有受测协议转换：仅在本机启动 loopback bridge。
5. 否则展示不可行原因；不把目标入口静默藏掉，也不假装“订阅一律不做”。

`plan()` 是唯一的规划出口；`canApply` 只表示当前有写实现和可解析的 source secret。产品支持、实验成熟度和当前可写性必须分别展示。真正的写入走 `bind()`，解除走 `unbind()`。

## 安全与所有权边界

- local bridge 只监听当前用户本机 loopback；不提供公网入口、多人共享或转售。
- 目标工具得到本地 bearer/引用，上游 credential 留在 AgentHub/bridge host。
- bridge 生成的 Provider、profile 或 local config 是私有投影，不进入登录列表，不作为下一次 bind 的 source。
- Routes 直接新增的“仅用于本机路由”官方登录 / API Key 仍是真实登录来源，但使用 `home=route_pool`，可不进入 Connections；它们不是自动生成的本机配置，且由 Routes 管理其生命周期。
- Account、Provider、Connection 和 ActiveBinding 由 core service 负责；前端、generated Provider、未来 sidecar 都不能各自维护第二套 current 真相。

## 明确不做

### 凭据落盘加密

当前决策是无必要、项目范围外，沿用既有存储方案。不要据此创建 keyring、AES、主密码、密文迁移或“先加密再重构”的实施任务。只有用户明确推翻这项决策并重新授权，才可重新评估。

### API Key 可分享；国产官方登录不可分享

所有 API Key 都可以分享至连接池，也可以在看板接到其他工具。不按登录所属 Agent 挡掉：WorkBuddy / ZCode / Pi 等上配置的 Key 与 Claude / Codex / Kimi 上的 Key 同一条规则。原因：API Key 本身可拷贝、可在各工具里填写，AgentHub 不应比手动复制更严。

国产官方登录（Kimi CLI 会员 OAuth，以及 GLM / DeepSeek / 通义 / 豆包等）不可分享至连接池，也不可接到其他工具。不为这些官方登录开 adapter 边；也不把官方登录伪装成 API Key、`native_endpoint` 或任何 OAuth→API 转换。国产路由只认产品已登记的官方 API Key。此项是产品关闭边，不是后续 roadmap。

Claude / Codex / Grok 的官方登录是否可接到其他工具，仍由已登记的 planner 边决定，不因本条而关闭或新开。

入池不复制登录；从池中移除不等于删除登录。实现不得用「这份登录挂在哪个 Agent」代替「这是 API Key 还是国产官方登录」。

### 其他产品边界

不把兼容服务默认常驻；不让自动生成配置反向成为连接；不把路由做成公网网关；不为了文档整洁引入微服务、DDD、CQRS、动态插件 ABI 或第二套领域数据库。

## 未来方向如何标记

`agenthub-adapterd` sidecar 仅是 `local_bridge` 的进程边界提案。当前 listener、saga 和退出 drain 仍在 Tauri `AppState` 进程内；sidecar 未迁移前，任何“IPC 已可用”“sidecar 负责写表”的表述都不准确。见 [Adapters and bridges](../concepts/adapters-and-bridges.md)。

本机同口授权池是当前默认能力：每个目标 Agent/surface 一个默认池，客户端只认固定 loopback 口和本机令牌；池内按模型与健康选成员。每个 Agent 同时仍只有一条 active binding；官方 `native_endpoint` / `config_sync` 不自动入池。混合供应商复合路由仍默认关闭，未声明等价关系时不得跨供应商转发。设计记录见 [本机同口授权池（归档）](../archive/unified-loopback-pool.md)。

分支与发布红线不在本文重复：日常开发只在 `dev`，发布流程和 `release` 约束以根 [AGENTS.md](../../AGENTS.md) 为准。

## 相关页面

- [Decision index](README.md)
- [Connections and routing](../concepts/connections-and-routing.md)
- [Accounts and authorization](../concepts/accounts-and-authorization.md)
- [Adapters and bridges](../concepts/adapters-and-bridges.md)
- [Architecture overview](../architecture/overview.md)
- [本机同口授权池（归档）](../archive/unified-loopback-pool.md)
