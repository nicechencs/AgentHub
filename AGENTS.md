# AgentHub 项目约定

本文件是仓库内的工程红线和协作规则。用户入口见 [README.md](README.md)，贡献流程见 [CONTRIBUTING.md](CONTRIBUTING.md)，文档规则见 [docs/STYLE.md](docs/STYLE.md)。

## 红线速览

- 分支：日常开发和 PR 使用 **`dev`**；`release` 仅用于正式发版。
- 范围外：凭据落盘加密、国产 OAuth 适配或 OAuth 转 API 不得列为待办、风险或实施任务。

## 分支与发布

- 日常开发与 PR 合入 `dev`，不要把 `main` 或 `release` 当作日常集成线。
- 正式发版时，同时修改 `package.json`、`Cargo.toml` 的 `[workspace.package]` 和 `src-tauri/tauri.conf.json` 三处版本号。
- 更新 `release` 分支后，创建并推送匹配的 `vX.Y.Z` tag；tag 必须指向 `release` 上的对应提交且不可覆盖已有版本。
- GitHub Actions 只在推送 `v*` tag 时出包。`dev` 与 `release` 是无关历史，不要把 `dev` 合并进 `release`。
- Agent 隐藏以 `dev` 的 store-stamp 为准，只影响界面，不是旧 release 线的软隐藏。
- 当前界面使用「登录」，不使用「票」。实现里的 Ticket / TicketPort 仍是内部名称。

## 前端 backend 分层与 Adapter

完整说明见 [架构总览](docs/architecture/overview.md)。目标结构如下，目录以源码为准：

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

### 命令与 Adapter

| 命令 | Adapter |
|---|---|
| `pnpm dev` | 普通 Vite 前端开发服务器，不启动 Tauri |
| `pnpm tauri:dev` | Tauri adapter，启动真实桌面后端 |
| `pnpm dev:mock` | browser mock adapter，仅用于演示与测试 |
| `pnpm build` | **强制** Tauri adapter |

### 硬约束

- **仅** `src/lib/backend/tauri/` 可以调用 `invoke`；页面层不得直接调用 `invoke`，`src/lib/api/` 是过渡兼容层。
- mock 只服务 `pnpm dev:mock` 和测试，不得进入生产构建。
- 非 Tauri 的生产页面必须明确报错或显示 **unavailable**，禁止静默回退到 mock。
- 产品写入走 `src/lib/api/tickets` 的 `plan` / `bind` / `unbind`；`src/lib/api/adapter` 只服务预览与本机路由运行时。

## 凭据存储决策

- 决定：没有必要做凭据落盘加密，沿用现有存储方案。
- 讨论进展、整理待办或生成提示词时，将凭据加密标为「无必要 / 项目范围外」，不得据此推导 keyring、AES、主密码或密文迁移任务。
- 只有用户明确推翻此决策并重新授权后，才可以讨论凭据落盘加密。

## 国产 OAuth：不开边、不转 API

- 产品关闭：不为中国产 AI 的 OAuth 开 Adapter 边，也不把 OAuth 转成 API 或伪装成 API Key。现有国产路由只认官方 API Key 登录。
- 不得把现有 Key 边扩成 OAuth 边。分析进展、整理待办时，将国产 OAuth 适配 / 转 API 标为「产品不做 / 项目范围外」。
- 只有用户明确推翻此决策并重新授权后，才可以讨论国产 OAuth 开边。

## 测试约定

完整说明见 [测试参考](docs/reference/testing.md) 与 [测试与验证](docs/guides/testing-and-validation.md)。

- 测试代码不得与生产代码写在同一文件。Rust 生产侧只放 `#[cfg(test)] mod tests;`，实现放 `*/tests.rs`；前端使用并列的 `*.test.ts`。
- 前端 Vitest 固定 mock backend；领域 reset 放 `src/dev/mocks`，不要往生产 façade 塞 `__reset*ForTests`。
- 日常改动运行与风险匹配的过滤测试。全量 `pnpm test`、完整 Rust crate 矩阵和生产 `pnpm build` 默认留给提交前或 CI，不作为每次本地改动的默认门禁。

## 协作规则

按风险分级选择 Agent 流程和验证范围。局部改动不要升级成完整多 Agent 流程；也不要把提交前或 CI 的全量门禁搬进每一次本地改动。

| 风险级别 | 典型改动 | 默认执行方式 | 最小验证 |
|---|---|---|---|
| 局部 | 文案、样式、单页面状态、纯函数、单文件改动，且不改共享 contract | 主 Agent 在同一回合完成实现并运行定向测试 | 对应 Vitest；必要时 `pnpm typecheck` |
| 模块 | 单个功能目录内的逻辑，不改 Rust / wire / 持久化 | 主 Agent 或一个实现 Agent | 相关测试 + `pnpm typecheck` |
| 跨层 | backend port、wire DTO、Tauri command、共享 service、契约 JSON | 明确范围后再用实现与独立审查 | contract test + 对应 typecheck / Cargo filter |
| 高风险 | 数据迁移、写入补偿、锁、安全边界、发布 | 完整 planner / coder / reviewer / tester 流程 | 提交前矩阵和 CI 全量 |

- 每次任务开始先判断能否拆成多个独立子任务；只有至少两个边界清晰、文件范围不重叠的任务才并行多个 Agent。架构拍板、敏感操作和最终验收由主 Agent 负责。
- 局部 UI、文案、纯函数和单文件改动由主 Agent 直接实现并运行定向测试。
- 只有跨层 contract、Rust 核心规则、持久化、安全和发布变更才启用完整 planner / coder / reviewer / tester 流程。
- 写测试和跑测试拆成两个 Agent，只适合确实可以并行、文件集不重叠且等待时间足以覆盖 Agent 启动成本的任务。
- 机械任务（已确定的测试命令、typecheck、构建、日志汇总）在跨层或高风险改动中交给测试或执行 subagent；测试 Agent 只执行给定命令并回报原始失败信息，不重新探索需求与架构。
- reviewer 从最终 diff、受影响调用方和对应验证开始，仅在发现具体风险时扩大读取范围。
- CodeGraph 已在 1–2 次查询内给出调用链和影响面时，不再用 grep / 重读重复证明同一结构事实。
- 代码类 subagent 必须获得明确的任务、文件范围、限制和验收标准。并行写入时不得让两个 Agent 修改同一文件。
- 只修改完成任务所需的文件，保留用户已有改动；不泄露密钥、令牌或其他敏感信息；未经授权不得删除、重置或覆盖用户数据。
- 不修改工作区外的用户级配置。
