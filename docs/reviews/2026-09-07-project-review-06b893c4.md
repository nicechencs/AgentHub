---
title: AgentHub 项目 review（2026-09-07，06b893c4）
type: status
status: current
owner: maintainers
updated: 2026-09-07
scope: dev；工作区文档变更及当前项目健康抽样；HEAD 06b893c4471b92e03246573bee935007bace8c27
---

# AgentHub 项目 review

本报告记录当前工作区的独立审查，不替代现行架构契约；未修改生产代码、测试或用户配置。后续覆盖与修复跟踪见 [c726bd2a 报告](./2026-09-07-project-review-c726bd2a.md) 和 [修复进展](./2026-09-07-review-fix-progress.md)。

## 1. 执行摘要

**Review verdict: BLOCK**。确认 **P0 0 项、P1 2 项、P2 1 项**。阻断的是本轮“运行时可长期使用”的验收，不是禁止合入本次文档入口变更。

两项重要问题：Kiro 旧会话 HTTP 请求没有落实本地取消；聊天进程输出达到保存上限后，即使继续输出，也可能被误判为空闲并终止。另发现一个明确的 Rust 测试分离约定违例。前端生产/mock 隔离、连接数据库事务及相关契约抽样表现良好。

主 Agent 已完成源码复核、97 项前端测试、60 项 Rust 定向测试、两项类型检查、生产构建及文档检查。现有测试通过不推翻两个未被组合场景测试覆盖的运行时缺陷。没有启动真实 Agent、登录外部服务或发送付费请求；两个 P1 是源码确认，不冒称完整本机复现。

### 审查对象与权限

- 本机日期：2026-09-07；Windows x64；仓库 `D:/demo_chen/2026/AgentHub`。
- 分支：`dev`；HEAD：`06b893c4471b92e03246573bee935007bace8c27`。
- 比较基线：本机 `origin/dev` 与 HEAD 相同；未 fetch，不能证明线上远程最新状态。
- 初始工作区：`docs/README.md` 修改 1 行；未跟踪 `docs/guides/project-review-handoff.md`；暂存区为空。用户改动仅增加审查提示词及入口，主 Agent 已查看该 diff 和提示词全文。
- 因工作区没有生产代码 diff，先核实文档变更，再按全项目健康检查抽样关键边界与近期提交。不是逐行穷尽全仓。
- 最终工作区仅比初始状态多出本报告；HEAD、分支、暂存区未变。唯一工作树仍为原仓库，没有分配临时 Git worktree。
- 未提交、推送、合并、切分支、发版、删除数据或修改用户级配置。标准构建与测试缓存属于允许的验证产物。

## 2. 范围与覆盖

| 深度 | 范围 | 方法及依据 |
| --- | --- | --- |
| 深入 | 前端 backend 选择、Tauri 不可用、生产/mock 隔离 | 构建配置、invoke 包装、边界扫描测试、生产构建 |
| 深入 | 连接创建/激活、当前项与存储同步、失败回滚 | reviewer 读取服务及仓储；主 Agent 回读事务入口，运行 connection_service 54 项测试 |
| 深入 | Kiro 旧会话发送、取消、HTTP 恢复出口 | UI 取消到服务的调用链、Early 结果调度、最终消息状态；两模型意见与主 Agent 裁决 |
| 深入 | Pi/通用聊天空闲期限、输出截断、进程终止 | 管道读取、消息队列、活动时钟、超时出口及已有测试；核对近期实际 diff |
| 抽样 | Windows npm/PowerShell 启动与进程清理 | scout 分段定位，大模型读取相关函数，4 项重写测试和 Windows 空闲超时测试 |
| 抽样 | 产品连接 API、Tauri command/DTO、mock 对照 | tickets 调用与端口、契约测试；不是所有 command 穷举验证 |
| 抽样 | 日志、会话恢复、测试组织、文档与近期提交 | 仅限已检查路径，不把旧报告或未来提案当作当前事实 |
| 未检查 | 其余页面/交互、完整桥协议转换、全部迁移、文件补偿与锁组合、CLI 全部命令 | 超出本轮深入样本；已具备阻断证据后不继续扩大搜索 |
| 未实测 | macOS/Linux、真实 Kiro/Pi/Cursor、桌面打包安装更新、长时间负载与后代 PID 清理 | 缺少安全沙箱或对应平台；不访问真实账号和用户配置 |

