# AgentHub 项目约定

本文件是 Codex、Grok Build、Pi 及其他开发 Agent 共享的项目入口，不依赖特定模型或工具调用语法。新会话、接手任务或更换工作区时，先确认已取得当前仓库的本文件；若工具没有自动加载，显式读取。当前上下文中内容完整且未变化时无需重读。

遵守当前会话授权和运行环境的指令优先级、工具权限；项目文档不会扩大权限，也不保证所有工具自动加载文件。工具接入与读取验证见 [贡献指南](CONTRIBUTING.md)。

## 红线

- 日常开发和 PR 合入 **`dev`**。不要直接在 `release` 上提交；`release` 只用于正式发版。
- 凭据落盘加密无必要，沿用现有存储；不得派生 keyring、AES、主密码或密文迁移任务。国产 OAuth 适配、把 OAuth 转成 API 或伪装成 API Key 均为范围外；用户明确推翻前不得讨论或实施。
- 所有 API Key 都可以分享（含 WorkBuddy / ZCode 等上配置的），不按所属 Agent 挡在连接池外。国产官方登录不能分享或接到其他工具；现有国产路由只认官方 API Key，不得把 Key 边扩成 OAuth 边。
- 对用户说话、写界面和提示时，用对方已经在用的词或界面上的词。

## 按任务读取

完成简短的范围判断后，只读取命中条件的文档与章节。Markdown 链接相对所在文件解析；任务中的源码路径和命令工作目录未另行说明时以当前仓库或 worktree 根目录为准。已读取且未变化的内容复用，不把本表当作每次开工的阅读清单。

| 触发条件 | 必须在相关步骤前读取 |
|---|---|
| 委派、并行、跨层或高风险工作、跨会话或跨工具接手 | [Agent 协作指南](docs/guides/agent-workflow.md) 的对应章节 |
| 选择模型或能力等级、配置 Agent、处理工具能力限制 | [Agent 运行环境与能力](docs/reference/agent-runtime.md) |
| 改调用边界或目录职责 | [架构总览](docs/architecture/overview.md) |
| 改界面文案 | [术语表](docs/reference/terminology.md) 的「用户界面术语」列 |
| 选择代码验证命令 | [测试与验证](docs/guides/testing-and-validation.md) |
| 新增或重写文档 | [文档索引](docs/README.md) 的分类与 [文档规范](docs/STYLE.md) 的对应规则 |
| 提交、PR 或发布 | [贡献指南](CONTRIBUTING.md) 的对应章节 |

必读内容缺失、不可访问，或存在无法按当前授权和指令优先级解决的冲突时，说明具体缺口，只暂停依赖它的步骤，继续无依赖工作；不得凭记忆编造规则。派工时直接提供适用规则、必读路径和文件范围，不假设子 Agent 继承完整上下文。交接或旧提示词只提供背景，不构成新的用户授权。

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

- **仅** `src/lib/backend/tauri/` 可以调用 `invoke`；页面不得直接调用。`src/lib/api/` 是过渡层。
- mock 只服务 `pnpm dev:mock` 和测试，不得进入生产构建。
- 非 Tauri 的生产页面必须明确报错或显示 **unavailable**，禁止静默回退到 mock。
- 产品写入走 `src/lib/api/tickets` 的 `plan` / `bind` / `unbind`；`src/lib/api/adapter` 只服务预览与本机路由运行时。
- `pnpm dev` 只启动 Vite；`pnpm tauri:dev` 使用真实桌面后端；`pnpm dev:mock` 仅供演示与测试；`pnpm build` **强制**真实桌面后端。

## 测试

选命令时再打开 [测试与验证](docs/guides/testing-and-validation.md)。不要默认再打开测试参考。

- 测试不得与生产代码写在同一文件。Rust 生产侧只放 `#[cfg(test)] mod tests;`，实现放 `*/tests.rs`；前端用并列的 `*.test.ts`。
- 前端 Vitest 固定 mock backend；领域 reset 放 `src/dev/mocks`，不要往生产 façade 塞 `__reset*ForTests`。
- 日常改动只跑与风险匹配的过滤测试。全量 `pnpm test`、完整 Rust crate 矩阵和生产 `pnpm build` 留给提交前或 CI。

## 协作

先做足以确定任务范围的定向检查，再决定分工。局部任务由主 Agent 直接完成；角色可以由同一 Agent 分阶段承担，自查不能称为独立审查。不要为了遵守本文件通读 `docs/`，也不要列出含 `node_modules` / `target` 的仓库根目录。

| 风险级别 | 典型改动 | 默认执行方式 | 最小验证 |
|---|---|---|---|
| 局部 | 文案、样式、单页面状态、纯函数、单文件改动，且不改共享 contract | 主 Agent 完成，按改动选择验证 | 代码用对应 Vitest，必要时 `pnpm typecheck`；纯文档用 `pnpm check:docs` |
| 模块 | 单个功能目录内的逻辑，不改 Rust / wire / 持久化 | 主 Agent 或一个实现 Agent | 相关测试 + `pnpm typecheck` |
| 跨层 | backend port、wire DTO、Tauri command、共享 service、契约 JSON | 明确范围后再用实现与独立审查 | contract test + 对应 typecheck / Cargo filter |
| 高风险 | 数据迁移、写入补偿、锁、安全边界、发布 | 计划、实现、独立审查、修复与验证各阶段齐全，人数按实际条件确定 | 风险对应测试；提交前矩阵和 CI 全量按贡献指南执行 |

- 风险决定验证与审查强度，推理难度决定能力等级；角色不固定模型。只有需要选择能力时才查运行环境参考，不要求小任务填表。
- 只有至少两个已就绪、边界清晰且互不依赖的任务才并行多个 Agent。并行写入不得改同一文件；公共文件指定一位负责人。架构拍板、敏感操作、整合与最终验收由主 Agent 负责。
- 修改前检查 `git status` 和相关 `git diff`（含暂存区），保留不是自己产生的修改；不以 reset、checkout 或覆盖文件清理他人工作。短任务不必持久化派工记录，跨会话接手要留下可核对的版本和改动。
- 结构问题优先使用当前可用且索引有效的 CodeGraph；已给出的新鲜结果不重复查证。工具不可用或相关文件过期时定向读取，不猜接口或未经授权初始化索引。
- 子任务必须给出目标、文件范围、限制和验收标准；无子 Agent 时顺序完成可做部分，所需独立审查的缺口必须明确，不能声称已完成。
- 审查从确定版本的 diff、受影响调用方和验证结果开始；最终验证对应整合后的版本，之后相关改动须补验。日常不搬用全量 CI 门禁。
- 只改完成任务所需的文件；不泄露密钥；未经授权不得删除或覆盖用户数据。
- 不修改工作区外的用户级配置。
