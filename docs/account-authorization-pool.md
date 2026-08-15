# 账号池：身份 × 授权 与去重方案

> 状态：**产品决策已定；PR-A/B/C 已落地（2026-08-03）**。  
> 关联：[capability-matrix.md](capability-matrix.md)（Agent 原生能力）、[provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)（协议图上的边）、[connection-binding-model.md](connection-binding-model.md)（票 / 绑定）、[architecture.md](architecture.md)、[cli-and-config.md](cli-and-config.md)（account 命令）。
> **废止**：此前「同 email/user_id 必合并为一条」的草案；以本文为准。

## 1. 产品决策（硬约束）

### 1.1 两个概念

| 概念 | 含义 | 例子 |
|---|---|---|
| **身份（Identity）** | 谁 | email、user_id、principal_id |
| **授权（Authorization）** | 一次授权拿到的那组凭据（一张「票」） | 一组 refresh/access token，或一把 API Key |

### 1.2 核心规则

1. **同一身份可以保留多次授权**（多条池记录）。
2. **尤其多枚 token 都仍有效时，必须分别保留**，不得因「同一个人」而合并或删除。
3. **去重只针对「同一张授权票」**（同 token / 同 API Key 指纹），不是针对身份。
4. 每个 agent 的 **live 生效位仍只有一个**（`is_current` 至多一条）；其余授权留在池内供切换。

| 场景 | 池内条数 | 说明 |
|---|---|---|
| 同 live / 同 token 再 import 一次 | **1** | 同授权 → upsert |
| 同一人，两次 OAuth，**token 不同**（皆可能 valid） | **2+** | 不同授权 → 并存 |
| 同一人，API Key A 与 Key B | **2** | 不同密钥 |
| 不同人 | **2+** | 不同身份 |

一句话：

> **身份可以重复出现在列表里；只有「同一张授权票」才去重。  
> 多枚仍有效的 token = 多条池记录；本机 live 仍只有一个 current。**

跨 Agent 复用时，这里的「授权」就是 [票（Ticket）](connection-binding-model.md)。`agent_id` / 从哪个 Agent 导入只表示出身，**不**决定能不能 `bind` 到别的 Agent。每个 Agent 的 active 绑定至多一条，对应本节的 live 生效位。

---

## 2. 与能力矩阵的边界

沿用 [capability-matrix.md §3](capability-matrix.md)：

| 问题 | 归谁 | 例子 |
|---|---|---|
| 能不能导入 / 切换账号？ | **能力矩阵** | `AccountSwitch`、`ApiKeyAccount` |
| 两份凭据是不是同一张「票」？ | **Adapter `authorization_key`** | token / api_key 指纹 |
| 命中后 create 还是 update？ | **AccountService** | 仅按授权指纹 merge |
| 列表怎么按人分组展示？ | **UI + 可选 `identity_label`** | 仅展示，不参与去重 |

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
  4. 同 agent + 同 kind 下查找 auth_key 命中行
       命中 → update 该行（import 时可 mark current）
       未命中 → create 新行
  5. 自愈：仅删除「同 auth_key」的冗余行
     禁止按 email/user_id 删除其它授权行
        │
        ▼
AgentAdapter
  authorization_key  → 这张票的指纹（去重）
  identity_label     → 可选，仅展示/分组（同人多行可相同）
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

/// 身份展示标签（可选）。同人多授权可返回相同值；禁止用于去重/删除。
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

`identity_label` 默认：扫 email / user_id / label_hint，仅供 UI。

### 4.2 键格式建议

```text
{kind}:{strategy}:{value}
```

示例：

- `apikey:sha256:a1b2…`
- `oauth:refresh_sha:c3d4…`
- `oauth:cred_sha:e5f6…`（无法抽出 refresh/access 时）

### 4.3 Service 匹配优先级

同 `agent_id` + 同 `kind`：

1. **`authorization_key` 双方都有且相等** → 同一授权  
2. **完整 `credentials` JSON 相等** → 同一授权（re-import 兜底）  
3. 否则 → **新授权**，`create`

### 4.4 merge 行为

| 入口 | 命中同授权后 | 其它行 |
|---|---|---|
| `import_live` | update credentials/label/extra；`is_current=true` | **不动**（除非同 auth_key 冗余） |
| `add_api_key` | update；不强制 current | 不动 |
| OAuth 完成入库 | 同 import 策略（按实现入口） | 不动 |

