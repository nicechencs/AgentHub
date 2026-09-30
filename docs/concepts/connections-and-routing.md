---
title: Connections、Routes 与绑定
type: explanation
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: Ticket/Connection services, adapter planner contracts, and product boundary decisions
updated: 2026-09-30
---

# Connections、Routes 与绑定

## 一句话

AgentHub 保存的是一份“登录”：一把 API Key 或一次官方订阅登录。用户把它接到某个编程工具时，系统在“直接改配置 / 写进对方认的登录 / 本机转发”三条路线里选一条。路线属于“这份登录 → 目标 Agent”这条边，不属于登录本身。

产品边界（谁能分享、什么不做）的原文在 [产品边界](../decisions/product-boundaries.md)；本页解释规划、写入和两个页面的分工。

## 术语

| 用户看到 | 内部术语 | 含义 |
| --- | --- | --- |
| 登录 / Connection | Ticket（过渡期聚合 accounts + providers） | 用户可选的一份真实授权 |
| 编程工具 | Agent | Claude、Codex、Grok、Pi、Kiro 等目标客户端 |
| 对方认的登录或配置位置 | Slot / writer | 目标 Agent 可写的本机位置 |
| 这份登录接到这个工具 | Edge / Binding | 一个来源到一个目标的使用关系 |
| 切换 | 领域路线 `native` | 登录本来就属于这个 Agent，只切换当前使用 |
| 直接改配置 | `native_endpoint`（领域路线 `reshape`） | 目标不需要常驻转发 |
| 写进对方认的登录 | `config_sync`（领域路线 `reshape`） | 目标自己负责后续使用和续期 |
| 本机转发 | `local_bridge`（领域路线 `bridge`） | 目标连本机端口，AgentHub 持有上游登录 |

界面只说“登录”“Connections”“Routes”；`Ticket`、`Binding`、`Wallet` 只在实现和设计文档里用。完整对照见 [术语表](../reference/terminology.md)。

## 规划器

```text
plan(source, target)
  → 来源能对上游说什么
  → 目标接受什么接口 / 登录位置
  → 有没有稳定的转换器和写入实现
  → route + maturity + changes + canApply + reason
```

`support`（稳定/实验/不支持）、`maturity`（stable/experimental/preview/none）和 `canApply` 不能混用：`canApply=true` 只表示今天有写入实现、且来源登录信息可解析，不代表产品价值判断。具体哪条边开着，见 [Route 兼容性](../reference/route-compatibility.md)。

优先级固定：

1. 这份登录本来就属于目标 Agent：切换。
2. 目标认这套订阅登录：写进目标的登录位置，不转发。
3. Key 已符合目标接口：只改配置。
4. 以上都不通但有受测转换：本机转发。
5. 没有写入实现、协议边或允许的登录契约：明确不可行并说明缺什么。

## 唯一写入口

```text
bind(source, target)   → 创建/更新目标 Agent 的 active binding
unbind(binding)        → 停止转发（若有）、恢复上一份本机配置、保留登录
```

`bind` 会重新规划，`canApply=false` 时 fail closed。直接改配置和写登录位置由 Ticket/Adapter apply 与 Account/Provider/Connection service 协调；本机转发的启动、目标配置写入、运行状态和回滚由桌面端 saga（带回滚的多步写入）协调。页面不能绕过 `bind` 直接“应用一份自动生成的配置”。前端入口是 `src/lib/api/tickets` 的 `plan` / `bind` / `unbind`。

每个 Agent 同时只有一条正在用的连接；一份登录可以接到多个 Agent，不会因此复制成多份。WorkBuddy / ZCode 是追加式：切换只写对应的模型或供应商行，其他条目留在对方列表里。`ConnectionService` 维护“当前使用”的一致性；旧的 `accounts.is_current` 和 `providers.is_current` 只是过渡镜像。

## 登录列表与 Routes

**添加与导入**

- Connections 和 Routes 都能添加官方登录 / API Key，入口是「添加登录」菜单。
- 设置 → 偏好里「自动导入本机登录」默认开启，此时菜单不显示「导入本机登录」；关掉后菜单为「导入本机登录 / 官方登录 / 添加 API Key」。
- 官方登录与 API Key 分行保存。WorkBuddy 自定义模型和 ZCode 供应商按条拆成多份登录，桌面套餐登录不导入。

**两个页面各管各的**

- 连接页添加的登录归连接页；连接池页（Routes）添加的登录只给连接池用（`home=route_pool`），可以不出现在连接页。
- 两边回收站分开：连接页删除进登录回收站，连接池移出进连接池回收站，恢复只回原来那一页。
- 连接页的登录进连接池走连接池页的「从连接同步」：所有 API Key 都可加入（含 WorkBuddy / ZCode 上配置的）；Claude / Codex / Grok 官方登录按已登记的接法；国产官方登录不进候选；已在池里的跳过。连接页没有「分享至连接池」入口。
- 同步进池不复制登录；从池里移除不删连接页的登录。在连接池里编辑这份官方登录并保存时，先复制成连接池自己的一份（连接页那份还在），再问要不要把模型写回连接页。
- 接到某个工具从 Dashboard「连接/切换」。

**本机转发怎么跑**

- Routes 管理本机转发：固定本机入口、入口 Key、默认池成员、模型名单、启停、自动恢复、失败详情和解绑。
- 接到本机转发后，目标客户端只认一个本机端口和一把入口 Key。每个目标 Agent / 接口一个默认池；往池里增删登录不改客户端配置。
- `/v1/messages` 默认连接池目前只接 Claude；不会把现有 Claude 池改成多 Agent Messages。
- Codex 与 Grok 共用 `/v1/responses`，具体格式跟路由一起保存，由入口 Key 选中，不看请求正文猜。写进 Codex / Grok 的具体配置键见 [本机 Routes API](../reference/local-route-api.md#route-surface-和上游协议)。
- 调度在本机网关里：先解析模型和协议，再在合格成员里按默认 `priority_failover` 选；可改为 `round_robin`，只在同类合格成员间轮询。已知剩余额度只用来打破平局，不覆盖粘性。未声明等价关系时，不会把请求发到另一家供应商。
- 矩阵里的 `multi_account=false` 不会关掉已入索引的池内多成员。
- 官方直连（`native_endpoint` / `config_sync`）不自动入池；Routes 对还能改成本机转发的直连提供「交给本机网关」。
- 只监听本机，不做公网、多人共享或转售；上游登录信息留在 AgentHub。

## 相关页面

- [Accounts 与 Authorization Pool](accounts-and-authorization.md)
- [Adapters 与本机 Bridge](adapters-and-bridges.md)
- [产品边界](../decisions/product-boundaries.md)
- [本机 Routes API](../reference/local-route-api.md)
- [本机同口授权池（归档）](../archive/unified-loopback-pool.md)
