# 账号池：身份 × 授权 与去重方案

> **现行状态（2026-08-23）**：OAuth 同一 Agent 同一身份只留一行并覆盖；跨 Agent 同身份各留一行；API Key 仍按指纹分行。本机路由导入同一 agent 只 upsert 一个槽；自动生成的配置不出现在登录列表。sidecar 未迁。
> 关联：[capability-matrix.md](capability-matrix.md)（Agent 原生能力）、[product-decisions.md](product-decisions.md)（跨 Agent 复用三路）、[provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)（协议图上的边）、[connection-binding-model.md](connection-binding-model.md)（票 / 绑定）、[architecture.md](architecture.md)、[cli-and-config.md](cli-and-config.md)（account 命令）。
> **已废止（仅同 Agent OAuth）**：2026-08-03「同人不同 token → 2 行」。API Key 与跨 Agent OAuth 仍分行。

## 1. 产品决策（硬约束）

### 1.1 两个概念

| 概念 | 含义 | 例子 |
|---|---|---|
| **身份（Identity）** | 谁 | email、user_id、principal_id |
| **授权（Authorization）** | 一次授权拿到的那组凭据（一份登录；实现里叫票） | 一组 refresh/access token，或一把 API Key |

### 1.2 核心规则

1. **OAuth：同一 Agent + 同一稳定身份 = 一行**。新 token / 再登录覆盖该行（credentials + `updated_at`），不另开一行。
2. **身份** = 凭据里的稳定标识集合：email 对 email，subject-like id（`user_id` / `sub` / `account_id` / `account_uuid` / `principal_id`）彼此求交。展示标签、token 预览不是身份。双方都没有这些字段则 fail-closed，不猜测合并；凭据完全相同仍可 upsert。
3. **跨 Agent 同身份保留各一行**（Claude OAuth 与 Grok OAuth 同一邮箱是两行）。合并只在该 `agent_id` 的池内进行。
4. **Pi**：仍按 live slot（`same_live_slot` / provider）分行。同一邮箱在 anthropic 槽与 xai 槽是两行。
5. **API Key：不同密钥 = 不同行**。`authorization_key` 默认仍是 token / key 指纹。本机路由槽 upsert 不变；loopback 不得吞掉官方 OAuth 行。
6. 每个 agent 的 **live 生效位仍只有一个**（`is_current` 至多一条）。

| 场景 | 池内条数 | 说明 |
|---|---|---|
| 同 live / 同 token 再 import 一次 | **1** | 同授权 → upsert |
| 同一 Agent，同一人，两次 OAuth，token 不同 | **1** | 覆盖原行 |
| 跨 Agent，同一人 OAuth | **各 1** | 不跨 `agent_id` 合并 |
| 同一人，API Key A 与 Key B | **2** | 不同密钥 |
| 不同人（同 Agent） | **2+** | 不同身份 |
| 身份不明的两份不同 OAuth token | **2** | 不猜测合并 |

**已废止（仅同 Agent OAuth）**：2026-08-03「同一人，两次 OAuth，token 不同 → 2+ 行」。

**例外（本机路由）**：`127.0.0.1` / `localhost` / `::1` 上的本机路由不是独立用户授权——bind 每次都会换端口和 bearer。同一 agent 只保留一个本机路由槽位，再 import 覆盖（新端口、新 token），不按 token 指纹另开一行。不得与远端 API Key / 官方 OAuth 合并，也不得删 adapter 自动生成的配置。自动生成的配置对用户**隐藏**（不出现在登录列表，不是第二份登录）。

一句话：

> **OAuth 按 Agent + 身份一行，更新覆盖；跨 Agent 同身份各留一行；Key 仍按指纹分行。  
> 本机 live 仍只有一个 current。**

跨 Agent 复用时，用户看到的是登录；这里的「授权」在实现里就是 [票（Ticket）](connection-binding-model.md)。`agent_id` / 从哪个 Agent 导入只表示出身，**不**决定能不能 `bind` 到别的 Agent，也不决定走直接改配置、写进对方认的登录还是本机转发。每个 Agent 的 active 绑定至多一条，对应本节的 live 生效位。见 [product-decisions.md](product-decisions.md)。

---

## 2. 与能力矩阵的边界

沿用 [capability-matrix.md §3](capability-matrix.md)：

| 问题 | 归谁 | 例子 |
|---|---|---|
| 能不能导入 / 切换账号？ | **能力矩阵** | `AccountSwitch`、`ApiKeyAccount` |
| 两份凭据是不是同一张「票」？ | **Adapter `authorization_key`** | token / api_key 指纹（Key 与精确 token） |
| 命中后 create 还是 update？ | **AccountService** | 授权指纹；OAuth 另加同 Agent 稳定身份 |
| 列表怎么按人分组展示？ | **UI + 可选 `identity_label`** | 行上展示；同 Agent OAuth 已按身份一行 |