近期线索来自 `git log -15 --oneline --decorate`：`06b893c4` Windows npm/Cursor、`7051eb2c` Pi busy turn、`12b35a2c` Kiro 路由参数、`2adf7b6b` HTTP 恢复、`32553ed7` 旧会话发送。主 Agent 对 `7051eb2c` 的 process/limits diff 做直接核对，对 `32553ed7` 查看统计；其他近期提交只作为定位线索，不声称都审过完整 diff。scout 的近期摘要与 Git 原始列表不完全一致，提交归因只采用主 Agent 的 Git 证据。

## 3. 确认问题

### AHREV-P1-001：Kiro 旧会话 HTTP 发送忽略取消，取消后仍可能报告成功

- **修复状态**：已关闭（`d1edd6f4`，合入 dest）。

- **分类/置信度**：confirmed；P1；高（源码可达路径），真实上游时序未复现。
- **位置**：`src/pages/chat/use-chat-page-send.ts` 的 `cancelRuntimeTarget`（约 740–777）；`crates/agenthub-core/src/services/chat_service.rs` 的 `cancel`（225–234）、取消登记（424–438）、调用运行服务（560）、释放占用（596–603）和最终取消状态（688）；`services/run_service.rs` 的 `resolve_job`（301–309）、`run_each_parallel`（512 起）；`adapters/kiro/http/client.rs` 的 `try_http_run_result`（571–637）。Rust 路径均位于 `crates/agenthub-core/src/`。
- **触发**：仍使用旧发送入口的 Kiro 会话，HTTP 请求已开始且仍在等待；用户点击停止；请求稍后返回成功。`src/pages/chat/chat-grok-follow-up.ts` 对已有消息的 Kiro 返回 `newChat`，不提供会丢原会话的续接（源码称 lossy upgrade）；会话仍可走旧发送入口。
- **实际**：UI 旧入口调用 `chatCancel`；服务只设置 token。`resolve_job` 同步请求 HTTP，未把 token 传给 HTTP。`run_each_parallel` 直接发送 Early 成功结果，未检查取消；最终消息仍可成功，`cancelled` 仅由结果状态得出，因而为 false。发送占用要等 HTTP 返回才释放。
- **预期/影响**：无法撤销远端已经开始的计算，但本地应响应停止、释放等待并正确报告取消，不应整个请求期间完全忽略取消。否则用户不能及时停止旧 Kiro 会话，且停止后仍收到正常完成。
- **证据**：源码确认。HTTP 还使用 `http/creds.rs:451` 固定 120 秒请求期限，不使用 `RunOptions` 的期限；这说明两条运行路径策略不同，不另计一个缺陷。刷新等多次请求使固定值不等同于整次发送上限。
- **历史**：`git blame` 将 Kiro 同步 HTTP 分支指向 `73e008d96`；当前 HEAD 已存在，不归因于本次文档 diff。未做完整首次引入追溯。
- **反方裁决**：接受 `cancel()` 注释中的 best-effort，取消不保证撤回远端请求；不接受“Early 接口没有 token，所以无需修复”。实现接口本身不能证明用户停止请求可以在整个 HTTP 等待期间被忽略。这里不是请求已完成后恰好点停止的竞态。
- **最小修复方向**：让 HTTP 执行阶段接收取消与期限，采用可中断的本地等待；统一取消后的结果出口，保留已存在的 HTTP 会话标识。不要只在请求开始前检查一次 token。
- **推荐验证**：可控 HTTP transport + 临时数据库，覆盖请求前取消、等待中取消、取消后成功返回和短期限；断言状态、消息、发送占用及 HTTP 会话标识。不得用真实登录或付费请求。

### AHREV-P1-002：输出保存达到上限后，仍在输出的聊天进程被误判为空闲

- **修复状态**：已关闭（`a1ad547c`，合入 dest）。

