---
title: AgentHub 项目 review（2026-09-07，c726bd2a）
type: status
status: current
owner: maintainers
updated: 2026-09-07
scope: dev；跨 HEAD 续审，最终目标 c726bd2aa1ff8ea0aaf206c6755f6cdcfbb6303c
---

# AgentHub 项目 review

本报告记录当前工作区的持续独立审查，不替代现行架构契约；本轮未修改生产代码、测试或用户配置。

## 执行摘要

**Review verdict: BLOCK**。

最终确认 **P0 0 项、P1 9 项、P2 5 项**。P1 集中在停止与进程生命周期、备份/回收一致性、本机路由的重放与登录刷新、SSE 结束处理。前端/backend 分层、生产/mock 隔离、连接激活事务、迁移事务及发版元数据抽样表现良好。

第一轮开始于 `06b893c4`。审查期间 `dev` 前进到 `c726bd2a`（`v0.4.10`）；新增提交只修改 `CHANGELOG.md`、`Cargo.lock`、`Cargo.toml`、`package.json`。第二、三轮以新 HEAD 复核源码并扩大覆盖。旧报告 `docs/reviews/2026-09-07-project-review-06b893c4.md` 保留，不覆盖。

## 范围与运行信息

- 仓库：`D:/demo_chen/2026/AgentHub`；Windows x64。
- 最终分支/HEAD：`dev` / `c726bd2aa1ff8ea0aaf206c6755f6cdcfbb6303c`。
- 本机 `origin/dev` 与 HEAD 一致；未 fetch，远程引用可能陈旧。
- `v0.4.10^{}` 指向 HEAD；该提交在 `dev` 与本机 `origin/release` 可达，本机 `release` 分支较旧。
- 工作区原有 `docs/README.md` 和 review guide；旧 HEAD 报告已存在。本轮只新增本报告，并修正旧报告中三处文字/行号后停止改旧报告。
- 未提交、推送、合并、切换/重置分支、发版、访问真实账号或启动真实 Agent。

### 覆盖

| 深度 | 范围 |
| --- | --- |
| 深入 | 前端/backend/Tauri/mock 边界；连接激活与事务 |
| 深入 | Chat 旧发送、持续运行、取消、删除、事件轮询、会话排序 |
| 深入 | 备份恢复、路由成员回收、路由所属登录恢复、迁移事务 |
| 深入 | 本机路由 HTTP/SSE、换成员、登录刷新、排空与停止 |
| 抽样 | Windows CLI 启动、安装/卸载、Tauri command/DTO、日志脱敏 |
| 抽样 | v0.4.10 版本、锁文件、CHANGELOG、tag 与 release 元数据 |
| 未实测 | macOS/Linux、真实 Kiro/Pi/Cursor、真实网络计费、安装升级、长时间负载 |
| 未穷尽 | 所有页面、全部协议排列、全部迁移/文件补偿组合、全仓日志安全 |

停止扩大搜索的原因：已有足够证据形成 BLOCK；继续搜索开始重复同类问题。全项目 review 不声称逐行穷尽。

## 确认问题

### AHREV-P1-001：Kiro 旧会话 HTTP 发送不响应停止

- **位置**：`crates/agenthub-core/src/services/run_service.rs:301-309,512-536`；`adapters/kiro/http/client.rs:571-637`；`services/chat_service.rs:225-234,424-438,560,596-603,688`。
- **触发/行为**：旧 Kiro 会话的 HTTP 请求等待期间点击停止。取消 token 没有传入同步 HTTP；Early 成功结果不检查取消，最终仍可能保存成功消息并报告 `cancelled=false`。
- **影响/证据**：停止失效直到固定 HTTP 期限返回；源码调用链确认，未访问真实上游。**P1，高置信度**。
- **最小修复/验证**：HTTP 执行接收取消和期限，统一取消出口；用可控 transport 覆盖等待中取消、迟到成功和会话标识保留。

### AHREV-P1-002：输出截断后仍输出的进程会被误判为空闲

- **位置**：`crates/agenthub-core/src/utils/process.rs:966-993,1117,1324-1370`；`services/chat_service.rs:490-499`；`catalog/limits.rs:13-20`。
- **触发/行为**：stdout 达到 2 MiB 后继续输出超过 10 分钟。管道继续读取但不再发送活动消息，`last_activity` 不刷新，进程被 `without output` 超时终止。
- **影响/证据**：长时间高输出任务被错误中断；共享源码和现有截断测试确认。**P1，高置信度**。
- **最小修复/验证**：读取字节独立刷新活动时间；用小上限/短期限辅助进程组合测试 Windows 与 Unix。