**不进矩阵**：字段路径、token 指纹算法、是否按 subject 合并（合并策略是产品/service 规则，不是 Full/Unsupported）。

---

## 3. 三层分工

```
UI / CLI：import_live / add_api_key / oauth 完成
        │
        ▼
AccountService
  1. require(AccountSwitch | ApiKeyAccount)
  2. 得到 kind + credentials
  3. auth_key = adapter.authorization_key(kind, credentials)
     （None 时回退：完整 credentials JSON 相等）
  4. 同 agent + 同 kind 下查找：
       - auth_key / 完整凭据命中，或
       - OAuth 且稳定身份相同（Pi 还要同 live slot）
       命中 → update 该行；OAuth 同身份多余行删除
       未命中 → create 新行
  5. 自愈：同授权指纹冗余 + 同 Agent 同 OAuth 身份多余行
     禁止跨 agent_id 合并；身份不明不猜测删除
        │
        ▼
AgentAdapter
  authorization_key  → token / key 指纹（默认仍不用 email）
  identity_label     → hint=None 时为稳定身份；带 hint 仅展示
```

---

## 4. 契约设计

### 4.1 Adapter（目标 API）

```rust
// AgentAdapter — 目标形态（实现时落地）

/// 授权指纹：同一张「票」才相同。用于去重，不是「是不是同一个人」。
/// - Some(key)：同 agent 下 key 相等 ⇒ 同一授权 ⇒ upsert
/// - None：交给 service 用「完整 credentials 相等」回退
///
/// 约束：
/// 1. 必须能区分「同人两次不同授权」（不同 refresh/access ⇒ 不同 key）
/// 2. 同一次 live 反复 import ⇒ 相同 key
/// 3. key 可进日志：api_key / token 用 hash，勿明文
fn authorization_key(
    &self,
    kind: AccountKind,
    credentials: &serde_json::Value,
) -> Option<String>;

/// 身份标签。`label_hint = None` 时为稳定身份（email / user_id / sub），
/// 供同 Agent OAuth 覆盖；带 hint 的展示名不得当身份。
fn identity_label(
    &self,
    kind: AccountKind,
    credentials: &serde_json::Value,
    label_hint: Option<&str>,
) -> Option<String>;
```

默认实现（未 override 时）：

| kind | `authorization_key` |
|---|---|
| ApiKey | `apikey:sha256:{api_key}` |
| OAuth | `oauth:refresh_sha:{hash}` → 否则 `oauth:access_sha:{hash}` → 否则 `oauth:cred_sha:{hash(整包)}` |

**禁止**默认用 email / user_id 作为 `authorization_key`。

`identity_label` 默认：扫 email / user_id / sub；再才用 `label_hint`。同 Agent OAuth 去重只走 `identity_label(..., None)`（不用展示名）。

### 4.2 键格式建议

```text
{kind}:{strategy}:{value}
```

示例：

- `apikey:sha256:a1b2…`
- `oauth:refresh_sha:c3d4…`
- `oauth:cred_sha:e5f6…`（无法抽出 refresh/access 时）

### 4.3 Service 匹配优先级

同 `agent_id` + 同 `kind`（Pi 还要同 live slot）：

1. **`authorization_key` 双方都有且相等** → 同一授权  
2. **完整 `credentials` JSON 相等** → 同一授权（re-import 兜底）  
3. **OAuth 且双方稳定身份相同** → 同一登录，覆盖  
4. 否则 → **新授权**，`create`（身份不明不猜测合并）

### 4.4 merge 行为

| 入口 | 命中同授权或同 OAuth 身份后 | 其它行 |
|---|---|---|
| `import_live` | update credentials/label/extra；`is_current=true` | 同 Agent 同 OAuth 身份多余行删除；其它身份不动 |
| `add_api_key` | update；不强制 current | 不动（Key 仍按指纹） |
| OAuth 完成入库 | 同 import：同身份覆盖 | 同身份多余行删除 |

**自愈范围：**

- 删除：同 agent + 同 kind + **同 authorization_key** 的多余行  
- 删除：同 agent + **同 OAuth 稳定身份**（及 Pi 同 slot）的多余行  
- **禁止**：跨 `agent_id` 合并；按展示名 / token 预览猜测删除 

### 4.5 current 与 live

- 每 agent 至多一条 `is_current=1`（现有 repo 不变量）  
- switch：把选中授权写入 live，并设为 current；**池内其它授权行完整保留**  
- 再 import live：以 `read_account` 胜出 kind 为准（Grok 嵌套 `auth.json` OAuth 一人一行；current 不得取槽位字母序最后一个，见 §4.6）  
- 本机同时有 Key 与官方登录：只导入胜出那份，另一份仅提醒（§4.6）

