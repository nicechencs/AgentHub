# 新增 Agent 步骤清单

> 当前状态：R00-R08 已完成。本文同时描述生产兼容接入轨和 AgentKey test-only 验证轨；生产组合仍保留 `AgentId`/`AgentAdapter` façade，不代表旧身份模型已删除。

> 更新：2026-08-15（补登记 `accepts` / `writer`，供票绑定规划器使用）
> 真源：`docs/platform-capability-refactor.md`、[platform-capability-remediation.md](platform-capability-remediation.md)、[connection-binding-model.md](connection-binding-model.md) 与本文件。
> **开闭验证（test-only）**：`crates/agenthub-core/src/platform/demo_agent_tests.rs` 使用独立 `AgentKey("demo-agent")`，不进入生产 registry，也不借用真实 AgentId。

本阶段目标：新增普通 Agent 时，**优先增加集成代码与稀疏端口**，而不是在平台 service / 页面里再写 `match` 分支。新增 Agent 采用生产接入与 test-only 验证双轨；两条轨都以 `AgentKey` 作为平台端口身份，但生产端在兼容期仍可由 `AgentAdapter`/`AgentId` façade 承接旧入口。

---

## 1. 接入双轨

### 1.1 生产接入轨（兼容期）

内置或正式发布的 Agent 当前仍需接入既有生产组合入口：

| 步骤 | 位置 | 说明 |
|---|---|---|
| 1. 兼容适配器 | `crates/agenthub-core/src/adapters/<id>.rs` | 兼容期实现 `AgentAdapter`；只实现该 Agent 真实拥有的能力 |
| 2. 注册 | `adapters/mod.rs` → `register_all()` | `reg.register(Arc::new(...))` **唯一生产注册点** |
| 3. 枚举 | `models/agent.rs` | `AgentId` 变体 + `ALL` + `as_str`/`parse`/`display_name` |
| 4. 前端 id | `src/lib/types.ts` + catalog 驱动列表 | 产品列表以 runtime catalog 为准；静态 `AGENTS` 仅过渡 |
| 5. key-native 端口 | `platform/*/registry.rs` | 以 `AgentKey` 注册 detector、install、config、skills、usage、stream、project 等实际支持的端口 |
| 6. 测试 | `cargo test -p agenthub-core` | adapter fixture + 已注册端口契约；禁止写死 agent 数量 |
| 7. 绑定入口 | `models/agent_capability.rs`：先登记 `accepts[]` + `writer` | 该 Agent 听什么协议/槽位、能否写 live。无 writer（如 Cursor）不能当 `bind` 落点。登记后由协议图长路由，不要再加一张「某商品 × 本 Agent」白名单。见 [connection-binding-model.md](connection-binding-model.md) |

> **禁止**：在 `platform/*` service、通用 utils、页面业务里新增具体 Agent 名称分支。  
> 差异只能进入：adapter / `platform/*/sources` 贡献 / descriptor / 明确兼容层。

### 1.2 test-only 验证轨（开放扩展）

用于证明新增 Agent 不依赖封闭枚举或真实 Agent 槽位：

- 在独立 `cfg(test)` 文件中创建 `AgentKey::parse("demo-agent")`；
- 注入 detector、声明式 install contribution 和至少一个可选平台端口；
- 通过真实 catalog/registry/lifecycle/service 主路径执行，安装边界使用 fake executor；
- 验证 duplicate key、unsupported capability、operation id、observed state 和生产 registry 排除；
- 测试不得修改 `AgentId`、`AgentId::ALL`、平台 service/page 分支或 migration。

当前参考实现为 `platform/demo_agent_tests.rs`，不是生产 Agent 注册方式。

---

## 2. 稀疏端口（按需；推荐）

平台能力已拆为可注入 registry。新 Agent **只注册自己支持的端口**：

| 端口 | 模块 | 何时实现 |
|---|---|---|
| 路径 | `platform/paths` | 几乎总是（home / config dir） |
| 安装贡献 | `platform/install` | 有 npm/native/setup 渠道时 |
| 流式解析 | `platform/stream` | 有 NDJSON/结构化 stdout 时 |
| 用量 | `platform/usage` | 有可解析用量日志时 |
| 项目源 | `platform/projects` + `project_service/sources` | 有本地会话/项目树时 |
| 配置投影 | `platform/config` | 可 schema 驱动读写原生配置时 |
| Skills 目标 | `platform/skills/target` | `skills_dir` 可用时（由 adapter + registry 派生） |
| Lifecycle | `platform/lifecycle` | 安装族操作走 coordinator（不改 runtime start/stop） |