### AHREV-P1-003：备份未完整恢复仍向界面报告成功

- **位置**：`services/backup_service/restore.rs:347-387`；`backup_service/mod.rs:65-78`；`src/lib/backend/contracts/backup-port.ts:7`；`src/lib/backend/tauri/backup.ts:33-35`；`src/pages/backups/BackupsPanel.tsx:186-196`。
- **触发/行为**：恢复需删除 AgentHub 管理的缺席文件，但文件只读、被占用或权限不足。`delete_failed` 只进入被 `serde(skip)` 的内部列表，前端端口返回 `void`，页面无条件提示成功。
- **影响/证据**：遗留配置仍可影响登录或配置选择，用户误以为完整恢复。源码确认；`edited/unknown` 安全保留不属于本问题。**P1，高置信度**。
- **最小修复/验证**：公开部分恢复结果或将 `delete_failed` 升为错误；注入删除失败并验证 Tauri/页面警告。

### AHREV-P1-004：路由所属登录恢复失败会消耗唯一恢复入口

- **位置**：`crates/agenthub-core/src/lib.rs:146-177`；`services/connection_service/trash.rs:212-246`；`services/route_pool_service.rs:540-580,1109-1119,1988-1993`；`services/ticket_read_service.rs:60-82`。
- **触发/行为**：恢复 `home=route_pool` 的登录时路由功能关闭，或重新加入路由失败。源行和回收记录先在事务中恢复/删除，之后才重新加入路由；失败后记录已消失，源行又被 Connections 过滤且不在路由成员中。
- **影响/证据**：登录信息留在数据库但两个主要界面不可见，再次恢复会因源行已存在失败。源码及真实 UI 路径确认。**P1，高置信度**。
- **最小修复/验证**：最后再删除回收记录，或失败时完整补偿；关闭功能/注入 attach 失败后断言可重试。

### AHREV-P1-005：SSE 正常结束受网络分块影响而误报失败

- **位置**：`crates/agenthub-core/src/bridge/host/stream.rs:1534-1546,1729-1741`；`bridge/protocol/responses/mod.rs:993-1003,1055-1071`。
- **触发/行为**：`response.completed` 后的合法注释、空帧或尾部字节与终止帧位于同一网络块。转换循环见结束事件即退出，但剩余 buffer 又被判为损坏；分到下一块则成功。
- **影响/证据**：Messages 可能先结束再报错，Chat 跳过正常 finish，路由记录成功请求为失败。源码与直通路径对照确认。**P1，高置信度**。
- **最小修复/验证**：终止后按合法 SSE 语义清理/忽略尾部，保留无终止帧检查；同块/跨块 fixture 覆盖 Messages 与 Chat。

### AHREV-P1-006：响应头前断连会换成员重放不明状态请求

- **位置**：`bridge/host/upstream.rs:211-217,291-319`；`bridge/host/transport/failover.rs:257-270,330-375,575-631`。
- **触发/行为**：无续接锁定且至少两个成员；成员 A 已读完 POST、响应头前断连。所有 `builder.send()` 错误都变为“可安全换成员”的 `Unavailable`，同一生成请求再发给 B；Chat/Responses 没有跨成员幂等键。
- **影响/证据**：可能重复生成和计费；不声称工具一定重复执行。代码自身对 header timeout 的“可能已计费、禁止重放”契约构成直接反证。**P1，高置信度**。
- **最小修复/验证**：仅明确连接未建立的错误允许换成员；本地双上游模拟 A 接收后断连，断言 B 不收到请求。

### AHREV-P1-007：跨池晚到的旧 token 401 会隔离已更新登录

- **位置**：`bridge/auth_reload.rs:97-140,182-194`；`services/account_service/oauth_owner.rs:95-154,210-225`；`bridge/host/transport/failover.rs:472-503`。
- **触发/行为**：两个池持有同一登录的独立 token cell。A 已把 T0 更新为 T1；B 稍后仍用 T0 收到 401。B 进入刷新协调器时错过 generation 等待，回调因数据库已是 T1 返回 `None`，B 不采用 T1，随后按指纹跨池隔离该登录。
- **影响/证据**：有效 T1 也从所有池的后续选择中消失。源码确认；现有 singleflight 测试只覆盖同 cell 重叠刷新。**P1，高置信度**。
- **最小修复/验证**：即使数据库未再次变化也向调用者返回当前 token，隔离前核对失败请求版本；双 cell 先后 401 测试。

