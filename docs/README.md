# AgentHub 文档索引

本目录是**开发与设计文档**，不是产品说明书。对外说明见根目录 [README.md](../README.md) 与 [product-decisions.md](product-decisions.md) 的白话部分。漏洞披露见 [SECURITY.md](../SECURITY.md)。

贡献者开发约定在仓库根目录 [AGENTS.md](../AGENTS.md)。

现行界面：Connections 行入口是「**分享 / 路由**」；侧栏与页标题是 **Routes / 路由**（默认显示，Settings 偏好可隐藏，`/routes` 仍可打开）。「本机转发」是做法名，不是侧栏名。

## 怎么读

| 你要找 | 打开 |
|---|---|
| 三种接法（白话） | [product-decisions.md](product-decisions.md) |
| 票 / 绑定 / `plan`·`bind`·`unbind` | [connection-binding-model.md](connection-binding-model.md) |
| 现在能不能写上去 | [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) §4 |
| 本机三条入口与转换 | [local-route-endpoints.md](local-route-endpoints.md) |
| 目录与分层 | [architecture.md](architecture.md) |
| 实现清单（已做 / 未做） | [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) |
| 页面线框 | [ui-design.md](ui-design.md) |
| 测试约定 | [testing.md](testing.md) |
| CLI | [cli-and-config.md](cli-and-config.md) |

已落地的实施记录、过期计划稿在 [archive/](archive/README.md)，不要当未完成任务派工。

## 现行契约

| 文档 | 内容 |
|---|---|
| [product-decisions.md](product-decisions.md) | 把已有登录接到另一个工具：直接改配置 / 写进对方认的登录 / 本机转发 |
| [connection-binding-model.md](connection-binding-model.md) | 领域模型（界面说登录；实现里仍叫票 / 绑定）。`plan` / `bind` / `unbind` 已落地；sidecar 未迁 |
| [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) | 厂商 / 凭据 / 现在能不能写入。§4 是可执行矩阵；§5.4 进程内统一网关已落地；§5.5 轮询内核已有、边默认关 |
| [local-route-endpoints.md](local-route-endpoints.md) | 本机 `/v1/messages` · `/v1/responses` · `/v1/chat/completions` 与上游转换；含 `GET /models` |
| [architecture.md](architecture.md) | workspace、core/gui/cli、前端 `lib/backend` 分层。目录树是示意，以源码为准 |
| [agenthub-plan.md](agenthub-plan.md) | 产品方案 v1.5。**§8 为实现状态真源**；§7 路线图是历史排期 |
| [ui-design.md](ui-design.md) | 页面线框与交互。芯片「直连 / 用这份登录 / 本机路由 / 当前不支持」 |
| [ui-component-standard.md](ui-component-standard.md) | 组件清单与决策树；不替代页面线框 |
| [adapter-design.md](adapter-design.md) | Routes 页与本机转发运行时（模块名仍叫 Adapter） |
| [route-detail-redesign.md](route-detail-redesign.md) | 路由详情面板（已落地） |
| [cli-and-config.md](cli-and-config.md) | CLI 命令树、退出码、配置 L0–L3 |
| [testing.md](testing.md) | 测试与生产分文件、vitest/cargo、mock 边界、CI |
| [logging.md](logging.md) | 日志级别、路径、脱敏、必打事件 |
| [capability-matrix.md](capability-matrix.md) | Agent「自己能不能」；表格以 CLI `agent capabilities` 为准 |
| [account-authorization-pool.md](account-authorization-pool.md) | 账号池：身份 × 授权去重（已落地） |
| [adding-an-agent.md](adding-an-agent.md) | 新增 Agent 清单；`accepts[]` 须登记 wire 协议和 OAuth 契约槽 |
| [deepseek-harness-integration.md](deepseek-harness-integration.md) | DeepSeek Harness（`dsh`）；P1–P5 已落地；StructuredStream 仍 Planned |
| [chat-process-streaming.md](chat-process-streaming.md) | Chat 过程流式契约；展示层已落地，协议侧未做 |
| [hardcoding-governance.md](hardcoding-governance.md) | 安装 allowlist / 定价表 / 路径真源 |
| [privacy.md](privacy.md) | 禁止提交项、截图规范、OAuth 常量勿外泄 |

## 目标 / 未实施

| 文档 | 内容 |
|---|---|
| [adapter-sidecar-design.md](adapter-sidecar-design.md) | `agenthub-adapterd` 目标架构。Phase 1 控制契约已落地；sidecar 二进制未开工 |
| [tray-background-modes.md](tray-background-modes.md) | 托盘低内存后台。**未实施**，不派生当前任务 |
| [modularity-improvement.md](modularity-improvement.md) | 模块化债：integrations / Ticket 写口 / `adapter_control` 已落地；仍待削 Adapter 厚表面与 sidecar |
| [adapter-kimi-codex-dogfood.md](adapter-kimi-codex-dogfood.md) | 真机 dogfood 清单（内部）。禁止记录密钥 / prompt / 正文 |

## 已落地、只留约束

| 文档 | 内容 |
|---|---|
| [platform-capability-refactor.md](platform-capability-refactor.md) | 平台能力改造方案（P01–P13 已完成） |
| [platform-capability-remediation.md](platform-capability-remediation.md) | 2026-08-07 审查修正（R00–R08 已完成） |
| [chat-page-redesign.md](chat-page-redesign.md) | Chat 工作台表面（已落地；一会话一 Agent） |
| [bridges-page-redesign.md](bridges-page-redesign.md) | Routes 页 IA（已落地；正文多为 `/bridges` 历史用词，现行 chrome 以 ui-design 为准） |
| [ui-experience-alignment.md](ui-experience-alignment.md) | Cursor/Codex 视觉对标（Phase 0–2 已做） |

## 对照笔记（不是 backlog）

| 文档 | 内容 |
|---|---|
| [chat-ui-agent-mechanism-comparison.md](chat-ui-agent-mechanism-comparison.md) | AgentHub Chat × DSH Desktop。**不派生实施任务** |

## 文档管理规则

- 本目录现行区只放有效契约、规范和当前状态。计划类 / 已落地实施记录标状态；一次性派工完成后删或移入 [archive/](archive/README.md)。
- 项目实现状态、未实现清单和风险以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 为准。
- 把已有登录接到另一个工具的**产品方向**以 [product-decisions.md](product-decisions.md) 为准。旧句「订阅 = 必须转发」作废。
- 领域模型以 [connection-binding-model.md](connection-binding-model.md) 为准。各家能不能写入以 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) 为准。
- 日常页面以 [ui-design.md](ui-design.md) 为准；组件以 [ui-component-standard.md](ui-component-standard.md) 为准。
- `local_bridge` 进程所有权与 sidecar 迁移以 [adapter-sidecar-design.md](adapter-sidecar-design.md) 为准；当前仍是 Tauri 进程内 Gateway。
- 平台能力改造的历史约束见 platform-capability-* 两篇；新增 Agent 走 [adding-an-agent.md](adding-an-agent.md)。模块化债见 [modularity-improvement.md](modularity-improvement.md)。
- 对外发布、截图与凭据相关表述遵守 [privacy.md](privacy.md)。
- 凭据落盘加密、国产 OAuth 开边 / 转 API：项目范围外，见根目录 [AGENTS.md](../AGENTS.md)。
- 新的一次性任务可以临时创建提示词；完成后删除或归档，并回写对应稳定文档。
