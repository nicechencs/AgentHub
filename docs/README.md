# AgentHub 文档索引

仓库入口见根目录 [README.md](../README.md)。项目约定见 [AGENTS.md](../AGENTS.md)。漏洞披露见 [SECURITY.md](../SECURITY.md)。

## 稳定设计文档

| 文档 | 内容 |
|---|---|
| [product-decisions.md](product-decisions.md) | 把已有登录接到另一个编程工具：三种接法、白话图、能接 / 接不上；后半是给实现的对照 |
| [agenthub-plan.md](agenthub-plan.md) | 产品方案、决策、适配矩阵、模块、路线图、风险（当前 **v1.5**；含平台环境差异与 Adapter sidecar 目标决策） |
| [architecture.md](architecture.md) | cargo workspace 目录、core/gui/cli 拆分、Service/Adapter；原则 12 按三路解释 `plan()` |
| [hardcoding-governance.md](hardcoding-governance.md) | **硬编码治理**：安装 allowlist / 定价表 / 路径真源；分层策略与落地状态 |
| [modularity-improvement.md](modularity-improvement.md) | **模块化审查与改进方案**（2026-08-16 回写）：integrations / Ticket 写口 / `adapter_control` 契约已落地；仍待削 Adapter 厚表面与 sidecar 二进制 |
| [platform-capability-refactor.md](platform-capability-refactor.md) | **平台能力架构改造方案**：解耦边界、稀疏端口、生命周期/Skills/连接/用量；P01-P13 与 R00-R08 已完成 |
| [platform-capability-remediation.md](platform-capability-remediation.md) | **2026-08-07 审查修正方案**：Active Binding、配置 fail-closed、Skills 安全/原子性、Lifecycle 审计与 AgentKey/OCP 真验证 |
| [testing.md](testing.md) | **测试约定**：测试与生产分文件、vitest/cargo 命令、mock 边界、Markdown 预览用例索引 |
| [ui-design.md](ui-design.md) | 前端布局、页面线框、交互与组件；三种做法是直接改配置 / 写进对方认的登录 / 本机转发。界面芯片是「直连 / 用这份登录 / 本机路由 / 当前不支持」。写进对方认的登录时不显示本机服务 |
| [ui-component-standard.md](ui-component-standard.md) | **UI 组件与体验标准**（**v1.0**）：现行清单、决策树、提示通道、对照审计与 Phase 3 收口；不替代页面线框 |
| [connection-binding-model.md](connection-binding-model.md) | 实现用的领域模型（一份登录 / 绑定 / 规划器）。**读模型 + plan/bind/unbind 已落地；sidecar 迁移未做**。读者向说明见 [product-decisions.md](product-decisions.md) |
| [hub-redesign-plan.md](hub-redesign-plan.md) | **Hub 重构 Phase 1 已实施**（历史）：ConnectFlowDialog；后续以 [connection-binding-model.md](connection-binding-model.md) 为 UI 与领域目标 |
| [adapter-design.md](adapter-design.md) | **Adapter 设计与进度**：用户表面 Routes / 本机路由，模块仍叫 Adapter；本页只服务本机转发；能改配置或写进对方认的登录就不另开程序；创建绑定走 Hub |
| [bridges-page-redesign.md](bridges-page-redesign.md) | **本机路由页终态 IA**（已落地，表面已改为 Routes / `/routes`）：对象是 loopback 进程；侧栏 Routes 有本机路由才出现；单层健康+端口。稳定文档已回写 ui-design / adapter-design / connection-binding-model |
| [adapter-kimi-codex-dogfood.md](adapter-kimi-codex-dogfood.md) | **真机 dogfood**：直接改配置（Kimi→Claude / Anthropic→Pi）；本机转发（Kimi→Codex）。禁止记录密钥 / prompt / 正文 |
| [adapter-sidecar-design.md](adapter-sidecar-design.md) | **Adapter Sidecar 目标架构**：`agenthub-adapterd` 所有权、IPC、状态机、单主/并发、升级恢复与三阶段迁移（目标已决策，当前未迁移） |
| [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) | **厂商 / API / OAuth 适配规则**：产品与协议边界、Kimi 双端点、当前路由矩阵和维护方法 |
| [ui-experience-alignment.md](ui-experience-alignment.md) | **UI 风格/体验对标 Cursor·Codex**：颜色层级、边框、字号、预览与提示体系、分阶段优化方案（**v1.1**） |
| [cli-and-config.md](cli-and-config.md) | **CLI 命令树 / 退出码 / GUI 矩阵 / 配置 L0–L3 契约与验收清单**（**v1.4**；doctor/env 宿主 Runtime） |
| [logging.md](logging.md) | **日志规范**：级别、文件路径、保留、target=module、必打事件、脱敏、排查与分期补点（**v1.1**） |
| [chat-process-streaming.md](chat-process-streaming.md) | **Chat 过程流式**：Phase 0–2 现行契约；Phase 3 **展示层已落地**；协议侧未做；§12 三条实现缺口已收口 |
| [chat-page-redesign.md](chat-page-redesign.md) | **Chat 工作台已落地**：会话 rail / header 芯片 / **一会话一 Agent** / 过程面板展示层 / 文件拆分 |
| [chat-ui-agent-mechanism-comparison.md](chat-ui-agent-mechanism-comparison.md) | **对照笔记**（2026-08-21，同日按 `d7da2f5` 复核）：AgentHub Chat × DSH Desktop 的 UI↔Agent 机制；§6 逐条深挖，§6.15 代码复核表。**不是** backlog，不派生实施任务 |
| [capability-matrix.md](capability-matrix.md) | **能力矩阵**：`Capability` 枚举 + 四级状态、现状矩阵、防漂移测试、P0–P4（**v1.0 已落地**） |
| [account-authorization-pool.md](account-authorization-pool.md) | **账号池身份×授权**：同人可多授权并存；去重仅限同一份登录；与能力矩阵边界（**已落地**） |
| [adding-an-agent.md](adding-an-agent.md) | 新增 Agent 适配器清单；`accepts[]` 须登记 wire 协议 **和** OAuth 契约槽 |
| [deepseek-harness-integration.md](deepseek-harness-integration.md) | **DeepSeek Harness（`dsh`）**：DeepSeek API 走直接改配置；DSH 不是本机转发。P1–P5 已落地；StructuredStream 仍 Planned |
| [tray-background-modes.md](tray-background-modes.md) | **托盘后台模式**：低内存后台 vs 隐藏界面；hide 不降内存的原因、三档设置设计与实施要点（**未来优化点，未实施**，不派生当前任务） |
| [routing-connection-refactor-plan.md](routing-connection-refactor-plan.md) | **路由 × 连接重构任务拆分**（2026-08-22 制定，**未实施**）：对齐 §5.4 表面统一与 §5.5 多账号轮询的四条泳道任务卡与派工波次；完成后回写稳定文档并删除本文 |
| [privacy.md](privacy.md) | **发布与隐私边界**：禁止提交项、截图规范、OAuth 常量勿外泄、docs 写法 |

