# 测试约定

> 真源：本文件。Agent / 贡献者在写或改测试时遵守以下规则。  
> 相关分层见 [architecture.md §4](architecture.md)；能力矩阵一致性见 [capability-matrix.md](capability-matrix.md)。

## 1. 硬约束：测试与生产分文件

**测试代码不得与生产代码写在同一个源文件里。**

| 层 | 生产文件 | 测试位置（示例） |
|---|---|---|
| `agenthub-core` service | `services/foo_service.rs` | `services/foo_service/tests.rs`（生产侧仅 `#[cfg(test)] mod tests;`） |
| `agenthub-core` 其它模块 | `adapters/mod.rs` 等 | 同目录 `tests.rs` 子模块，或独立 `*.rs` 测试模块文件 |
| `agenthub-gui` Tauri 命令 | `src-tauri/src/commands/foo.rs` | `src-tauri/src/commands/foo/tests.rs`（生产侧仅 `#[cfg(test)] mod tests;`） |
| 前端 TS/TSX | `src/**/bar.ts` | 同目录 `bar.test.ts` / `bar.spec.ts`，或领域旁的 `*.mapstatus.test.ts` 等 |

允许：

- 生产文件末尾 **一行** 声明：`#[cfg(test)] mod tests;`（实现必须在独立文件）。
- 前端 `src/test/setup.ts`、`src/test/factories/*` 作为共享测试基础设施（不进生产 bundle；`vite build` 会拦 `src/test`）。

禁止：

- 在生产 `.rs` / `.ts` / `.tsx` 内嵌大段 `#[cfg(test)] mod tests { ... }` 或 `describe(...)`。
- 为方便测试向生产 façade 导出 `__reset*ForTests` 一类 hook（重置逻辑放 `dev/mocks`）。
- 让 `pnpm build` / 生产 module graph 依赖测试或 mock 文件（见 architecture 生产护栏）。

历史遗留（与本条约不一致，**不要扩大**）：`crates/agenthub-core/src/logging/mod.rs` 仍内嵌 `#[cfg(test)] mod tests { ... }`；部分 GUI commands 仍把测试写在生产文件里（如 `src-tauri/src/commands/chat.rs`、`settings.rs`、`account.rs`、`backup.rs`、`project.rs`，以及 `tray.rs` / `window_policy.rs`）。**新代码必须分文件**。

## 2. 运行命令