- **分类/置信度**：confirmed；P1；高。跨平台共享源码确认，未运行完整 2 MiB/10 分钟复现。
- **位置**：`crates/agenthub-core/src/utils/process.rs` 的 `run_spec_streaming`（966–993、1117 起）及 `read_pipe_capped`（1324–1370）；`services/chat_service.rs:490–499`；`catalog/limits.rs:13–20`。
- **触发**：聊天进程 stdout 超过 2 MiB 保存上限，stderr 没有后续活动，输出队列已排空；进程随后继续工作并输出超过 10 分钟，尚未到 2 小时绝对期限。
- **实际**：`read_pipe_capped` 达到上限后继续读取，但 `accepted == 0` 直接 continue，不再发出活动消息；主循环仅在 `rx.try_recv()` 收到文本时刷新 `last_activity`。因此持续读到的数据不能刷新计时，最终触发 `without output` 并终止进程树。
- **预期/影响**：保存和展示上限不能充当真实 I/O 活动上限。否则长时间、高输出的编码任务被错误中断；不是单纯日志截断或测试缺口。
- **证据**：源码确认 + 现有测试佐证。`read_lines_capped_stops_emitting_after_max` 实际通过，证明截断后停止通知；Windows 空闲超时测试通过。二者没有组合成“截断后继续输出”的回归测试，故不能称已完整复现。
- **历史**：主 Agent 读取 `git show 7051eb2c` 和 `git blame`，确认活动时钟及空闲超时分支由该提交新增；与原有截断逻辑组合后形成当前缺陷。不是 `06b893c4` 的 Windows batch 修复所引入。
- **反方裁决**：不接受将生产故障降为“只需补测试”；反方没有发现截断后另有活动计数来源。测试建议归入同一根因，不另列 P2。
- **最小修复方向**：在成功读取字节时更新独立、线程安全的活动计数/时间，由主循环观察；保留输出上限与绝对期限，不用扩大内存保存来规避问题。
- **推荐验证**：本地辅助进程，小输出上限与短空闲阈值，持续输出超过上限仍应正常完成；之后真正停止输出才应超时。Windows 与 Unix 都应覆盖。

### AHREV-P2-001：Chat 运行时生产文件内仍包含测试实现

- **分类/置信度**：confirmed；P2；高，主 Agent 在补核 Kiro 入口时直接发现并复核。
- **位置**：`crates/agenthub-core/src/services/chat_runtime/ops.rs:751–1159`，`#[cfg(test)] mod tests { ... }`；例如 `acp_session_plan_reuses_live_kiro_and_skips_cross_process_load`（1100 起）。
- **触发/实际/预期**：读取当前源码即可确认；生产文件内嵌大量测试实现。`AGENTS.md` 明确要求生产侧只声明测试模块，测试实现放独立文件。
- **影响/证据**：工程契约冲突，增加生产模块阅读负担；不是发布二进制包含测试代码，也不据此认定运行时故障。未穷举同类位置。
- **最小修复方向**：仅把此测试模块迁到相邻 `ops/tests.rs`，保留 `#[cfg(test)] mod tests;`，不顺带重构。
- **推荐验证**：迁移后运行对应 `chat_runtime::ops::tests` Cargo filter，并确认没有丢失平台编译条件和测试数量。

## 4. 候选裁决与误报

| 编号/线索 | 裁决 | 理由 |
| --- | --- | --- |
| AHREV-P1-001 | confirmed | 从阶段“待核实”升级；本地取消没有进入 HTTP 等待及 Early 成功出口 |
| AHREV-P1-002 | confirmed | 主 Agent 回读管道/计时并核对实际提交，反方未推翻活动断链 |
| AHREV-P2-001 | confirmed | 当前文件与明确工程规则直接冲突 |
| CH-001：Early 优化允许忽略停止 | invalid | 接口没有参数不是用户行为正确的证明；best-effort 不等同于完全不观察取消 |
| CH-002：截断后活动只属测试缺口 | duplicate / invalid 降级 | 测试缺口归入 P1-002，不另计；降级没有源码反证 |
| CH-003：两个 reviewer verdict 不同是 P1 | invalid | 两者切片本来不同，边界 reviewer 未声称全仓无缺陷；不是产品 Bug |
| CH-004：review verdict 协调另列 P2 | duplicate | 由主 Agent 综合裁决，无需作为代码问题 |
| Windows 换行/引号参数真实往返 | needs-evidence | 向量重写测试通过不等同真实 CLI 启动正确；没有确认新的参数 Bug |
| Windows 后代 PID 泄漏 | needs-evidence | 有 Job Object 与清理逻辑，现有返回时限测试不足以证明所有后代消失，也不足以确认泄漏 |
| Kiro HTTP 恢复完整上下文 | needs-evidence | 标识保留可静态核实，上游会话语义未用真实账号验证 |
| 生产 bundle 体积警告 | needs-evidence | 构建通过，缺少实测启动/内存影响，不把警告直接升级为缺陷 |
| 加密存储、国产 OAuth、OAuth 转 API、按 Agent 限制 API Key 分享 | out-of-scope | 明确产品范围，不作为待办 |

