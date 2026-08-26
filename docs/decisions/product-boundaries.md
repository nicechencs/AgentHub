---
title: 产品边界与术语决策
type: decision
status: current
owner: maintainers
audience: product, design, frontend, and core contributors
source-of-truth: root AGENTS.md, current planner contracts, and connection/account services
updated: 2026-08-26
---

# 产品边界与术语决策

## 用户表面

- 页面说“登录”“Connections”“Routes”，不说“票”“钱包”“Binding”。
- `Ticket`、`TicketBinding`、`Wallet`、`Binding` 是实现/领域术语，不能外溢到普通 UI 文案。
- 连接列表展示真实登录；本机路由是登录的一种使用方式，不是另一份登录。
- Routes 管理本机路由 runtime；Connections 管理登录与“分享/路由”入口。

## 三种接法

对一份 source 登录和一个目标 Agent，planner 按以下顺序给出路线：

1. 目标本来就是这份登录所属的 Agent：切换。
2. 目标认同一套订阅登录：写进目标认的登录槽。
3. API Key 已符合目标 wire：直接改配置。
4. 以上都不通但有受测协议转换：仅在本机启动 loopback bridge。
5. 否则展示不可行原因；不把入口隐藏，也不假装“订阅一律不做”。

`plan()` 是唯一的规划出口；`canApply` 只表示当前有写实现和可解析的 source secret。产品支持、实验成熟度和当前可写性必须分别展示。真正的写入走 `bind()`，解除走 `unbind()`。

## 安全与所有权边界

- local bridge 只监听当前用户本机 loopback；不提供公网入口、多人共享或转售。
- 目标工具得到本地 bearer/引用，上游 credential 留在 AgentHub/bridge host。
- bridge 生成的 Provider、profile 或 local config 是私有投影，不进入登录列表，不作为下一次 bind 的 source。
- Account、Provider、Connection 和 ActiveBinding 由 core service 负责；前端、generated Provider、未来 sidecar 都不能各自维护第二套 current 真相。

## 明确不做

### 凭据落盘加密

当前决策是无必要、项目范围外，沿用既有存储方案。不要据此创建 keyring、AES、主密码、密文迁移或“先加密再重构”的实施任务。只有用户明确推翻这项决策并重新授权，才可重新评估。

### 国产 OAuth 与转 API

不为 Kimi CLI 会员 OAuth、GLM/DeepSeek/通义/豆包等国产 OAuth 开 adapter 边；也不把 OAuth 伪装成 API Key、`native_endpoint` 或任何 OAuth→API 转换。国产路由只认产品已登记的官方 API Key。此项是产品关闭边，不是后续 roadmap。

### 其他产品边界

不把兼容服务默认常驻；不让自动生成配置反向成为连接；不把路由做成公网网关；不为了文档整洁引入微服务、DDD、CQRS、动态插件 ABI 或第二套领域数据库。

## 未来方向如何标记

`agenthub-adapterd` sidecar 仅是 `local_bridge` 的进程边界提案。当前 listener、saga 和退出 drain 仍在 Tauri `AppState` 进程内；sidecar 未迁移前，任何“IPC 已可用”“sidecar 负责写表”的表述都不准确。见 [Adapters and bridges](../concepts/adapters-and-bridges.md)。

本机同口授权池（一份 Hub 令牌、跨产品成员、按模型选号）仅是调度提案。当前本机令牌仍对应单条边，每个 Agent 同时只有一条 active binding；未实施前不得写成已同口调度。见 [本机同口授权池](../proposals/unified-loopback-pool.md)。

分支与发布红线不在本文重复：日常开发只在 `dev`，发布流程和 `release` 约束以根 [AGENTS.md](../../AGENTS.md) 为准。

## 相关页面

- [Decision index](README.md)
- [Connections and routing](../concepts/connections-and-routing.md)
- [Accounts and authorization](../concepts/accounts-and-authorization.md)
- [Adapters and bridges](../concepts/adapters-and-bridges.md)
- [Architecture overview](../architecture/overview.md)
- [本机同口授权池（提案）](../proposals/unified-loopback-pool.md)
