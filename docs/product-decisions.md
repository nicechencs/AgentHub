# 产品决策（跨 Agent 复用三路）

> 状态：**2026-08-15**。本文是跨 Agent 复用的**产品**真源。  
> 领域对象与 `plan()` 仍以 [connection-binding-model.md](connection-binding-model.md) 为准。  
> 厂商边与**当前能否 bind** 以 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) 为准。  
> 实现清单以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 为准。

## 0. 一句话

一张票接到另一个 Agent，只可能是下面三路之一。分类挂在 **(票, 目标 Agent)** 这条边上，不挂在票上。

```text
① API 端点直连     上游同一把 Key 已提供目标听的协议（常见：双协议 Key）
② 原生订阅复用     目标 Agent 自己就认这套 OAuth / 订阅登录
③ 本机协议桥       协议对不上，才起 loopback 做转换
```

能走 ① 或 ② 就不起桥。③ 是兜底，不是默认。  
**未实现**只表示这条边还没接到 `bind`，不表示产品不做。

## 1. 三路

### 1.1 API 端点直连

上游同一把 API Key 已经提供目标 Agent 听的协议。典型是**双协议 Key**：同一产品同时给 Anthropic Messages 和 OpenAI Chat Completions。一把 Key 可以接到两种以上 Agent，只改 Base URL / 槽位，**不起桥**。

| 例子 | 目标 | 用户感知 |
|---|---|---|
| Kimi Code 会员 Key | Claude Code | 写 Anthropic 兼容入口 |
| 同一把 Key | Pi | 写入 Pi 的对应槽 |
| GLM Coding Plan / DeepSeek API | Claude Code | 写官方 Anthropic 兼容入口 |

同一机制也覆盖**单协议 Key**：Anthropic Key → Pi、OpenAI Key → Pi。它们不是「双协议」，但同样是直连、不起桥。

硬约束：

- `OpenAI 兼容`必须写全名。Chat Completions **不是** Codex 要的 Responses。
- 所以同一把双协议 Key → Codex 常常掉进 **③**，不是「双协议 = 万能」。

### 1.2 原生订阅复用

目标 Agent **公开支持**同一套订阅 / OAuth 契约（同一登录方式、同一刷新语义、有对应 provider 槽）。Hub 只把授权写进目标自己的槽，**不起桥**，也不把 OAuth token 翻译成另一家 Key。

| 例子 | 目标 | 用户感知 |
|---|---|---|
| Claude 订阅 | Pi 的 Anthropic 槽 | 和在 Pi 里登录 Claude 同一回事 |
| Codex / ChatGPT 订阅 | Pi 的 `openai-codex` 槽 | 写 Pi 的 Codex 登录槽 |
| Grok / xAI 订阅 | Pi 的 xAI 槽 | 写 Pi 的 xAI 登录槽 |
| 任一订阅 | 签发它的那个 Agent | 普通切换（`native`） |

Pi 是当前已登记的跨 Agent 第 2 路落点。别的 Agent 必须逐个验证契约，不能因为「都是 OAuth」就推导。

不是第 2 路（常见误判）：

| 组合 | 实际 | 原因 |
|---|---|---|
| Claude 订阅 → Codex | ③ 或暂不可行 | Codex 不吃 Anthropic PKCE |
| Grok 订阅 → Claude | ③ 或暂不可行 | Claude 不吃 xAI OAuth，也没有 Messages 兼容面 |
| Codex 订阅 → Claude | **③** | Claude 只听 Messages；这是本机桥，不是写 Claude 官方登录 |

第 2 路有工程门禁，不是改判成 ③ 的理由：若 refresh token 单次轮换，原 Agent 与目标各自刷新会互相打翻。逐边选「目标自己再登录」或「Hub 统一刷新 + 目标只持引用」。

### 1.3 本机协议桥

票能说的和 Agent 听的对不上，图上又有已测转换边。这时才起本机 loopback：目标只持本地 bearer，上游 secret 留在 Hub / sidecar。

| 例子 | 用户感知 |
|---|---|
| Codex 订阅 → Claude Code | Claude 指到本机桥，额度来自 ChatGPT 订阅 |
| Kimi / Anthropic API Key → Codex | Codex 听 Responses，上游是 Chat 或 Messages，要转换 |
| Claude 订阅 → Codex（若 transport 成立） | Codex 听 Responses，上游是 Anthropic OAuth |