本轮没有依赖旧 review 问题，因此无需要冒认 stale/已修复的旧条目；不以旧报告替代当前证据。

## 5. 架构评估

- **前端/backend**：`vite.config.ts:105–108` 在构建时选实现；页面、runtime、端口职责清楚。`src/lib/api/tickets.ts` 的产品写入经过 backend，兼容 API 的存在本身不是重复业务规则证据。
- **Tauri**：`src/lib/backend/tauri/invoke.ts` 明确拒绝非桌面环境，不静默回退。生产构建 guard 和边界测试构成两道不同检查。只覆盖抽样 command，不能外推所有 Tauri/CLI 壳都没有业务复制。
- **Rust core/存储**：连接激活在同一 IMMEDIATE 事务内改当前项与连接引用；失败注入、版本冲突和扩展字段保留测试有实际价值。未验证全部跨文件写入补偿与迁移组合。
- **进程/会话**：统一进程执行有取消、Job Object、有限等待和期限机制，但截断与活动检测耦合错误。旧 Kiro HTTP 在任务解析阶段执行外部 I/O，绕过统一执行控制，是需要局部修正的职责问题，不要求全局重构。
- **连接/路由**：产品写入端口与连接数据库事务抽样通过；协议转换和本机路由全部失败组合未覆盖，不给整体无风险结论。
- **mock/生产**：本机生产构建通过，未触发禁止模块检查；Vitest 和不可用分支测试通过。架构文档把 sidecar、跨进程 IPC、saga 标为未来方向，本次不当作缺失实现。

## 6. 成熟度评估

不使用无依据的整体分数；以下评级仅适用于覆盖样本。

| 维度 | 判断 | 证据与限制 |
| --- | --- | --- |
| 架构 | 基础较稳 | 构建时实现选择、唯一 invoke 包装、服务事务；旧 HTTP 执行边界需修正 |
| 正确性 | 需修复后验收 | 两项可达 P1，不能凭测试全绿批准长任务可靠性 |
| 错误处理 | 部分成熟 | 非桌面明确不可用；HTTP 取消状态存在遗漏 |
| 数据一致性 | 抽样良好 | 54 项连接事务/恢复相关测试通过；不覆盖全部文件补偿 |
| 跨平台 | 验证不足 | Windows 定向测试通过；macOS/Linux 未运行，真实 CLI 参数未验证 |
| 测试 | 分层有效但有关键组合缺口 | 97 项前端、60 项 Rust 通过；输出上限与空闲时钟没有组合测试 |
| 维护 | 需局部整理 | 主体责任清晰；ops.rs 内嵌测试违反规则，不建议大规模重构 |
| 日志 | 样本可诊断 | 不可用和超时有明确错误；未完整审计秘密脱敏，不宣称全面安全认证 |
| 恢复能力 | 部分成熟 | Kiro HTTP 标识保留逻辑存在，真实上下文恢复及后代进程清理待测 |
| 发布准备度 | 暂不通过运行时验收 | 前端可构建，不等同桌面安装/升级已验收；两个 P1 未修复 |

## 7. 测试与本机验证

环境：Node `v24.19.0`、pnpm `9.4.0`、rustc `1.89.0`、cargo `1.89.0`；`node -p "process.platform + ' ' + process.arch"` 返回 `win32 x64`。未启动 Tauri/Agent CLI 查询版本，避免用户配置或外部副作用。

