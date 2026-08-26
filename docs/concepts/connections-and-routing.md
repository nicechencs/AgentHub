---
title: Connections、Routes 与绑定
type: explanation
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: Ticket/Connection services, adapter planner contracts, and product boundary decisions
updated: 2026-08-26
---

# Connections、Routes 与绑定

## 一句话

AgentHub 保存的是一份登录：API Key 或一次订阅授权。用户把它接到某个编程工具时，系统在“直接改配置 / 写进对方认的登录 / 本机转发”中选择一条路线。路线属于“这份登录 → 目标 Agent”这条边，不属于登录本身。

## 术语

| 用户看到 | 领域/实现术语 | 含义 |
| --- | --- | --- |
| 登录 / Connection | Ticket（过渡期聚合 accounts + providers） | 用户可选择的一份真实授权 |
| 编程工具 | Agent | Claude、Codex、Grok、Pi 等目标客户端 |
| 对方认的登录或配置位置 | Slot / writer | 目标 Agent 可写的 native 位置 |
| 这份登录接到这个工具的做法 | Edge / Binding | 一个 source 到 target 的使用关系 |
| 直接改配置 | `native_endpoint` / `config_sync`，领域常归 `reshape` | 目标无需常驻 bridge |
| 写进对方认的登录 | `config_sync` / native account switch | 目标自己负责后续使用/刷新 |
| 本机转发 | `local_bridge` | 目标连 loopback，Hub 持有上游授权 |

界面说“登录”“Connections”“Routes”；`Ticket`、`Binding`、`Wallet` 只在实现和设计文档中使用。自动生成的 Provider/profile 不是新的登录。

## 规划器

具体的 source/target 兼容性和边成熟度以 [Route compatibility reference](../reference/route-compatibility.md) 为准；本页只定义规划、绑定和产品语义。

```text
plan(source, target)
  → source 能对上游说什么
  → target 接受什么 wire / auth slot
  → 是否存在稳定转换器与 writer
  → route + maturity + changes + canApply + reason
```

领域路线只有 `native`、`reshape`、`bridge` 和不可行；wire 上的 `native_endpoint`、`config_sync`、`local_bridge` 是实现/展示名。`support`（稳定/实验/不支持）、`maturity`（stable/experimental/preview/none）和 `canApply` 不应混为一谈：`canApply=true` 只代表今天存在写实现且 source secret 可解析，不代表产品价值判断。

优先级固定为：

1. 这份登录本来就是目标 Agent：切换到它。
2. 目标认这套订阅登录：写进目标的登录槽，不转发。
3. Key 已经符合目标接口：只改配置。
4. 前三者不通但有受测转换：本机转发。
5. 没有 writer、协议边或允许的登录契约：明确不可行并说明缺口。

## 唯一写入口

```text
bind(source, target)   → 创建/更新目标 Agent 的 active binding
unbind(binding)        → 停桥（若有）、恢复上一份 live、保留登录
```

`bind` 必须重新规划并在 `canApply=false` 时 fail closed。`native`/`reshape` 由 Ticket/Adapter apply 与 Account/Provider/Connection service 协调；`bridge` 的启动、目标配置写入、运行状态和回滚由桌面 host saga 协调。页面不得绕过 `bind` 直接调用“apply 一份自动生成配置”。

每个 Agent 同时只有一条 active binding；一份登录可以绑定多个目标 Agent，不会因绑定而复制成多份登录。`ConnectionService` 维护 current/binding 一致性，旧 `accounts.is_current` 和 `providers.is_current` 只是过渡镜像。

## 登录列表与 Routes

- Connections 列出真实 accounts/providers；列表不包含 bridge 生成的 local Provider。
- 行入口使用“分享 / 路由”，规划器按目的过滤可行目标；不可行项显示原因而不是隐藏。
- Routes 只管理 `local_bridge` runtime：loopback 地址、端口、启停、自动恢复、失败详情和解绑。
- 生成的 local token 只给目标客户端使用，上游 credential 留在 Hub/host；不监听公网，不做多人共享或转售。

## 相关页面

- [Accounts and authorization](accounts-and-authorization.md)
- [Adapters and bridges](adapters-and-bridges.md)
- [Product boundaries](../decisions/product-boundaries.md)
- [Local route API](../reference/local-route-api.md)
- [本机同口授权池（提案）](../proposals/unified-loopback-pool.md)