**自愈范围（收窄）：**

- 仅删除：同 agent + 同 kind + **同 authorization_key** 的多余行  
- **禁止**：因 subject/email 相同删除其它行  

### 4.5 current 与 live

- 每 agent 至多一条 `is_current=1`（现有 repo 不变量）  
- switch：把选中授权写入 live，并设为 current；**池内其它授权行完整保留**  
- 再 import live：只 upsert 与 **当前 live 授权指纹** 相同的那一行  

---

## 5. 各 Agent 策略（authorization_key）

| Agent | AccountSwitch | v1 授权指纹原则 | identity_label（展示） |
|---|---|---|---|
| Claude | Full | API Key 材料 hash；勿用身份字段当票 | 脱敏 key / 自定义名 |
| Grok | Full | 可轮换的 token 材料 hash；**不要**只用 user_id | email / user_id |
| Pi | Full | provider 键控 body 中的票材料 hash | provider + 展示 hint |
| Codex | Full | token 材料或整包 cred hash | 能抠则邮箱类 hint |
| Kimi | Full | 可轮换 token 优先，否则整包 | 有则邮箱类 hint |
| WorkBuddy | Unsup | 不走 import | — |
| Cursor | Unsup | 不走 import | — |

能力矩阵单元格不变；各家差异在 adapter 如何抽 **票** 的指纹，不在「能不能」。字段抽取细节以 adapter 源码为准，不在文档展开。

---

## 6. UI 建议（非阻塞 core）

同 `identity_label` 可分组，避免多条 `grok-oauth` 难辨：

```text
user@example.com
  ● 授权 2026-08-02 10:00:49  （当前）
  ○ 授权 2026-08-02 09:02:10
```

- 展示：导入/更新时间、是否 current、后续可加 expired  
- 文案：池内可多授权；**当前生效仅一条**（写入 live 的那条）

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
| `accounts_same_authorization` | `account_service.rs` |
| `AgentAdapter::authorization_key` / `identity_label` 默认实现 | `adapters/mod.rs` |
| import / add_api_key / create 去重 | `AccountService` |
| extra.identityLabel 写入 | `attach_identity_meta` |
| UI 按身份分组 | `groupAccountsByIdentity` + `pages/accounts` |
| 单测 | `account_service::tests`、`account-map.test.ts` |

| 行为 | 结果 |
|---|---|
| subject 相同、token 不同 | **2 行**（不合并） |
| merge 自愈 | **只** 清同 authorization_key 冗余 |
| 同 token / 同 api_key 双 import | **1 行** |

---

## 9. 实施记录

### PR-A — 去重语义

- [x] `accounts_same_authorization`（credentials 全等 + adapter 授权指纹）  
- [x] 去掉 OAuth subject 合并  
- [x] 自愈仅限同授权指纹  
- [x] 测试：同票 1 行 / 同人不同票 2 行 / 同 key 1 行  

### PR-B — Adapter 契约

- [x] `authorization_key` + `default_authorization_key`  
- [x] `identity_label` + `default_identity_label`  
- [x] Service 经 adapter 调用；默认实现覆盖各家  

### PR-C — UI 分组

- [x] `identityLabel` / `createdAt` 映射  
- [x] Connections 账号列表按身份分组  
- [x] 多授权副文案：授权时间 / 「当前生效仅一条」  

### 验收清单

- [x] 同 live 连点两次 import → 1 条（单测）  
- [x] 同人不同 token → 2 条（单测）  
- [x] 同授权冗余清理时保留其它授权（单测）  
- [ ] 手工：switch 后另一授权仍在（桌面端回归）  
- [x] 无新增「去重类」Capability  

---

## 10. 风险

| 风险 | 缓解 |
|---|---|
| 列表变长、同名难辨 | identity 分组 + 时间戳 + 当前标记 |
| 用户误删仍 valid 的授权 | 删除需确认；展示非 current 也可保留 |
| access 轮换导致 auth_key 变（新行） | 优先 refresh_token 入指纹；文档承认极端情况下可能多一行 |
| 与「同人一条」旧直觉冲突 | 产品文案写清：管理的是授权，不是通讯录去重 |

---

## 11. 变更记录

| 日期 | 说明 |
|---|---|
| 2026-08-03 | 初版：产品定为「同人多授权并存」；去重仅限同授权票 |
| 2026-08-03 | PR-A/B/C 落地：service 去重、adapter 指纹、Connections 分组 UI |
