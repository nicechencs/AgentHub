---
title: 添加 Agent
description: 按稀疏端口、能力声明和目录注册把一个 Agent 接入 AgentHub。
type: guide
audience: contributor
status: current
updated: 2026-09-05
---

# 添加 Agent

本指南适用于新增一个由 AgentHub 管理的第三方 Agent。目标是让差异停留在 adapter 和 `integrations/agents/<key>/`，平台 service、页面和通用工具不新增具体 Agent 名称分支。

## 1. 先确认身份和范围

先确定唯一的小写 key，例如 `dsh`，再确认：

- 可检测的 binary 和版本命令；
- 可执行安装渠道及所需 Runtime；
- home/config/skills/project/usage 的公开边界；
- 哪些能力是真实可用，哪些是 Partial、Unsupported 或 Planned。

不要把厂商 API、模型名或登录方式当成 Agent key。登录、provider 和 Agent 是不同领域对象，术语见 [terminology.md](../reference/terminology.md)。

## 2. 注册生产入口

按以下顺序修改，修改前先读取相邻 Agent 的实现和测试：

| 步骤 | 位置 | 要求 |
|---|---|---|
| Adapter | `crates/agenthub-core/src/adapters/<id>.rs`、adapter registry、`register_all()` | 实现现有 trait；只暴露该 Agent 真实拥有的能力 |
| 稀疏端口 | `crates/agenthub-core/src/integrations/agents/<key>/` | 按需增加 `paths`、`install`、`config`、`usage`、`stream`、`project` 等贡献 |
| 身份 | `crates/agenthub-core/src/models/agent.rs` | 增加 `AgentId`、`ALL`、解析和展示名；兼容期仍需维护生产 façade |
| 占用 | `agent_bind_capability` / `LiveOccupancy` | 声明独占写入、具名槽还是目录追加；WorkBuddy 模型行 / ZCode 供应商行是目录追加，不要默认覆盖整份配置 |
| 目录 | Agent catalog / install registry | 让 doctor、Agents 页和安装流程从 catalog 看到同一份元数据 |
| 前端 | `src/config/agents.ts`、`KNOWN_AGENT_IDS`、`src/styles/tokens.ts` | `agents.ts` 仅展示装饰；catalog 是列表真源，已知 id 集合不是封闭业务枚举，颜色集中在 tokens |

平台端口通过 registry 注入。不要在 `platform/*` service、页面或通用 utils 中写 `match AgentId` 来补功能。

## 3. 声明能力

`Capability::ALL` 当前是 14 个能力键。每个 adapter 的 `capability()` 必须穷尽匹配，不能用 `_ =>` 兜底；新增能力键应让所有 adapter 编译失败，迫使每家给出明确答案。

四种级别的含义：

- `Full`：已经接入且契约完整。
- `Partial`：可以使用，但存在已知降级，调用方必须提示用户。
- `Unsupported`：对方 Agent 的稳定契约不存在或明确做不到。
- `Planned`：AgentHub 尚未接入，属于路线图，不得伪装成已支持。

所有非 `Full` 状态都必须带原因。静态能力和运行时安装状态是两个维度：能力由 adapter 声明，安装/版本由 detect 结果提供。

## 4. 接入稀疏端口

只注册有证据的端口：

| 端口 | 适用条件 |
|---|---|
| `paths` | 能确定 home、配置目录或 skills 目录 |
| `install` / `lifecycle` | 有可审计的 npm/native 安装渠道；只有官网、没有脚本时只打开官网并给中文指引，不要报成安装失败 |
| `config` | 能安全读取/合并公开配置字段；无法保证 round-trip 时保持只读或 fail-closed |
| `skills` | 有稳定的技能目标目录 |
| MCP inventory 路径 | 仅当用户级 MCP live 文件形状已验证；写入仍要求 `Capability::Mcp` 不再是 Planned |
| 插件 / extension 端口 | 仅当该 Agent 有官方 `plugin`/`install` CLI 或已验证的 enabled 清单；与 MCP 端口分开注册 |
| `usage` | 有脱敏 fixture 锁定日志字段 |
| `projects` | 有明确的会话/项目根和安全删除边界 |
| `stream` | 已验证结构化 stdout/事件协议 |

不支持的端口返回 typed unsupported；不要为了填满矩阵而伪造 Full，也不要把计划中的 install channel 放进可执行 catalog。本机配置里若混有不可导入的套餐登录，探测要标出来，导入必须排除它们。

## 5. 前端接线

1. Agent 列表读取 runtime catalog，不在页面复制 Rust 的 Agent 列表。
2. 配置页使用 `getAgentConfigSchema` 和 `GenericConfigForm`，有 projector 才开放写入。
3. 任何 Tauri 调用放在 `src/lib/backend/tauri/`；页面通过 backend contract 或 `lib/api` façade。
4. mock fixture 只为 `pnpm dev:mock` 和测试准备，不进入生产 build。
5. UI 说「登录」和「路由/Routes」；内部实现可使用 Ticket、Binding、bridge 等名称，但不要把内部名直接当用户文案。


## 5a. 页面触点（UI awareness）

接入新 Agent 时，先读 [页面模式](../ui/page-patterns.md) 里各页的 **Agent touchpoints**，再决定要不要改 catalog / 前端装饰：

| 典型需要感知的页面 | 何时 |
|---|---|
| Agents | catalog / install / detect / 隐藏 |
| Dashboard | Usage 解析器、ConnectFlow 直连/本机路由 |
| Connections | 导入、官方登录、API Key 写入与占用方式 |
| Routes（board/pool/tokens/activity） | `plan`/`bind`、入池、本机令牌写回 |
| Skills / Projects / Plugins / MCP | Skills 矩阵、ProjectHistory/Delete、插件列表或只读 MCP 路径 |
| Chat | StructuredStream / DangerousMode / SessionResume |
| Settings → 备份 | LiveBackup 快照身份 |
| Sub2API | 仅当要从站点导入 Key 到该 Agent |

不要在页面里写死新的 `match AgentId` 分支；列表仍以 runtime catalog 为准。功能落地后的文档映射见 [STYLE.md](../STYLE.md#功能完成后的文档更新映射)。

## 6. 测试

测试与生产文件分开。至少覆盖：

- adapter 的检测、路径和能力声明；
- catalog 含该 Agent，且未安装时页面不会假成功；
- 已注册端口的契约和 unsupported 行为；
- install contribution 的 Runtime 前置和失败结果；
- usage/project parser 的脱敏 fixture；
- `AgentId::ALL`、catalog 和 CLI capability 输出的一致性。

本地可先跑：

```text
cargo test -p agenthub-core --locked <filter>
pnpm test -- --run <test-file>
```

若只验证开放扩展路径，可参考 test-only `demo-agent`；它不得进入生产 registry、migration 或 UI。

## 7. 完成标准

- `register_all()` 和 catalog 都能发现 Agent；
- 能力声明诚实，所有非 Full 有原因；
- 端口注册后平台 service 无新增具体 Agent 分支；
- doctor、Agents 页和 CLI 对安装失败明确报告，不静默成功；
- 相关 Rust、Vitest、typecheck 和生产 build 通过。

不要把凭据落盘加密、keyring、主密码迁移或国产 OAuth 适配列为本任务的一部分；它们不在当前项目范围内。