### 4.6 双凭据并存提醒

本机 live detect 可能同时看到 API Key 与官方登录（OAuth / CLI 登录态）。这是**提醒**，不是第二次导入，也不改入库规则。

| 环节 | 行为 |
|---|---|
| 探测 | `read_auth` 报告胜出的 `kind`（与 `read_account` 当前票一致），并用 `alsoPresent` 列出本机还在的另一族（脱敏；典型值 `oauth` / `api_key`）。仅一族时为空 |
| 导入 | `import_live` 仍以 `read_account` 的胜出 kind 为准（Grok 若 config.toml 有 API Key 则导入 `grok_bundle`，不从 `alsoPresent` 再合成一张票）。若胜出的是 Grok 嵌套 `auth.json` OAuth，则一人一行导入；**不得**用槽位字母序最后一个当 current：已有 current 且该人仍在文件里则保持；空池或 current 已不在文件则激活默认槽 `https://auth.x.ai::client`。Pi 多 provider 仍不抢全局 current。loopback / 本机路由投影不导入。显式同步始终以文件为准，不走 list 的 rt/mtime 双向对账。 |
| UI | Connections「导入当前登录」对话框用 Notice 标明会收入哪一份 |

**不**自动删除另一份，**不**把两份合成一张票。

各 Agent **运行时**认哪份由官方 CLI 决定（仅说明，不改变导入）：

- Claude：API Key 压过订阅  
- Grok：模型条目上的 Key 压过 session；全局 `XAI_API_KEY` 不压过  
- Kimi：看当前 provider  
- Codex TUI：多半认登录态  

---

## 5. 各 Agent 策略（authorization_key）

| Agent | AccountSwitch | v1 授权指纹原则 | identity_label（展示） |
|---|---|---|---|
| Claude | Full | API Key 材料 hash；勿用身份字段当票 | 脱敏 key / 自定义名 |
| Grok | Full | 可轮换的 token 材料 hash；**不要**只用 user_id | email / user_id |
| Pi | Full | 可 import；写回 API Key 需官方槽；自定义 URL+Key 走 Provider / models.json | provider + 展示 hint |
| Codex | Full | token 材料或整包 cred hash | 能抠则邮箱类 hint |
| Kimi | Full | 可轮换 token 优先，否则整包 | 有则邮箱类 hint |
| WorkBuddy | Unsup | 两边都不支持（无 import / 无 `add_api_key`） | — |
| Cursor | Unsup | 可 `add_api_key` 入池；`apply_account` 为 Unsupported（不写 live） | — |
| dsh | Partial | 可 import / API Key 切换，无 OAuth | — |

能力矩阵单元格不变；各家差异在 adapter 如何抽 **票** 的指纹，不在「能不能」。字段抽取细节以 adapter 源码为准，不在文档展开。

---

## 6. UI 建议（非阻塞 core）

