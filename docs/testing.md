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

历史遗留：部分旧模块仍有内嵌 `#[cfg(test)]`；**新代码与触达重构必须分文件**，不要扩大内嵌面。

## 2. 运行命令

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
| `route` / `support` / `canApply` / `ruleId` / `gateKind` / `reason` | analyze/plan 对外表面 |
| `applyPath` | 生产执行入口：`native`（`AdapterApplyService`）/ `local_bridge`（Tauri bridge controller）/ `rejected`（禁止 apply） |

**改矩阵 / reason / 白名单时必须先改或同步此 JSON**，再改：

1. `crates/agenthub-core` 的 `ADAPTER_CAPABILITY_MATRIX` / route service  
2. `src/dev/mocks/adapter.ts`  

两端测试：

- Rust：`cargo test -p agenthub-core shared_capability_contract`
- 前端：`pnpm test -- src/dev/mocks/adapter.test.ts`（`it.each(contract.cases)`）

快捷：`pnpm test:contracts`（含 boundary 全仓扫描 + backend features）。

## 7. 分层边界护栏

- 生产 module graph：`vite build` 禁止 `src/dev` / `src/test` / `*.test.ts` 入图。
- 源码扫描：`src/lib/backend/boundary-imports.test.ts`  
  - 仅 `lib/backend/tauri/**`（及 `lib/platform.ts` 的 `isTauri`）可 import `@tauri-apps`  
  - 仅 `lib/backend/tauri/invoke.ts` 可从 `@tauri-apps/api/core` 直接 import `invoke`  
  - `pages/**` / `lib/hooks/**` 不得 import `lib/backend/tauri` 或 `@/dev/*`
- 运行时：非 Tauri 调用 Tauri port → `BackendUnavailableError`（禁止静默 mock）。

## 8. CI

| 触发 | 工作流 | 内容 |
|---|---|---|
| PR / push `dev`·`main` | `.github/workflows/pr-ci.yml` | typecheck、全量 `pnpm test`、`cargo test -p agenthub-core` |
| push `release` | `.github/workflows/release.yml` | 上列 + workspace 全量 cargo + 打包发布元数据 |

本地等价：`pnpm test:pr`。

Bridge 半 e2e（端口重绑 / 上游轮转 / restore realign）见 `pnpm test:bridge` 与 [adapter-kimi-codex-dogfood.md](adapter-kimi-codex-dogfood.md) 自动覆盖对照表。

## 9. 提交前最低检查

改动触及对应层时至少跑：

1. 相关 `cargo test` / `pnpm test -- <path>`（过滤到本域）。
2. 若改了前端分层、Adapter 规则或边界：`pnpm test:contracts` 或 `pnpm test:pr`。
3. 若改了前端分层或依赖：`pnpm typecheck`（允许仅修本域错误，勿掩盖无关历史失败而不说明）。
4. 新增测试文件路径符合 §1；生产文件无内嵌测试体。
5. 改 Adapter 可执行表面时同步 `adapter-capability-contract.json`（含 `applyPath`）。