③ 对齐 cc-switch 的 Codex OAuth 反代、CLIProxyAPI 的协议 translator。  
**不**学 CLIProxyAPI「永远先起一个兼容 HTTP」。

## 2. 和领域 route 的映射

领域模型不新增枚举。`plan()` 仍只返回 `native | reshape | bridge | 不可行`。三路是用户说明，由 `plan()` 派生，前端不得自己猜。

| 用户三路 | 判定 | 领域 `route` | 实现名 | 起桥 |
|---|---|---|---|---|
| ① API 端点直连 | API Key，且 `speaks ∩ accepts ≠ ∅` | 通常 `reshape`；发给本 Agent 时 `native` | `native_endpoint` / `config_sync` | 否 |
| ② 原生订阅复用 | OAuth，且目标有**同一授权契约槽** | 跨 Agent 通常 `reshape`；本 Agent `native` | `config_sync` / 账号切换 | 否 |
| ③ 本机协议桥 | 无共同协议或契约槽，图上有已测边 | `bridge` | `local_bridge` | 是，仅 loopback |
| —— | 无 writer / 无边 / 登录态不能当 HTTP 上游 | 不可行 | `unsupported` | 否 |

`accepts[]` 要同时登记 **wire 协议** 和 **OAuth 契约槽**。OAuth 票的 `speaks` 除协议外要带契约身份（如 `openai-codex-pkce`），否则规划器判不出 ②。

## 3. 判定顺序

```text
plan(ticket, agent):
  if 无 writer:                              不可行（目标不可写）
  if 票本来就签给这个 Agent:                 native          # 切换；② 的本 Agent 情形
  if OAuth 且目标 accepts 含同一授权契约:    reshape         # ② 不起桥
  if API Key 且 speaks ∩ accepts 非空:       reshape         # ① 不起桥
  if 有可用上游 且 图上有 speaks→accepts 边:  bridge          # ③ 仅 loopback
  不可行（写明缺的是：契约槽 / 共同协议 / 转换边 / HTTP 上游）
```

顺序不可交换：`native` > `reshape`（①②）> `bridge`（③）> 不可行。

## 4. 同票不同路（防「双协议 = 万能」）

三路不是票的字段。钱包只标这张票 **对上游能说什么**；走哪一路只出现在 bind 预览里。

| 同一张票 | → Claude | → Pi | → Codex |
|---|---|---|---|
| Kimi / GLM / DeepSeek 双协议 Key | ① 直连 Messages 入口 | ① 写槽 | ③ Chat ≠ Responses，要桥 |
| Anthropic API Key | native / ① | ① 写 Anthropic 槽 | ③ Messages → Responses |
| Codex 订阅 | ③ 本机桥 | ② 写 `openai-codex` 槽 | native |
| Claude 订阅 | native | ② 写 Anthropic 槽 | ③ 或暂不可行 |
| Grok 订阅 | ③ 或暂不可行 | ② 写 xAI 槽 | ③ 或暂不可行 |

固定句式：

> 双协议指这把 Key 对**上游**能说 Messages 和 Chat Completions。能不能直连由**目标听什么**决定。Chat Completions 不是 Codex 的 Responses。

## 5. 参考项目怎么对

不要把三个项目都概括成「本机代理」。

| | cc-switch | CLIProxyAPI | Management Center | AgentHub |
|---|---|---|---|---|
| ① 双协议 / 官方兼容入口直连 | 有（preset 直连） | 也能配 Key，但默认仍走代理 | 不管转发 | **优先直连** |
| ② 订阅写进目标自己的槽 | 有（各 App 官方登录 / 切换） | 弱；倾向先登录再暴露 HTTP | 发起 OAuth、管凭据 | **Pi 三槽是范例**；能写槽就写槽 |
| ③ 协议转换 / 订阅反代 | Codex OAuth 反代进 Claude；Needs Local Routing | 核心：多协议 translator | 无 | 只在 ①② 都走不通时起桥 |
| 管理面 | 托盘 / 健康 | 服务端 | 主职：登录、配额、探测、日志 | 用现有页面做这些动作，不抄多栏工作台 |

明确不抄：公网入口、号池拼车、转售、把投影再当票、参考项目源码、CLIProxyAPI「永远起代理」。  
凭据落盘加密仍为项目范围外。

## 6. 产品开，实现可以关