### AHREV-P1-008：登录刷新同步 I/O 阻塞异步工作线程

- **位置**：`bridge/host/transport/failover.rs:472-474`；`bridge/auth_reload.rs:125,182-192`；`services/account_service/oauth_owner.rs:157-174`；`oauth/providers.rs:252-271`。
- **触发/行为**：多个不同登录同时进行 Hub-owned token 刷新。异步请求线程同步执行最长 30 秒的 `ureq` 调用，没有阻塞执行边界。
- **影响/证据**：可占满 Tokio worker，拖住其他路由、取消与排空计时；singleflight 只约束同一指纹。生产调用链与测试中的 `block_in_place` 注释确认。**P1，高置信度**。
- **最小修复/验证**：同步刷新放入阻塞池并保留 singleflight 所有权；固定少量 worker、多指纹慢刷新测试其他请求和停止仍推进。

### AHREV-P1-009：Chat 启动中停止/删除可在子进程仍运行时报告成功

- **位置**：`src/pages/chat/runtime-run-state.ts:62-80`；`use-chat-page-send.ts:610-619,740-769,814-817`；`use-chat-page-sessions.ts:284-287`；`crates/agenthub-core/src/services/chat_runtime/mod.rs:314-353,528-540,548-633,831-866,1155-1158`。
- **触发/行为**：冷启动目录拉取或 actor 同步 `turn/start` 阻塞时停止/删除。前端 pendingStart 只记本地取消；冷启动 transport 尚未登记。actor 建立后也在同步命令期间不处理 Cancel/Shutdown；shutdown 忽略 5 秒确认超时并继续删库。
- **影响/证据**：会话已显示删除，子进程却可继续运行到 30 秒期限。源码可达路径确认。**P1，高置信度**。
- **最小修复/验证**：目录和 start 请求可取消，Shutdown 能抢占 transport；未确认退出时不删库。假 app-server 分别阻塞 `model/list` 与 `turn/start`。

### AHREV-P2-001：53 个生产 Rust 文件内嵌测试实现

- `git ls-files` + 正则确认 46 个 `crates/**` 文件、7 个 `src-tauri/**` 文件含 `#[cfg(test)] mod tests { ... }`，包括 `services/chat_runtime/ops.rs:751`、`src-tauri/src/commands/backup.rs:144`、`project.rs:152`、`sub2api.rs:323` 等。
- 与 `AGENTS.md`“生产侧只放模块声明，测试实现放相邻 tests.rs”直接冲突；不影响发布二进制运行。**P2，高置信度**。
- 按模块分批迁移，禁止顺带重构；迁移后核对测试数量和平台条件。

### AHREV-P2-002：路由成员回收失败会留下部分状态或重复记录

- `services/route_pool_service.rs:964-1044` 先自动提交回收记录，再逐池删除成员和同步投影；失败无整体补偿，重试可再插入记录，schema 无来源唯一约束。
- 主 Agent 将 reviewer 的 P1 **降为 P2**：现有恢复会跳过仍存在的成员，重复记录可逐条安全消耗，尚无同等级数据丢失证据；但部分成功、错误后 UI 不刷新和重复记录仍是真实正确性问题。
- 最小修复是同事务处理，或用稳定操作 ID + 失败补偿；在后续池/投影失败点注入测试。

### AHREV-P2-003：安装页打开失败仍提示“已打开”

- `services/install_service.rs:150-151,240-242,2665-2701`：`open_in_browser` 失败只写日志，仍返回 `spawn_error: None`，WorkBuddy/ZCode 等 setup-guide 入口继续生成“已打开官网安装页”。
- 用户状态与事实相反，但渠道窄且无数据风险。**P2，高置信度**。
- 保留打开失败到结果并走失败提示；注入浏览器打开器失败测试。

### AHREV-P2-004：actor 的 64 事件批处理通常每轮只读一个 wire 事件

- `services/chat_runtime/mod.rs:831-834,1625-1635` 后续 63 次传 `Duration::ZERO`；`codex_transport.rs:384-397` 在尝试 `wire_rx` 前因零期限返回。
- 正常流式通知约每轮一条，容量 128 的队列会反压 stdout，延迟消息、权限请求和终态。**P2，高置信度**。
- 后续读取改为真正 `try_recv`，用 200 条突发通知验证单轮 64 条和取消公平性。

### AHREV-P2-005：持续聊天后续发送不更新会话排序时间