> 核对日期：2026-08-22。以 [§8](#8-实现对照已落地) 为准：Connections 已是登录列表，**不要**再验收按身份分组。

现行 Connections 按**一份登录一行**列出。同 Agent OAuth 同人只一行；跨 Agent 或不同 Key 仍可多行。`identity_label` 仍可作行上展示标签，不是分组验收项。

（历史）曾建议同 `identity_label` 折叠分组，避免多条 `grok-oauth` 难辨：

```text
user@example.com
  ● 授权 2026-08-02 10:00:49  （当前）
  ○ 授权 2026-08-02 09:02:10
```

该分组 UI **未作为现行验收**（见 §8）；不是 `pages/accounts` 身份通讯录。

仍成立（与是否分组无关）：

- 展示：导入/更新时间、是否 current、后续可加 expired  
- 文案：池内可多授权；**当前生效仅一条**（写入 live 的那条）
- 导入当前登录时若本机同时有 Key 与官方登录：对话框 Notice 标明当前会收入哪一份（§4.6）；不自动删另一份

---

## 7. 明确不做什么（本方案范围）

| 项 | 说明 |
|---|---|
| 远端 revoke 旧 token | 覆盖/保留池记录 ≠ 吊销 IdP 会话 |
| 后台自动刷新守护 | 仍按 plan 范围外/后续 |
| 凭据落盘加密 | 项目范围外 |
| 把去重策略写进 Capability 枚举 | 禁止 |
| 会话设备管理完整产品 | 当前用「多行授权」表达即可，不做独立 sessions 表（除非未来需要） |

---

## 8. 实现对照（已落地）

| 项 | 位置 |
|---|---|
| `accounts_same_authorization` / `accounts_same_oauth_identity` | `account_service/surface.rs` |
| `AgentAdapter::authorization_key` / `identity_label` 默认实现 | `adapters/adapter_trait.rs` |
| import / add_api_key / create 去重 | `AccountService` |
| extra.identityLabel 写入 | `attach_identity_meta` |
| `groupAccountsByIdentity` | 仅 `account-map` 映射/测试残留；Connections 已是登录列表，**不要**再验收 `pages/accounts` 按身份分组 |
| 单测 | `account_service::tests`、`account-map.test.ts` |

| 行为 | 结果 |
|---|---|
| 同 Agent OAuth，subject 相同、token 不同 | **1 行**（覆盖） |
| 跨 Agent OAuth 同身份 | **2 行** |
| Pi 两 provider 同邮箱 | **2 行**（按 slot） |
| merge 自愈 | 同 authorization_key 冗余 + 同 Agent OAuth 同身份多余行 |
| 同 token / 同 api_key 双 import | **1 行** |
| API Key 不同指纹 | **2 行** |

---

## 9. 实施记录

### PR-A — 去重语义（2026-08-03；同 Agent OAuth 部分已由 2026-08-23 覆盖）

- [x] `accounts_same_authorization`（credentials 全等 + adapter 授权指纹）  
- [x] （历史）去掉 OAuth subject 合并 — **已废止于同 Agent OAuth**  
- [x] 自愈含同授权指纹  
- [x] （历史）测试：同人不同票 2 行 — **已改为同 Agent OAuth 1 行**

### PR-B — Adapter 契约

- [x] `authorization_key` + `default_authorization_key`  
- [x] `identity_label` + `default_identity_label`  
- [x] Service 经 adapter 调用；默认实现覆盖各家  

### PR-C — UI 分组（历史）

> 核对日期：2026-08-22。以 §8 为准：Connections 已是登录列表；下列映射已落地，**不再验收**按身份分组。

- [x] `identityLabel` / `createdAt` 映射（行上展示，不参与去重）  
- [x] （历史）`groupAccountsByIdentity` 仅 `account-map` 映射/测试残留；**不要**再验收 `pages/accounts` 或 Connections 按身份分组  
- [x] 多授权副文案：授权时间 / 「当前生效仅一条」（映射层仍保留；登录列表一行一份登录）  

### 验收清单

> 核对日期：2026-08-22。清单**不含**按 identity 分组 UI（已标（历史），见 §8）。

- [x] 同 live 连点两次 import → 1 条（单测）  
- [x] 同 Agent OAuth 同人不同 token → 1 条覆盖（单测）  
- [x] 跨 Agent 同身份 OAuth → 2 条（单测）  
- [x] Pi 两 provider 同邮箱 → 2 条（单测）  
- [x] 同授权冗余清理时保留其它**身份**（单测）  
- [ ] 手工：switch 后另一身份仍在（桌面端回归）  
- [x] 无新增「去重类」Capability  

### 2026-08-23 — OAuth 同一 Agent 同一身份一行

- [x] `accounts_same_oauth_identity` + merge/import/create/reconcile 覆盖同身份  
- [x] live reconcile 折叠同身份多余行  
- [x] `authorization_key` 默认仍按 token 指纹（Key / 精确 token 不变）

---

## 10. 风险

| 风险 | 缓解 |
|---|---|
| 列表变长、同名难辨 | identity 分组 + 时间戳 + 当前标记 |
| 用户误删仍 valid 的授权 | 删除需确认；展示非 current 也可保留 |
| access 轮换导致 auth_key 变 | 同 Agent OAuth 有稳定身份则覆盖原行；身份不明仍可能多一行 |
| 历史同身份多行 | list/import 时折叠为一条，保留 current 行 id |

---

## 11. 变更记录

| 日期 | 说明 |
|---|---|
| 2026-08-03 | 初版：产品定为「同人多授权并存」；去重仅限同授权票 |
| 2026-08-03 | PR-A/B/C 落地：service 去重、adapter 指纹、Connections 分组 UI |
| 2026-08-16 | 对照代码：补 dsh / Cursor 入池边界；§8 不再验收 `pages/accounts` 分组；Connections 已是登录列表 |
| 2026-08-18 | 双凭据并存提醒：`read_auth.alsoPresent` + 导入仍只收胜出 live 票 |
| 2026-08-18 | Pi：可 import；写回 API Key 需官方槽；自定义 URL+Key 走 Provider / models.json |
| 2026-08-22 | D1 核对：§6/§9 以 §8 为准，按身份分组 UI 标为（历史），不再验收 |
| 2026-08-23 | **废止**同 Agent OAuth「同人不同 token → 2 行」：按 Agent + 身份一行并覆盖；跨 Agent / Key / Pi slot 仍分行 |