### 命令与结果

| 完整命令 | 退出码 | 关键结果 |
| --- | --- | --- |
| `git status --short --branch` | 0 | 开始及收尾均为 dev；保留用户两项文档状态，仅新增本报告 |
| `git rev-parse HEAD` | 0 | 06b893c4471b92e03246573bee935007bace8c27 |
| `git branch --show-current` | 0 | dev |
| `git log -15 --oneline --decorate` | 0 | 获取近期历史，不做网络更新 |
| `git diff --stat` / `git diff --name-status` | 0 / 0 | 用户 README 增加 1 行；未跟踪文件另由 status 标识 |
| `git diff --cached --stat` / `git diff --cached --name-status` | 0 / 0 | 暂存区为空 |
| `git rev-parse origin/dev` | 0 | 与 HEAD 一致，仅本机引用 |
| `git diff -- docs/README.md` | 0 | 新增 review guide 入口 |
| `git worktree list` | 0 | 仅原仓库 dev |
| `git show 7051eb2c --format=short -- crates/agenthub-core/src/utils/process.rs crates/agenthub-core/src/catalog/limits.rs` | 0 | 验证空闲计时新增与设计注释 |
| `git show --stat --oneline 32553ed7` | 0 | 旧 Kiro 会话相关变更列表 |
| `git blame -L 298,307 -- crates/agenthub-core/src/services/run_service.rs` | 0 | Kiro HTTP 分支归因线索 |
| `git blame -L 966,993 -- crates/agenthub-core/src/utils/process.rs` | 0 | 活动时钟与空闲超时来自 7051eb2c |
| `pnpm typecheck` | 0 | 生产 TypeScript 通过 |
| `pnpm typecheck:test` | 0 | 测试 TypeScript 通过 |
| `pnpm test:contracts` | 0 | 4 文件，61 项通过 |
| `pnpm exec vitest run src/lib/backend/tauri/boundary.test.ts src/lib/backend/tauri/ticket.test.ts src/lib/backend/contracts/ticket.test.ts` | 0 | 3 文件，36 项通过 |
| `pnpm build` | 0 | 3689 modules，生产边界检查通过 |
| `cargo test -p agenthub-core --locked rewrite_windows_batch_run_spec` | 0 | 2 项通过 |
| `cargo test -p agenthub-core --locked spawn_npm_cmd_via_node` | 0 | 2 项通过 |
| `cargo test -p agenthub-core --locked connection_service` | 0 | 54 项通过 |
| `cargo test -p agenthub-core --locked read_lines_capped_stops_emitting_after_max` | 0 | 1 项通过；只佐证截断通知行为 |
| `cargo test -p agenthub-core --locked streaming_idle_timeout` | 0 | Windows 1 项通过；非 Windows 刷新活动用例未编译运行 |
| `pnpm check:docs` | 0 | Documentation checks passed (82 Markdown files)；最终文本写完后再次检查 |

生产 build 因本轮确实检查生产/mock 边界而执行；没有跑无关全量矩阵。

### 警告、失败分类与未运行项

- 测试无失败。boundary 测试中的 `BackendUnavailableError` stderr 是预期断言路径，不是测试故障。
- build 有非阻断警告：runtime 同时静态/动态 import；主 JS 3586.59 kB 超过 3200 kB 警戒值。没有调大阈值掩盖警告。
- 父级机械定位曾读错临时输出路径（ENOENT），通过运行记录找到实际 artifact；另有猜测测试/模块路径的 `rg` 报文件不存在，改按受控 tracked 文件清单定位。这些不是生产失败或 subagent 启动失败；没有执行模式回退。组合 shell 的末尾退出码不能证明每项搜索成功。
- 未运行全量 `pnpm test`、`pnpm test:pr`、完整 Cargo workspace、桌面打包/发布、真实账号请求。当前证据已足够形成结论；新建失败用例属于修复后验证建议，本轮只允许写报告，未修改测试。

## 8. 多模型与 Mission 记录

Goal Mission：`653f4321-ab14-420e-bc9b-93e666caab00`；声明预算 180000 tokens。单个顶层 async workflow：`4258a122-c15f-43ec-ad7b-a4c7c24df152`；4 个子任务全部 completed，没有嵌套 subagent，没有并发写最终报告。