## 文档管理规则

- 本目录只保留当前有效的稳定设计、契约、规范、验收和最终状态文档。
- 计划类 / 已落地实施记录须在条目上标明状态，避免当未完成任务派工。
- 一次性派工提示词、阶段任务清单和已替代的审查快照在执行完成后删除；最终结论只回写到稳定文档。
- 项目实现状态、未实现清单和风险以 [agenthub-plan.md §8](agenthub-plan.md) 为唯一真源。
- 平台能力改造的最终约束、暂缓项和验证证据以 [platform-capability-remediation.md](platform-capability-remediation.md) 为唯一真源。
- 模块化债、双真源收口与上帝文件拆分以 [modularity-improvement.md](modularity-improvement.md) 为改进方案真源；不替代 architecture / platform-capability / sidecar 既有决策。
- 对外发布、截图与凭据相关表述遵守 [privacy.md](privacy.md)。
- 把已有登录接到另一个编程工具的**产品方向**（直接改配置 / 写进对方认的登录 / 本机转发）以 [product-decisions.md](product-decisions.md) 为唯一真源。旧句「订阅 = 必须转发」「消费订阅不是产品」作废。
- 「把已有登录接到另一个工具」的领域模型（界面说登录；实现里仍叫票 / 绑定 / 协议图）以 [connection-binding-model.md](connection-binding-model.md) 为唯一真源。读模型 + `plan` / `bind` / `unbind` 已落地；sidecar 迁移未做。实现状态仍以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 为准。
- 各家接口、凭据类型与现在能不能写上去以 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) 为规则真源。日常 UI 页面线框与业务交互以 [ui-design.md](ui-design.md) 为准；组件用法、决策树与现行清单以 [ui-component-standard.md](ui-component-standard.md) 为准。Phase 1 实施记录见 [hub-redesign-plan.md](hub-redesign-plan.md)。该文的实现矩阵描述**当前能否写入**，不表示产品否决某一种做法。
- DeepSeek Harness（Agent `dsh`）的安装、会话、用量、Skills 与模型配置以 [deepseek-harness-integration.md](deepseek-harness-integration.md) 为设计真源；未实现前不得把该文能力表抄进 CLI 矩阵快照。
- Chat UI↔Agent 与 DSH Desktop 的机制对照以 [chat-ui-agent-mechanism-comparison.md](chat-ui-agent-mechanism-comparison.md) 为笔记；不替代过程契约、Chat IA 与 DSH 接入方案，不派生实施任务。凭据落盘加密与国产 OAuth 仍为范围外。
- `local_bridge` 的进程所有权、控制面和 sidecar 迁移契约以 [adapter-sidecar-design.md](adapter-sidecar-design.md) 为唯一真源；当前实现状态仍以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 为准。
- 托盘后台内存模式（省电 / 深度低内存）以 [tray-background-modes.md](tray-background-modes.md) 为设计记录；**未实施**，实现前不进入能力矩阵与 CLI 矩阵。
- 新的一次性任务可以临时创建提示词文件；任务完成后删除提示词和任务拆分，并同步更新对应稳定文档。
