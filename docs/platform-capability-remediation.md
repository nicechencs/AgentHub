# AgentHub 平台能力改造审查修正方案

> 状态：最终收口记录；R00-R08 已完成，最终代码收口见 `9f57354`、`f2658bb`、`de779c1`、`930270b` 及此前 R01-R07 提交。
> 审查日期：2026-08-07
> 审查范围：`053321d..e149933` 基线，以及 R00-R08 修正提交
> 文档用途：保留最终约束、实际结果、暂缓项和验证证据；已执行的阶段提示词与任务拆分已清理。

## 1. 结论

现有改造方向已经完成收口：项目仍是模块化单体，Catalog、Lifecycle、Configuration、Connection、Skills、Usage、Projects 等平台能力保持清晰边界。R00-R08 已关闭本轮审查发现的安全与扩展性阻塞项，并通过真实的 test-only `demo-agent` 验证开闭原则。

本轮已验证的结果包括：

1. 平台扩展端口和 registry 以 `AgentKey` 为主路径；`AgentId` 仅保留在旧 API、旧数据库 DTO 或兼容 façade 边界。
2. `demo-agent` 使用独立的 `AgentKey`，实现 detector、install contribution 和配置端口测试，不进入生产 registry，也不借用真实 AgentId。
3. Active Binding、配置 fail-closed、Skills 所有权/原子更新和 Lifecycle operation 审计的失败路径已有实现与测试。
4. Lifecycle 提供 key-native detailed API，保留旧 `InstallOutcome`/`AgentId` façade 以兼容现有 CLI、Tauri 和内置 Agent。

以下兼容边界是已知且有意保留的后续迁移项，不构成本轮失败：默认 Lifecycle executor 仍通过内置 `AgentAdapter` 执行；`SkillService` 的主 list/sync/project façade、Usage 非空数据持久化和 RunService 仍有 `AgentId`/legacy 组合边界；Project 的 legacy DTO/ID 解析仍有 `AgentId` 边界。它们可在后续独立迁移，不应通过本轮文档声称已完成全链路 key-native。

本轮修正继续坚持“按平台能力划分模块”。不引入微服务、DDD、CQRS、事件总线、动态插件 ABI 或新的 Marketplace。

## 2. 修正目标与实际结果

以下不变量已由 R01-R07 的实现、契约测试和最终复审验证；本节保留目标表述，作为后续演进的约束。

### 2.1 Agent 扩展

- `AgentKey` 是平台扩展端口和 registry 的原生主键。
- 平台 registry 不得通过 `AgentId::parse(key)` 查询扩展实现。
- `AgentId` 只保留在旧 API、旧数据库 DTO 或兼容 façade 边界。
- 新增测试 Agent 时，不修改 `AgentId`、`AgentId::ALL`、平台 service 或页面分支。
- registry 重复 key 必须返回错误，不能静默覆盖已有实现。

### 2.2 Active Binding

- `ConnectionService` 是 binding 与 legacy current 镜像的唯一事务协调者。
- create/import/update/switch/clear/delete 任一入口都不能留下双 current 或悬空 binding。
- 旧 `accounts.is_current`、`providers.is_current` 暂时保留，但只作为兼容镜像。
- 不在本轮删除旧字段，也不重做凭据、Provider、Model 数据模型。

### 2.3 Configuration

- Catalog 明确声明存在 projector 的 Agent，schema 加载失败时必须 fail closed。
- loading、unsupported、read error、validation error、apply error、success 是不同状态。
- 后端暂时不可用时，不得降级到旧 parser 并继续保存。
- 只有 Catalog 明确声明没有 projector 的兼容 Agent 才允许使用旧 façade。
- 前端不吞掉 JSON/TOML 解析错误，不把非法正文默认为空对象。

### 2.4 Skills

- 只有平台 marker、平台 assignment 及匹配的内容指纹，或可验证的受管链接，才能证明投影归平台所有。
- “文件内容碰巧相同”不能作为历史托管证明。
- 禁用或卸载前必须重新验证所有权；无法证明时返回 conflict，绝不递归删除。
- source 内容、`.skill-lock.json` 和 `skill_packages.revision` 在主提交阶段保持一致。
- 共享包提交后，单 Agent 投影失败不回滚共享包，但必须写入该 assignment 的 observed error。

### 2.5 Lifecycle

