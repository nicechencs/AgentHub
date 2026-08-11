# AgentHub 文档索引

仓库入口见根目录 [README.md](../README.md)。项目约定见 [AGENTS.md](../AGENTS.md)。漏洞披露见 [SECURITY.md](../SECURITY.md)。

## 稳定设计文档

| 文档 | 内容 |
|---|---|
| [agenthub-plan.md](agenthub-plan.md) | 产品方案、决策、适配矩阵、模块、路线图、风险（当前 **v1.4**；含 **§5.7.5 平台环境差异**） |
| [architecture.md](architecture.md) | cargo workspace 目录、core/gui/cli 拆分、Service/Adapter、**runtime/env / host_runtimes**；**前端目标目录（`lib/backend` / `dev/mocks`）与 pnpm 命令 ↔ adapter** |
| [platform-capability-refactor.md](platform-capability-refactor.md) | **平台能力架构改造方案**：解耦边界、稀疏端口、生命周期/Skills/连接/用量；P01-P13 与 R00-R08 已完成 |
| [platform-capability-remediation.md](platform-capability-remediation.md) | **2026-08-07 审查修正方案**：Active Binding、配置 fail-closed、Skills 安全/原子性、Lifecycle 审计与 AgentKey/OCP 真验证 |
| [testing.md](testing.md) | **测试约定**：测试与生产分文件、vitest/cargo 命令、mock 边界、Markdown 预览用例索引 |
| [ui-design.md](ui-design.md) | 前端布局、页面线框、交互与组件（含环境未就绪态） |
| [ui-experience-alignment.md](ui-experience-alignment.md) | **UI 风格/体验对标 Cursor·Codex**：颜色层级、边框、字号、预览与提示体系、分阶段优化方案（**v1.0**） |
| [cli-and-config.md](cli-and-config.md) | **CLI 命令树 / 退出码 / GUI 矩阵 / 配置 L0–L3 契约与验收清单**（**v1.2**；doctor/env 宿主 Runtime） |
| [logging.md](logging.md) | **日志规范**：级别、文件路径、保留、module 字段、脱敏、排查与 `config` 键（**v1.0**） |
| [chat-process-streaming.md](chat-process-streaming.md) | **Chat 过程流式**：Cursor 式步骤/工具展示的统一模型、分 Agent 接入、Phase 0–3 计划 |
| [capability-matrix.md](capability-matrix.md) | **能力矩阵**：`Capability` 枚举 + 四级状态、现状矩阵、防漂移测试、P0–P4（**v1.0 已落地**） |
| [account-authorization-pool.md](account-authorization-pool.md) | **账号池身份×授权**：同人可多授权并存；去重仅限同授权票；与能力矩阵边界（**已落地**） |
| [adding-an-agent.md](adding-an-agent.md) | 新增 Agent 适配器清单与调研步骤 |
| [privacy.md](privacy.md) | **发布与隐私边界**：禁止提交项、截图规范、OAuth 常量勿外泄、docs 写法 |

## 文档管理规则

- 本目录只保留当前有效的稳定设计、契约、规范、验收和最终状态文档。
- 一次性派工提示词、阶段任务清单和已替代的审查快照在执行完成后删除；最终结论只回写到稳定文档。
- 项目实现状态、未实现清单和风险以 [agenthub-plan.md §8](agenthub-plan.md) 为唯一真源。
- 平台能力改造的最终约束、暂缓项和验证证据以 [platform-capability-remediation.md](platform-capability-remediation.md) 为唯一真源。
- 对外发布、截图与凭据相关表述遵守 [privacy.md](privacy.md)。
- 新的一次性任务可以临时创建提示词文件；任务完成后删除提示词和任务拆分，并同步更新对应稳定文档。
