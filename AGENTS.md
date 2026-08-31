# AgentHub 项目约定

本文件是每次任务都会加载的工程红线。不要把它当成开工阅读清单：贡献步骤、文档分类、架构说明、测试命令只在对应任务需要时再打开。

## 红线

- 日常开发和 PR 合入 **`dev`**。不要直接在 `release` 上提交；`release` 只用于正式发版。
- 凭据落盘加密、国产 OAuth 适配、把 OAuth 转成 API：范围外，不得列为待办或实施任务。
- 对用户说话、写界面和提示时，用对方已经在用的词或界面上的词。

## 对用户说话

默认读者不是本仓库的作者。同时约束：**回复**、**界面/提示**、**用户会读到的文档**。代码、测试和架构说明仍用内部名称，不要改名。

- 先说完：做了什么、现在怎样、接下来要不要动手。
- 只用用户说过的词，或界面上已有的词（登录、连接、路由、供应商、共享库、会话）。内部名第一次用人话解释。
- 提示要短；实现细节不写进弹窗。改界面文案时再对照 [术语表](docs/reference/terminology.md) 的「用户界面术语」列。
- 对用户不要直接用：
  - 票 / Ticket / 绑定 → 登录、接到某个 Agent
  - 凭据 → 登录信息或 API Key
  - 真源 / 投影 / 映射 → 共享库、同步到某个 Agent
  - live → 本机正在用的配置
  - Adapter / façade / wire / 桥 → 对接方式、本机转发、路由
- 不要自造缩写或听起来很专业的标签。用户问「通俗地说」时，用人话重讲，不要再解释内部词。

## 分支与发布

- 日常合入 `dev`，不要把 `main` 或 `release` 当日常集成线。**禁止直接在 `release` 上提交。**
- 发版：只改 `package.json` 的 `version`，运行 `pnpm release:sync-version`；在 `dev` 升版并写 `CHANGELOG.md` → 合入 `release` → 在 **`dev` 打并推送 `vX.Y.Z` tag**。已发布的 tag 不可覆盖。
- 发版逐步操作需要时再打开 [CONTRIBUTING.md](CONTRIBUTING.md)。
- Agent 隐藏以 `dev` 的 store-stamp 为准（当前 `store_stamp_version = 1` 默认软隐藏 **Cursor Agent**），只影响界面。用户可在 Agents 管理页取消隐藏。

## 前端 backend 分层

改调用边界或目录时再打开 [架构总览](docs/architecture/overview.md)。目录以源码为准：

```text
src/
├─ app/runtime/                 # 应用组合入口
├─ lib/backend/
│  ├─ contracts/                # DTO、接口、纯映射
│  ├─ tauri/                    # 唯一允许调用 invoke 的地方
│  └─ current.ts                # 默认生产实现
├─ dev/mocks/                   # browser mock（backend / account / chat / fixtures）
├─ test/                        # setup.ts
├─ lib/api/                     # 兼容 façade，页面可渐进迁移
└─ pages/
```

| 命令 | 含义 |
|---|---|
| `pnpm dev` | 普通 Vite，不启动 Tauri |
| `pnpm tauri:dev` | 真实桌面后端 |
| `pnpm dev:mock` | 仅演示与测试 |
| `pnpm build` | **强制** 真实桌面后端 |

- **仅** `src/lib/backend/tauri/` 可以调用 `invoke`；页面不得直接调用。`src/lib/api/` 是过渡层。
- mock 只服务 `pnpm dev:mock` 和测试，不得进入生产构建。
- 非 Tauri 的生产页面必须明确报错或显示 **unavailable**，禁止静默回退到 mock。
- 产品写入走 `src/lib/api/tickets` 的 `plan` / `bind` / `unbind`；`src/lib/api/adapter` 只服务预览与本机路由运行时。

## 凭据存储

没有必要做凭据落盘加密，沿用现有存储。讨论待办时标为「无必要 / 项目范围外」，不得据此推导 keyring、AES、主密码或密文迁移。用户明确推翻前不要讨论。

## 国产 OAuth

不为中国产 AI 的 OAuth 开 Adapter 边，也不把 OAuth 转成 API 或伪装成 API Key。现有国产路由只认官方 API Key 登录。不得把现有 Key 边扩成 OAuth 边。用户明确推翻前不要讨论。

## 测试

选命令时再打开 [测试与验证](docs/guides/testing-and-validation.md)。不要默认再打开测试参考。

- 测试不得与生产代码写在同一文件。Rust 生产侧只放 `#[cfg(test)] mod tests;`，实现放 `*/tests.rs`；前端用并列的 `*.test.ts`。
- 前端 Vitest 固定 mock backend；领域 reset 放 `src/dev/mocks`，不要往生产 façade 塞 `__reset*ForTests`。
- 日常改动只跑与风险匹配的过滤测试。全量 `pnpm test`、完整 Rust crate 矩阵和生产 `pnpm build` 留给提交前或 CI。

## 协作

局部改动不要升级成多 Agent 流程，也不要把提交前或 CI 的全量门禁搬进每一次本地改动。不要为了遵守本文件去通读 `docs/`，也不要列出含 `node_modules` / `target` 的仓库根目录。

| 风险级别 | 典型改动 | 默认执行方式 | 最小验证 |
|---|---|---|---|
| 局部 | 文案、样式、单页面状态、纯函数、单文件改动，且不改共享 contract | 主 Agent 在同一回合完成并跑定向测试 | 对应 Vitest；必要时 `pnpm typecheck` |
| 模块 | 单个功能目录内的逻辑，不改 Rust / wire / 持久化 | 主 Agent 或一个实现 Agent | 相关测试 + `pnpm typecheck` |
| 跨层 | backend port、wire DTO、Tauri command、共享 service、契约 JSON | 明确范围后再用实现与独立审查 | contract test + 对应 typecheck / Cargo filter |
| 高风险 | 数据迁移、写入补偿、锁、安全边界、发布 | 完整 planner / coder / reviewer / tester | 提交前矩阵和 CI 全量 |

- 只有至少两个边界清晰、文件范围不重叠的任务才并行多个 Agent。架构拍板、敏感操作和最终验收由主 Agent 负责。
- 写测试和跑测试拆开，只适合确实可以并行且启动成本划得来的任务。
- CodeGraph 1–2 次查询已给出调用链时，不要用 grep / 重读再证明一遍。
- 代码类子 Agent 必须写清任务、文件范围、限制和验收标准。并行写入时不得改同一文件。
- 审查从最终 diff、受影响调用方和对应验证开始，只在发现具体风险时再扩大读取范围。
- 只改完成任务所需的文件；不泄露密钥；未经授权不得删除或覆盖用户数据。
- 不修改工作区外的用户级配置。