- `storage/chat_repo.rs:91-100` 按 `updated_at DESC`；`services/chat_runtime/store.rs:490-501` 仅标题为空时更新会话行。已有标题的后续发送不会提升持久化排序，旧路径则每轮更新。
- 刷新后最近使用的会话仍可能排在旧位置。**P2，高置信度**。
- `begin_turn` 事务内无条件更新 `updated_at`，标题仍只在为空时赋值；增加两会话排序测试。

## 候选裁决

| 线索 | 裁决 | 理由 |
| --- | --- | --- |
| Early 接口可把停止视为 best-effort | invalid | best-effort 不等于整个 HTTP 等待期不观察取消 |
| 截断后空闲只是测试缺口 | invalid / merged | 生产活动时钟确实断链；测试缺口并入 P1-002 |
| 路由成员回收跨事务为 P1 | severity adjusted | confirmed，但恢复幂等缓解，降为 P2-002 |
| `edited`/`unknown` 文件保留也算恢复失败 | invalid | 明确安全策略；P1-003 仅针对 `delete_failed` 不可见 |
| 成员 permit 在响应头后释放 | needs-evidence | 现行文档只约束整体并发，没有成员许可必须覆盖完整 body 的契约 |
| Windows npm/Cursor 参数真实往返 | needs-evidence | 向量测试通过；未启动真实 CLI，不确认额外缺陷 |
| Windows 后代 PID 泄漏 | needs-evidence | 有 Job Object 与排空实现，现有测试不足以确认泄漏 |
| bundle 体积警告 | needs-evidence | 缺少启动/内存实测，不把警告直接列缺陷 |
| 凭据落盘加密、国产 OAuth、OAuth 转 API、按 Agent 禁止分享 API Key | out-of-scope | 项目明确排除 |

没有引用旧 review 作为当前证据；没有重复计算同一根因。P2-001 按全仓一个规则根因计一项，而不是按 53 个文件计数。

## 架构与成熟度评估

### 架构

- **前端/backend/Tauri**：构建时选择实现，唯一 invoke 包装与生产 module guard 有效；页面抽样未绕过 backend。command/DTO 抽样一致。
- **存储**：连接激活与迁移使用 IMMEDIATE/EXCLUSIVE 事务并有失败测试；但备份和路由回收跨服务步骤仍存在部分成功出口。
- **Chat**：运行状态持久化和重启收敛设计较完整；同步启动命令、取消和事件轮询仍有关键缝隙。
- **本机路由**：请求体限制、错误脱敏、停止 drain 基础较好；不明状态重放、跨池 token cell、同步刷新与 SSE 收尾影响长期可靠性。
- **安装/发版**：删除白名单、路径复核及 v0.4.10 元数据一致；安装页失败提示需要修正。

### 成熟度

| 维度 | 判断 | 依据 |
| --- | --- | --- |
| 架构 | 基础清晰，边界内仍有跨步骤缝隙 | backend 分层通过；路由/恢复原子性不足 |
| 正确性 | 不通过长期使用验收 | 9 项 P1 均有当前可达源码路径 |
| 数据一致性 | 局部成熟、关键恢复需修复 | 事务测试较强；P1-003/004 与 P2-002 |
| 并发/取消 | 需重点修复 | P1-001/007/008/009 |
| 跨平台 | Windows 定向较好，整体不足 | 无 macOS/Linux 和真实 CLI 验证 |
| 测试 | 数量充分但组合故障缺口明显 | 现有测试全绿，不覆盖本报告时序/失败点 |
| 维护 | 规则执行不一致 | 53 个生产 Rust 文件内嵌测试 |
| 发布准备度 | 版本元数据通过，运行时验收阻断 | release 检查通过但 P1 未修复 |

## 测试与本机验证

### 当前 HEAD `c726bd2a`

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | 退出 0 |
| `pnpm typecheck:test` | 退出 0 |
| `pnpm test:contracts` | 退出 0；4 文件、61 项通过 |
| `pnpm build` | 退出 0；3689 modules；生产/mock guard 通过 |
| `cargo test -p agenthub-core --locked backup_service` | 44 项通过 |
| `cargo test -p agenthub-core --locked route_pool_service` | 50 项通过 |
| `cargo test -p agenthub-core --locked codex_responses_oauth_messages_stream` | 2 项通过 |
| `cargo test -p agenthub-core --locked auth_reload` | 9 项通过 |
| `cargo test -p agenthub-core --locked v2_concurrent_401_reload_is_singleflight` | 1 项通过 |
| `cargo test -p agenthub-core --locked stop_drains_an_inflight_request_before_returning` | 1 项通过 |
| `cargo test -p agenthub-core --locked chat_runtime` | 65 项通过、6 项真实登录测试 ignored |
| `pnpm release:check` | 退出 0；0.4.10 与 changelog 元数据通过 |
| `pnpm pricing:check` | 退出 0；嵌入表一致 |
| `git rev-parse 'v0.4.10^{}'` | 指向 c726bd2a |