Agent 协作：跑测试、汇总日志等机械步骤由 subagent 执行，主 Agent 只看结论并验收。见 [AGENTS.md § Agent 协作规则](../AGENTS.md#机械任务必须交给其他 Agent)。

```bash
# 前端（始终走 mock backend：vitest `#backend` alias）
pnpm test
pnpm test -- src/lib/api/skill.markdown.test.ts

# core
cargo test -p agenthub-core
cargo test -p agenthub-core read_skill_markdown

# GUI 命令层（含 commands/*/tests.rs）
cargo test -p agenthub-gui
cargo test -p agenthub-gui commands::skill::tests
```

## 3. 前端测试约定

- 环境：`vitest`，`environment: 'node'`（默认不渲染 DOM；逻辑 / 契约优先）。
- Backend：`vitest.config.ts` 将 `#backend` 指到 `dev/mocks/create-backend.ts`；`src/test/setup.ts` 在每例前 `resetBackend()`。
- 领域重置：需要会话/项目状态隔离时，调用 `dev/mocks` 的 `reset*`，不要从 `lib/api` 导出测试专用 API。
- 命名：`*.test.ts` / `*.spec.ts`；与源文件并列或使用清晰领域后缀（如 `skill.markdown.test.ts`）。

## 4. Rust 测试约定

- Service / 命令逻辑测 **inner / 公开 service API**，不测 Tauri async command 壳本身（壳只做 `with_hub_blocking` + 参数解析）。
- 需要临时目录时用 `tempfile`；不要写仓库内真实用户路径。
- 路径断言注意 Windows 大小写不敏感：文件名比较优先 `eq_ignore_ascii_case`。
- 错误码断言用 `AppError::code()` 或 GUI 字符串中的可读片段，避免绑死完整 Display 文案。

## 5. Markdown 预览相关用例（当前）

| 范围 | 文件 | 覆盖点 |
|---|---|---|
| Core 读 `SKILL.md` | `crates/agenthub-core/src/services/skill_service/tests.rs` | 共享/私有读取、非法 id、小写 `skill.md`、缺文件、大文件截断、DTO serde |
| GUI 命令 inner | `src-tauri/src/commands/skill/tests.rs` | `read_skill_markdown_inner` 成功/缺失/非法 agent；既有 skill 命令回归 |
| 前端 façade + mock | `src/lib/api/skill.markdown.test.ts` | 共享/私有预览、未知 id、DTO 字段契约 |

UI 组件（`MarkdownView` / 预览对话框）以库 `@uiw/react-markdown-preview` 为主；单测优先契约与 service，不强制 jsdom 快照。

## 6. 跨层契约（Adapter 路由）

真源用例表：

`src/dev/mocks/fixtures/adapter-capability-contract.json`

| 字段 | 含义 |
|---|---|
| `route` / `support` / `canApply` / `maturity` / `ruleId` / `gateKind` / `reason` / `reusePath` | `plan()` 是唯一规划出口；analyze 仍给 route/support/reason 主旨。`canApply` = 矩阵开放 ∩ write_gate。`reusePath` 由 `plan()` 派生，是用户三路的展示字段；三路不是领域枚举，展示走 `plan.reusePath` |
| `applyPath` | 生产执行入口：`native`（`AdapterApplyService`）/ `local_bridge`（Tauri bridge controller）/ `rejected`（禁止 apply） |

**改矩阵 / reason / write_gate / `reusePath` 可写路径时必须先改或同步此 JSON（JSON 也要带 `reusePath`）**，再改：

1. `crates/agenthub-core` 的 `ADAPTER_CAPABILITY_MATRIX` / route service  
2. `src/dev/mocks/adapter.ts`  

两端测试：

- Rust：`cargo test -p agenthub-core shared_capability_contract`
- 前端：`pnpm test -- src/dev/mocks/adapter.test.ts`（`it.each(contract.cases)`）

快捷：`pnpm test:contracts`（含 boundary 全仓扫描 + backend features）。

## 7. ConnectFlow / Hub 入口

Hub Phase 1 统一连接流程的测试分文件存放（遵守 §1）；前端 vitest 固定走 mock backend（`#backend` → `dev/mocks`），领域 reset 放 `dev/mocks`，不要往生产 façade 塞测试 hook。

| 层 | 文件 | 覆盖点 |
|---|---|---|
| 逻辑 | `src/lib/connect-flow/eligibility.test.ts` | 候选资格 |
| 逻辑 | `src/lib/connect-flow/plan-fanout.test.ts` | plan fan-out |
| 逻辑 | `src/lib/connect-flow/connection-usage.test.ts` | 用途聚合 |
| 逻辑 | `src/lib/connect-flow/default-deps.test.ts` | 默认依赖组装 |
| 逻辑 | `src/lib/connect-flow/reuse-offer.test.ts` | 真登录常驻「分享 / 路由」，只排除自动生成的配置与非登录行；不可行在 ConnectFlow 置灰 + 原因，见 [connection-binding-model.md](connection-binding-model.md) |
| 逻辑 | `src/lib/connect-flow/connect-intent.test.ts` | 导入登录 / 新 API Key 引导深链（intent/resume/`/?connect=` 的 parse/build/consume） |
| 状态机 | `src/components/connect/connect-flow-state.test.ts` | 对话框状态机；purpose share/route 过滤可见目标 |
| UI | `src/pages/connections/TicketWalletList.test.tsx` | 无类型芯片；行上「分享 / 路由」；OAuth 刷新由 oauthListAction 驱动 |
| 契约 | `src/lib/backend/contracts/account-actions.test.ts` | oauthListAction |

可行性权威为 `plan()` 的 route / maturity / canApply / reason。`canApply` 表示**现在能写入**（有 bind 实现且 secret 可按登录 `source_kind` 解析），禁止只测 `analysis.support`，也不要恢复商品白名单。不可行组合仍应覆盖「原因原文可见」，不要用「按钮不存在」代替规划结果。Account 与同表面 Provider 应断言相同 route/support/reason 主旨。Kimi Code 会员 Account → Claude / Pi / Codex / Grok、OpenAI API Account → Pi / Grok、Anthropic API Account → Pi / Codex、GLM Coding Plan / DeepSeek API Account → Claude / Pi / Codex、Claude/Codex/Grok OAuth Account → Pi 的 `canApply` 与同表面实现相同且可写；Kimi Account 必须是 `kind=apikey` 且 credentials 为 `format=api_key` + `api_key`；GLM/DeepSeek → Pi 还应断言自定义槽的 `baseUrl`、`api`、`models` 与 secret materialize/scrub，→ Codex 应断言官方 Responses URL、`wire_api=responses`、TOML 与 secret materialize/scrub；Kimi/OpenAI → Grok 应断言官方 Chat URL、`api_backend=chat_completions`、TOML 与 secret materialize/scrub；Codex 订阅 → Grok 本机路由应断言 `api_backend=responses`；带 `access_token` 的 Codex `auth_json` 或 Grok OAuth Account → Claude 使用 `local_bridge`、`canApply=true`、`reusePath=local_bridge`，Grok 上游为 xAI Responses（cli-chat-proxy）；Claude 订阅 → Codex 已于 2026-08-21 改判为可路由（规划），落地前断言 `canApply=false` 且 reason **不再含「产品不做」**，而是未取证/未实现类原因；Codex App Server、OauthOther 或缺少 `access_token` 的订阅目标仍为不可写；其它 Account 仍为 false。确认步测 `bind`（成功判据为该 Agent 的 active 绑定），不要再断言 `applyAdapter`。

## 8. 分层边界护栏

- 生产 module graph：`vite build` 禁止 `src/dev` / `src/test` / `*.test.ts` 入图。
- 源码扫描：`src/lib/backend/boundary-imports.test.ts`  
  - 仅 `lib/backend/tauri/**`（及 `lib/platform.ts` 的 `isTauri`）可 import `@tauri-apps`  
  - 仅 `lib/backend/tauri/invoke.ts` 可从 `@tauri-apps/api/core` 直接 import `invoke`  
  - `pages/**` / `lib/hooks/**` 不得 import `lib/backend/tauri` 或 `@/dev/*`
- 运行时：非 Tauri 调用 Tauri port → `BackendUnavailableError`（禁止静默 mock）。

## 9. CI

| 触发 | 工作流 | 内容 |
|---|---|---|
| PR / push `dev`·`main` | `.github/workflows/pr-ci.yml` | `pnpm typecheck`、`pnpm typecheck:test`、全量 `pnpm test`、`cargo test -p agenthub-core` |
| push `v*` tag | `.github/workflows/release.yml` | 更严：`pnpm typecheck` + `pnpm typecheck:test` + 全量 `pnpm test` + `cargo test --workspace` + 打包发布元数据；tag 必须指向 `release` 上的提交且与三文件版本一致 |

本地等价：`pnpm test:pr`。

Bridge 半 e2e（端口重绑 / 上游轮转 / restore realign）见 `pnpm test:bridge` 与 [adapter-kimi-codex-dogfood.md](adapter-kimi-codex-dogfood.md) 自动覆盖对照表。

## 10. 提交前最低检查

改动触及对应层时至少跑：

1. 相关 `cargo test` / `pnpm test -- <path>`（过滤到本域）。
2. 若改了前端分层、Adapter 规则或边界：`pnpm test:contracts` 或 `pnpm test:pr`。
3. 若改了前端分层或依赖：`pnpm typecheck`（允许仅修本域错误，勿掩盖无关历史失败而不说明）。
4. 新增测试文件路径符合 §1；生产文件无内嵌测试体。
5. 改 Adapter 可执行表面时同步 `adapter-capability-contract.json`（含 `applyPath`）。