**不支持**的能力：`Capability` 声明 `Unsupported`/`Planned`，平台调用返回 typed unsupported；**不要**伪装 Full。

---

## 3. 配置 / 连接 / Skills（当前实现）

- **配置**：`ConfigurationService` + `AgentConfigProjector`；schema/read/validate/apply；前端 `GenericConfigForm` + `Backend.config`。  
- **当前连接**：`agent_active_bindings` + `ConnectionService`；`accounts`/`providers.is_current` 仍双写兼容。  
- **Skills**：  
  - 来源/包：`SkillSourceService` / `SkillPackageService`（`platform/skills`）  
  - 分配：`skill_packages` / `skill_assignments` + `SkillAssignmentService` / `SkillReconciler`  
  - 门面：`SkillService`（`sync`/`disable`/`install`/`update` 仍可用）  
  - git 更新：**禁止** live `git pull`，使用 staging clone + 原子替换  

---

## 4. 前端

1. Agent 列表：优先 `list_agent_catalog` / catalog store，勿手同步 Rust 枚举业务规则。  
2. 配置表单：`getAgentConfigSchema` + `GenericConfigForm`（有 projector 的 Agent）。  
3. **仅** `src/lib/backend/tauri/` 可 `invoke`。  
4. mock 仅 `pnpm dev:mock` / 测试；生产不得静默 mock。  
5. 配色：`src/styles/tokens.ts`；勿在组件抄色值。

---

## 5. 测试约定

- 生产文件与测试文件分离：Rust 仅 `#[cfg(test)] mod tests;`，实现放 `tests.rs` / `*_tests.rs`。  
- 新 Agent 至少：adapter 单元测试 + catalog 覆盖 + 已注册端口契约测试。  
- 开闭自检：`cargo test -p agenthub-core --lib demo_agent`（仅测试代码注入 `demo-agent`）。  
- 本域：`cargo test -p agenthub-core --lib skill` / `platform::` 等过滤后提交。

---

## 6. 不要做

- 不在 Skills 矩阵 / Sidebar 等写死 N 列。  
- **不**做凭据落盘加密（keyring/AES/主密码）——项目范围外。  
- 不平行维护第二套 Agent 枚举业务规则。  
- 不把 Planned 渠道写进可执行 install_channels。  
- 不把 test-only agent 写进 `register_all()` / migration / 生产 UI。

---

## 7. Definition of Done

- [ ] `register_all()` 含新 adapter；`AgentId::ALL` 与 catalog 生产列表一致  
- [ ] 每个 `Capability` 有诚实 `capability()` 答案  
- [ ] 支持的端口已 `register`，平台 service **无**新 agent 分支  
- [ ] `cargo test -p agenthub-core` 通过；触及前端时 `pnpm test` + `pnpm exec tsc --noEmit`  
- [ ] doctor / Agents 页可 detect / 安装；未安装时其它页不假成功  

---

## 8. 未完成 / 暂缓（诚实标注）

| 项 | 状态 |
|---|---|
| 删除 `AgentId` / 胖 `AgentAdapter` | 暂缓；兼容层仍在 |
| `integrations/agents/<key>/` 物理目录 | 目标布局；贡献仍多在 `platform/*/sources` |
| Skills `projection_mode=link` 进 reconciler | 库字段已有；reconcile 仍以 copy/sync 语义为主 |
| `install_skill` 自动写 `skill_packages` | 首次 sync/bootstrap 写入；可后续收紧 |
| Bootstrap 扫描真实 `~/.agents` 于 open | **故意不**自动扫描用户目录 |
| 前后端契约 codegen / xtask new-agent | 未授权，不做 |
| 凭据落盘加密 | **无必要 / 范围外** |

## 9. 建议后续 cleanup PR（勿在本阶段执行）

1. 删除已无调用者的 provider-detect agent 分支（CodeGraph callers 证明后）  
2. 统一 market install 路径到 `SkillSourceService`  
3. 删除 `is_current` 字段（binding 稳定后单独 migration）  
4. 物理目录迁入 `integrations/agents/*`（仅移动、不改行为）