当前 HEAD 共 **172 项 Rust 定向测试通过**。第一轮在源码等价的 `06b893c4` 另有 60 项 Rust、36 项额外 Vitest 通过；累计记录为 **232 项 Rust、97 项前端测试**。测试通过不覆盖报告列出的故障组合。

build 保留两个非阻断警告：runtime 同时静态/动态 import；主 JS 约 3586.60 kB，超过 3200 kB 警戒值。未通过调阈值隐藏警告。

未运行真实登录、付费请求、完整 workspace、桌面打包、macOS/Linux 或全量端到端。

## 多模型记录

执行前均调用 capabilities/model 列表，只使用注册且可执行的 native Agent；codex-exec runner 缺失，未调用，也没有静默改用外部 CLI。

| 阶段 | Agent / 模型 | 分工 |
| --- | --- | --- |
| 第一轮 | scout / `openai-codex/gpt-5.4-mini:low` | 边界、近期改动、大文件定位 |
| 第一轮 | reviewer / `gpt-5.6-terra:high`、`gpt-6-astra:high` | backend/存储与运行时复核 |
| 第一轮 | reviewer / `xai/grok-4.3` | 反方挑战 |
| 报告审核 | reviewer / `xai/grok-4.6` | 逐项审计原报告，发现 3 处文字/行号修正 |
| 第二轮 | scout / `gpt-5.4-mini:low` | 存储、路由、安装索引 |
| 第二轮 | reviewer / `gpt-5.6-sol:high`、`gpt-6-astra:high`、`gpt-5.6-terra:high` | 存储、SSE、生命周期 |
| 第二轮 | reviewer / `xai/grok-4.6` | P1/P2 反方复核 |
| 第三轮 | scout / `gpt-5.4-mini:low` | 授权池、Chat runtime、wire/release 索引 |
| 第三轮 | reviewer / `gpt-6-astra:high`、`gpt-5.6-sol:high`、`gpt-5.6-terra:high` | 并发/停止、状态机、契约与发版 |
| 第三轮 | reviewer / `xai/grok-4.6` | 最终候选反方复核 |

机械扫描使用低成本模型；跨事务、并发、取消与协议问题才升级强模型。主 Agent 回读争议源码、运行定向验证并最终降级 P2-002；模型意见不是投票结论。

Goal Missions：第一轮 `653f4321-ab14-420e-bc9b-93e666caab00` 已关闭；第二/三轮 `0c17780c-8885-437b-8836-06e8d6d3fdb7` 在最终文档检查后关闭。

## 残余风险与建议顺序

### 建议顺序

1. **立即修复**：P1-004（恢复入口丢失）、P1-006（不明状态重放）、P1-003（恢复误报）。
2. **同一批处理**：P1-007/008 的登录刷新边界；P1-001/009 的停止和子进程生命周期。
3. **随后修复**：P1-002 长输出活动、P1-005 SSE 收尾。
4. **下一轮**：五项 P2，优先事件轮询和全仓测试分离的分批迁移。
5. **不建议**：范围外产品事项、无证据的大重构、仅为消除 build 警告扩大阈值。

### 残余风险

- 9 项 P1 均未修复；多数是源码确认、尚缺专门失败测试或本机复现。
- 未测试真实上游断连后的计费、真实登录刷新、多平台文件占用及安装升级。
- 未穷尽所有协议组合、文件补偿、数据库迁移、日志与资源耗尽路径。
- 本机 `origin/*` 未 fetch；只说明本机引用状态。
- 报告是风险清单，不是修复授权；本轮未改生产代码。

## 最终结论与文档检查

**Review verdict: BLOCK**。

当前项目具备清晰的基本分层和相当数量的有效测试，但停止/取消、恢复原子性、路由重放与登录刷新仍存在真实可达的重要缺陷，不能判定已满足真实用户长期使用的可靠性要求。

最终报告通过 `pnpm check:docs`：退出 0，`Documentation checks passed (83 Markdown files)`；`git diff --check` 无错误。检查收尾时当前仓库已由外部推进到 `5b16767e` 并出现新的未提交改动，因此本报告按约定保留为 `c726bd2a` 的历史精确审查，不冒充后续代码状态。
