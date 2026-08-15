# 产品决策（订阅本机路由）

> 状态：**2026-08-15 纠正**。本文是「订阅 / 账号 → 本机协议转换 → 其他 Agent 使用」的产品真源。  
> 实现状态仍以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 和 [provider-api-oauth-adaptation.md §4](provider-api-oauth-adaptation.md#4-当前实现矩阵) 为准。  
> 领域对象见 [connection-binding-model.md](connection-binding-model.md)。厂商边与工程门禁见 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)。

## 0. 一句话

AgentHub 要做成和 **cc-switch / CLIProxyAPI / Management Center** 同一类能力：用户把自己的**订阅或账号**放进钱包，本机做**协议转换与路由**，让 **Claude Code、Codex、Pi 等其他 Agent** 当标准客户端来用。

这是核心产品，不是「受限实验、默认不做」。  
**未实现**只表示工程还没接到 `bind`，不表示产品否决。

## 1. 用户任务（产品）

用户只关心这一条链路：

```text
登录 / 导入订阅或 Key
  → 本机认出票面（产品 + 凭据类 + 上游协议）
  → 需要时做协议转换（Messages ↔ Responses ↔ Chat Completions …）
  → 把目标 Agent 指到本机 loopback 或上游兼容入口
  → 其他 Agent 用起来
```

例子（与参考产品对齐，不是宣传矩阵）：

| 来源票 | 目标 Agent | 用户感知 |
|---|---|---|
| ChatGPT / Codex 订阅 | Claude Code | Claude 走本机桥，额度来自 ChatGPT 订阅 |
| Claude 订阅 | Codex / 其他兼容客户端 | 本机暴露 Anthropic 或 OpenAI 兼容面 |
| Gemini / Antigravity 订阅 | Claude Code / Codex | 本机转换后写入目标配置 |
| Kimi / GLM / DeepSeek 等已有兼容入口 | Claude / Codex / Pi | 能直连就直连，不能就桥 |

「官方登录 / API Key / 桥 / 投影 Provider」都是实现。用户不该先判断协议再决定去哪个页。

## 2. 先前决策错在哪

仓库里多处把**参考产品的核心能力**写成了非目标，又把**未实现**写成了**产品关闭**。这与「对齐 cc-switch / CLIProxyAPI / Management Center」冲突。

| 旧表述 | 问题 | 纠正后 |
|---|---|---|
| 参考项目「只借鉴方法，不借鉴产品」 | 用户要的就是对方的产品能力：订阅本机路由 | **借鉴产品能力**；不借鉴运营形态（公网、拼车、转售） |
| 「不是把订阅变成通用 API」 | 参考产品正是：订阅 → 本机兼容 API → 其他客户端 | **本机**兼容面是目标；不是公网通用网关 |
| 「消费订阅一律不做 / 仅受限实验」 | 把合规提醒升级成了产品否决 | 订阅路由是要做的；风险写进预览与 dogfood，不挡产品方向 |
| 「Grok / 订阅仍关」写进规则结论 | 这是实现矩阵状态，被读成「永远不做」 | 实现未开 ≠ 产品不做。打开边是后续工程，不是再讨论「能不能做」 |
| Codex 订阅 → Claude 必须等「官方契约 / 条款门禁完整通过」才允许当产品 | 参考产品已经用非官方 Codex 通道做这件事；把官方背书当准入会永久卡住 | **产品要做这条边**。工程上仍要协议 fixtures、refresh、不泄露 secret；条款风险对用户可见，不作为「产品不做」的依据 |
| Usage「零侵入，不做本地代理」 | 和 Adapter 本机桥打架 | Usage 继续只读日志；**订阅路由的本机桥是另一条产品线**，必须做 |
| Management Center 被降成「只抄 UI 交互」 | 它是 CLIProxyAPI 的管理面：OAuth、凭据、配额、探测 | 管理面能力要对齐；视觉体系和多栏工作台不必抄 |

## 3. 三个参考项目分别对齐什么

三个项目不是同一个东西，不要混成「只抄其中一个的 UI」或「只抄协议方法」。

```text
cc-switch          = 桌面端：多 Agent 配置切换 + 本机代理 + 订阅反代
CLIProxyAPI        = 本机代理本体：订阅 OAuth → 多协议兼容 HTTP
Management Center  = CLIProxyAPI 的管理 UI：登录、凭据、配额、日志、探测
```

AgentHub 的组合是：**cc-switch 那种桌面钱包 / 切换** + **CLIProxyAPI 那种本机协议路由** + **Management Center 那种管理动作**（用 AgentHub 自己的页面，不另起一套视觉系统）。

### 3.1 要做成一样的（产品能力）