| Agent / 实际指定模型 | 层级与任务 | run |
| --- | --- | --- |
| scout / `openai-codex/gpt-5.4-mini:low` | 低成本机械扫描、大文件符号/行范围、测试定位；不确认重大问题 | ccaa5fcb-5612-467e-bf12-a611d9d22bd6 |
| reviewer / `openai-codex/gpt-5.6-terra:high` | 强模型：生产边界、连接存储事务、契约 | 64b9e71e-8243-4ba3-a0af-c1f9f5cab300 |
| reviewer / `openai-codex/gpt-6-astra:high` | 强模型：复杂进程/会话/取消/超时，重大候选复核 | 979e28b2-d3fc-488d-8aad-ce8228a970e5 |
| reviewer / `xai/grok-4.3` | 不同系列的反方：只挑战重大候选，不重复全仓扫描 | fcdec52b-fcc3-425c-947b-376d11b14698 |

执行前调用 capabilities list 与 models，使用实际注册标识；未调用不可执行的 codex-exec（runner 缺失），未启动外部 CLI 替代。四个 native 子任务均成功，没有不可用模型或运行器失败。

强模型升级理由是跨层取消和线程/期限控制，不是目录扫描。两条强 review 按边界拆分；传递索引及候选而非全文件，减少大文件重复读取。主 Agent 仅回读关键争议函数及证据。scout 状态约 61k tokens / 52 tools，仍偏多；不能声称精确节约比例。Mission 面板在结束 workflow 后仍显示 0/180000，明显不能作为实际消耗为零的依据，本报告不伪造费用或总 token。

反方承认调用链事实，但认为 best-effort/Early 是设计取舍，并把截断问题侧重描述为测试缺口。主 Agent 根据用户影响和源码裁决保留两项 P1；不同切片 reviewer 的局部 verdict 不构成互相否定。P2 测试分离问题由主 Agent 直接确认，未为简单规则检查额外启动强模型。

审查输出位于本机会话 artifact 的 `review/map.md`、`boundaries.md`、`runtime.md`、`challenge.md`。可由 workflow receipt 恢复：`C:/Users/chen/AppData/Local/Temp/pi-subagents-user-chen/async-subagent-runs/4258a122-c15f-43ec-ad7b-a4c7c24df152/workflow-receipt.json`。最终可交付事实已汇入本报告，不依赖临时输出长期保留。

## 9. 待验证与残余风险

- 源码确认的两个 P1 尚未修复；真实网络取消及完整长输出组合未本机复现。
- Windows 真实 npm/Cursor 启动、Pi 长任务、Kiro 上游完整上下文恢复仍需隔离环境验证。
- 连接数据库测试不能替代全部迁移、文件备份/恢复与并发补偿验证。
- 没有全仓秘密泄露、协议、安全或资源耗尽审计；不能将本轮日志样本推广为整体安全结论。
- 不把尚未检查的目录记成通过；没有发布或线上验证证据。

## 10. 建议顺序

1. **立即修复**：P1-002 的截断/活动耦合；P1-001 的 HTTP 本地取消与结果状态。先写无账号的失败用例，再做最小修复。
2. **下一轮**：补 Windows/Unix 组合测试及隔离 CLI 参数验证；迁移 P2-001 内嵌测试。
3. **可选优化**：测量 bundle 对启动和内存的实际影响后再考虑拆包；不要直接扩大阈值。
4. **不建议处理**：范围外产品事项、无证据的大规模重构、为统一 reviewer 意见而改产品、仅根据未来提案增建 sidecar。

## 11. 最终结论

**Review verdict: BLOCK**。

当前基础架构和已验证样本可继续开发，但不能批准“聊天长任务及旧 Kiro 停止行为可靠”的整体验收。报告任务完成不等于缺陷已修复；修复需另获用户授权，本轮不自动动手。

## 12. 文档检查

初稿及最终报告均通过 `pnpm check:docs`：退出 0，`Documentation checks passed (82 Markdown files)`。`git diff --check` 无错误。没有为通过检查修改无关文档；收尾确认 HEAD 和暂存区未变，工作区仅新增本报告。
