---
title: Connections、Routes 与绑定
type: explanation
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: Ticket/Connection services, adapter planner contracts, and product boundary decisions
updated: 2026-08-29
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

每个 Agent 同时只有一条正在用的连接；一份登录可以接到多个目标 Agent，不会因绑定而复制成多份登录。WorkBuddy / ZCode 是目录追加：切换只写入对应模型或供应商，其它条目仍留在对方的列表里。`ConnectionService` 维护 current/binding 一致性，旧 `accounts.is_current` 和 `providers.is_current` 只是过渡镜像。

## 登录列表与 Routes

- Connections 列出真实 accounts/providers；列表不包含 bridge 生成的 local Provider。登录仍由 Connections 管理。官方登录与 API Key 分行保存；添加入口是「导入授权 / 官方登录 / 添加 API Key」。
- WorkBuddy 自定义模型和 ZCode 供应商按目录拆成多条登录，桌面套餐登录不导入。
- 行入口使用“分享 / 路由”，规划器按目的过滤可行目标；不可行项显示原因而不是隐藏。
- Routes 管理本机转发 runtime：固定 loopback 入口、本机令牌、默认池成员、模型名单、启停、自动恢复、失败详情和解绑。
- 接到本机转发后，目标客户端只认一个 loopback 口和一把本机令牌。默认每个目标 Agent/surface 一个池；往池里增删合格登录不改客户端配置。Codex 与 Grok 共用 `/v1/responses`，具体格式跟路由一起保存，由本机令牌选中，不根据请求正文猜测。接到 Codex 时写入 Responses + 本机 API Key（进 `auth.json`）；接到 Grok 时写入 `api_backend = "responses"` 和本机令牌。这不是 Codex↔Grok 双向转换开关。
- 调度留在本机网关：先解析模型和协议，再从合格成员里按默认 `priority_failover` 选择；`GET /models` 与实际请求共用同一份 resolver。未声明等价关系时，不会把请求发到另一个供应商。
- 官方直连（`native_endpoint` / `config_sync`）不自动入池。Routes 对仍可改成本机转发的直连提供「交给本机网关」。
- 生成的本机令牌只给目标客户端使用，上游登录信息留在 Hub；不监听公网，不做多人共享或转售。

## 相关页面

- [Accounts and authorization](accounts-and-authorization.md)
- [Adapters and bridges](adapters-and-bridges.md)
- [Product boundaries](../decisions/product-boundaries.md)
- [Local route API](../reference/local-route-api.md)
- [本机同口授权池（归档）](../archive/unified-loopback-pool.md)