| 能力 | cc-switch | CLIProxyAPI | Management Center | AgentHub 落法 |
|---|---|---|---|---|
| 订阅 / CLI OAuth 登录入库 | 有（含 Codex Device Code） | 有（`--codex-login` 等） | 发起并轮询 OAuth | Connections 钱包；登录仍走现有 PKCE / device code |
| 把订阅当上游，给其他 Agent 用 | Codex OAuth 反代进 Claude | 暴露 OpenAI / Claude / Gemini / Codex 兼容口 | 不管转发 | `bind(票, Agent)`：reshape 或 `local_bridge` |
| 协议转换 | Responses ↔ Messages / Chat | 多协议 translator | 无 | 已有 IR / Bridge；订阅边接同一内核 |
| 写目标 Agent 配置 | 有，可接管 Claude / Codex | 用户自己改 `ANTHROPIC_BASE_URL` 等 | 无 | 现有 Adapter writer；目标只持 loopback bearer |
| 本机 loopback，不导出上游 token | 有 | 默认本机端口 | 无 | 保持：上游 secret 留在 Hub / sidecar |
| 账号、配额、探测、启停 | 托盘 / 健康 | 服务端 | 管理面主职 | Dashboard / Connections / 桥与适配；探测可对齐「测试上游 / 测试目标」 |
| 模型别名 / 角色映射 | 有 | 有 | 可配 | 需要时做；不挡第一条订阅边 |

### 3.2 明确不做成一样的（运营形态，不是能力）

这些是参考项目里**可以关掉或我们本来就不要**的部分，不能再写成「所以订阅路由也不做」。

| 不抄 | 原因 |
|---|---|
| 公网入口、远程管理默认开、团队共享端点 | 个人本机工具；默认只听 loopback |
| 多号轮询 / fill-first / 权重 / 冷却池 / 拼车拆票 | 不做号池转售或多人共用一张订阅 |
| 把生成的 localhost Provider 再当新票去 bind | 投影是绑定的 runtime，禁止二次投影 |
| 抄参考项目源码进仓库 | 只对齐行为；许可仍要单独审；优先重写 |
| Management Center 的多栏工作台、SCSS token、完整日志控制台 | 管理动作要对齐，页面留在 AgentHub 现有 IA |
| 把 Usage 改成靠代理截流计费 | Usage 继续解析本地日志；桥只服务路由 |

凭据落盘加密仍为项目范围外，不列入本决策的待办或风险。

## 4. 产品开，实现可以关

两套话不要混：

| 层 | 说什么 | 不说什么 |
|---|---|---|
| 产品 | 订阅本机路由是要做的；缺边就补转换器 + 写配置 | 「订阅默认不做」「不是产品」 |
| 实现 | 这条边现在 `canApply=false` / 还没接 transport | 「用户不准问起」「入口藏掉」 |
| 安全 | 本机、当前用户、token 不进目标 Agent、不进日志 | 「未获官方书面批准就不能做产品」 |

因此：

- `plan()` 对未完成的订阅边应给出**可预览的路线**（bridge + 原因 + 缺什么），而不是用产品否决句结束。
- 打开 `canApply` 的条件是工程就绪：分类、secret 解析、refresh single-flight、协议 fixtures、loopback bearer、写入与回滚。**不是**再开一次「要不要做订阅路由」的产品讨论。
- 用户可见风险（非官方通道、账号可能被上游限制）写在预览里，作为 opt-in，不作为永久 `unsupported`。

## 5. 产品优先级（工程顺序，不再讨论方向）

与参考产品对齐时，先做「订阅当上游」，再做管理面增强。

1. **ChatGPT / Codex 订阅 → Claude Code**（cc-switch 已有的旗舰边；CLIProxyAPI 的同类能力）
2. **Claude 订阅 → Codex / 其他已登记 writer**（CLIProxyAPI 方向）
3. **Gemini / Antigravity 等 CLI 订阅 → 已登记 Agent**
4. 已有 API Key / 官方兼容入口的边继续按协议图补齐（Kimi / GLM / DeepSeek / OpenAI / xAI）
5. 管理面：OAuth 状态、配额、最小探测、桥启停（对齐 Management Center 的职责，不抄页面）

能直连（`native` / `reshape`）就不强行起桥。桥是协议不同时的默认手段，不是「能不用就永远不用」的禁区。

## 6. 其他文档怎么读

| 文档 | 纠正后怎么读 |
|---|---|
| 本文 | 订阅本机路由的**产品**真源 |
| [connection-binding-model.md](connection-binding-model.md) | 票 / 绑定 / 协议图仍是领域模型；「不借鉴产品」已废止 |
| [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) | 厂商边与**当前能否 bind**；§1.1 不再把订阅写成非目标 |
| [adapter-design.md](adapter-design.md) | 页面与桥 runtime；「不是通用网关」仅指不做公网/多租户 |
| [agenthub-plan.md](agenthub-plan.md) | 总方案；§8 是实现清单，不是产品否决清单 |
| [adapter-sidecar-design.md](adapter-sidecar-design.md) | 桥进程归属；不改变「订阅路由要做」 |

旧句「只借鉴方法、不借鉴产品」「消费订阅不是产品」视为历史误写，以本文为准。