- 同一次生命周期调用产生的所有进度事件使用同一个非空 `operation_id`。
- execute 前的 operation 记录/步骤持久化失败时，不执行外部副作用。
- execute 后 finalize 失败时，返回明确的“外部操作结果已发生但审计持久化失败”，不能报告普通成功。
- operation 最终状态与 Detector 的 observed state 分开表达。

## 3. 平台能力归属

| 问题 | 唯一责任模块 | 推荐放置 | 禁止放置 |
|---|---|---|---|
| Agent 标识与扩展发现 | `platform/agent_catalog` + 各 capability registry | `platform/<capability>/registry.rs` | 页面、通用 utils、具体 Agent match |
| Detect / install 生命周期 | `platform/lifecycle`、`platform/install` | key-native coordinator 与窄 Detector port | Account/Provider service、全局事件总线 |
| current/binding 一致性 | `ConnectionService` | `services/connection_service.rs` + repo transaction helpers | Account/Provider 的 best-effort side effect |
| 配置 schema 与原生投影 | 后端 `platform/config` | projector、schema、materialize | React 页面中的 JSON/TOML Agent 分支 |
| 配置表单流程 | 前端 Configuration feature | Provider dialog 的薄编排 + feature-local form flow | `shared` 中的后端业务规则 |
| Skill 投影所有权 | `platform/skills` | ownership marker/store + reconciler | 页面、Agent adapter 自行维护 enabled 真相 |
| Skill 来源与原子更新 | `platform/skills` | sources/packages/lockfile/update transaction | `SkillService` 中继续复制新实现 |
| operation 审计和进度 | `platform/lifecycle` | coordinator、operation repo、per-call sink | 全局事件总线、后台工作流引擎 |

## 4. 验收结果

### 安全闸门 A（已通过）

R01-R05 已完成并确认：

- current 的所有写路径和删除路径都有事务测试；
- 配置后端失败不会执行 legacy save；
- 无 marker 的 Skill 目录不会被删除；
- lock 写失败后旧 Skill 内容与旧 revision 仍一致；
- lifecycle persistence failure 不会被报告成普通成功。

R01-R05 的实现和测试已满足上述条件，随后完成 R06 的跨端口 AgentKey 改造。

### 架构闸门 B（已通过）

R06-R07 已完成并确认：

- 平台 registry 原生以 AgentKey 查询，`get_key` 不再转回 AgentId；
- demo-agent 不借用任何真实 AgentId；
- key-native lifecycle 能通过 fake executor 完成并产生 operation；
- production registry 和 UI 不出现 demo-agent；
- 新 Agent 测试没有修改平台 service/page 分支。

R06A-R07 已满足上述条件；生产组合仍保留 `AgentId`/`AgentAdapter` 兼容 façade，不能据此推断旧身份模型已删除。

## 5. 明确暂缓项

以下内容明确暂缓，不属于本轮修正结果，也不应被视为已完成：

- 删除 `AgentId`、`AgentAdapter` 或所有旧 façade；
- 删除 accounts/providers 的 `is_current` 字段；
- 凭据落盘加密、keyring、AES、主密码或密文迁移；
- Prompt、MCP/Tool、Agent Marketplace；
- 动态库插件、脚本插件 ABI、代码生成框架；
- 微服务、DDD、CQRS、事件总线、工作流引擎；
- Provider/Connections UI 重设计；
- 全仓目录搬迁、全仓 rename 或全仓格式化；
- 为了“shared”整洁而提前移动尚未稳定的业务组件。

后续可独立评估但不阻塞本轮：`GenericConfigForm` 从根 `shared` 移入 Configuration feature、继续拆分 `SkillService` hotspot、清理 Usage/Project legacy dead code、删除 `is_current` 的最终 migration。

## 6. 最终验证摘要

R00-R08 收口采用以下全量验证命令：

```text
cargo test --workspace
pnpm test
pnpm typecheck
pnpm build

```

- `cargo test --workspace`：通过。
- `pnpm test`：通过（295 tests）。
- `pnpm typecheck`：通过。
- `pnpm build`：通过；仅保留既有 bundle 大小提示。
- `pnpm typecheck:test`：通过（补齐 browser mock 的 `CoreSkill` 返回类型）。
- `git diff --check`：收口文档与代码变更无新增空白错误。

`cargo fmt --all -- --check` 当前仍可能报告历史基线差异；本轮不以全仓格式化掩盖该差异。