| 层 | 说什么 | 不说什么 |
|---|---|---|
| 产品 | 三路都要做；先直连和原生订阅，对不上再桥 | 「订阅 = 本机路由」「订阅不是产品」 |
| 实现 | 这条边现在 `canApply=false` | 「用户不准问起」「入口藏掉」 |
| 安全 | 本机、当前用户、③ 的 token 不进目标 Agent | 「未获官方书面批准就不能做 ③」 |

打开 `canApply` 的条件是工程就绪，不是再讨论「要不要做」。③ 的非官方通道风险写在预览里 opt-in。

## 7. 工程顺序（不再讨论方向）

1. **① 补齐**：双协议 Key 接到更多已登记 Agent；GLM/DeepSeek → Pi 已可 experimental bind（自定义 provider 槽）；单协议 Key 的 reshape 继续按图补。
2. **② 先用已有槽**：Claude / Codex / Grok 订阅 → Pi（目标已声明契约）。**当前实现**：这三条边已可 experimental bind（写入 Pi `auth.json` 对应槽，Pi 拥有刷新）。再评估其他 Agent 有没有同类槽。
3. **③ 旗舰桥**：Codex 订阅 → Claude Code（cc-switch 已有）。再评估 Claude 订阅 → Codex、Grok 订阅 → Claude。
4. 管理面：OAuth 状态、配额、最小探测、桥启停（对齐 Management Center 的职责，不抄页面）。

## 8. 其他文档怎么读

| 文档 | 怎么读 |
|---|---|
| 本文 | 三路的**产品**真源 |
| [connection-binding-model.md](connection-binding-model.md) | 票 / 绑定 / `native·reshape·bridge`；三路是用户映射，不是第二套枚举 |
| [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) | 厂商边与当前能否 bind；订阅 ≠ 要起桥 |
| [adapter-design.md](adapter-design.md) | 页面与桥 runtime；桥只服务 ③ |
| [ui-design.md](ui-design.md) | ConnectFlow 预览标 ①②③；② 不显示本机服务 |
| [adding-an-agent.md](adding-an-agent.md) | 新 Agent 必须登记 wire 协议 **和** OAuth 契约槽 |
| [architecture.md](architecture.md) | 模块拆分；原则 12 按三路解释 `plan()` |
| [agenthub-plan.md](agenthub-plan.md) | 总方案；§8 是实现清单 |
| [adapter-sidecar-design.md](adapter-sidecar-design.md) | 只有 ③ 依赖 sidecar |
| [hub-redesign-plan.md](hub-redesign-plan.md) | Phase 1 **历史记录**；「不改 OAuth 门禁」不是产品否决 |
| [adapter-kimi-codex-dogfood.md](adapter-kimi-codex-dogfood.md) | ① 与 ③ 的真机清单；同票不同路 |
| [deepseek-harness-integration.md](deepseek-harness-integration.md) | DeepSeek API 属 ①；DSH 不是桥 |
| [account-authorization-pool.md](account-authorization-pool.md) | 票的去重；不决定走哪一路 |
| [capability-matrix.md](capability-matrix.md) | Agent **自己**能不能；不是复用三路 |
| [testing.md](testing.md) | 契约仍是 `route`/`canApply`；三路由 plan 派生 |
| [cli-and-config.md](cli-and-config.md) | CLI「代理模式」≠ ③ |
| [privacy.md](privacy.md) / [logging.md](logging.md) | 脱敏与截图；与三路正交 |
| [platform-capability-*.md](platform-capability-refactor.md) | 平台端口历史；不定义复用产品 |

旧句「订阅本机路由是唯一产品」「只借鉴方法不借鉴产品」「消费订阅不是产品」作废。

## 9. 评审纪要（Fable / GPT Sol）

2026-08-15 与 Fable、GPT Sol 对质后的共识，写入本文，不再另开讨论：

- 三路是产品一等语言，**不**进领域模型当第五个 `route`。
- 分类在边上，不在票上。同一把 Kimi Key → Claude 是 ①、→ Codex 是 ③，这是常态。
- 反对「全部走本机桥」：①② 本可零进程；强行桥增加 sidecar、语义损失和条款暴露。cc-switch 对双协议 preset 也是直连。
- 反对「订阅一律 ③」：Pi 三个 OAuth 槽已经是 ② 的反例。不能当 API Key ≠ 目标不能原生吃同一契约。
- 上一版把「订阅」几乎都写成 ③，风险是：先造桥、漏掉 Pi `config_sync`、UI 对原生订阅误显示「需要本机服务」。
