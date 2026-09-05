---
title: Codex Chat 接入验证与交接
type: status
status: current
owner: maintainers
audience: chat implementers
updated: 2026-09-05
---

# Codex Chat 接入验证与交接

本文记录 [Chat 统一体验方案](../proposals/chat-unified-experience.md) 的 S0 实施证据。用户已授权使用 GPT‑5.6 Terra / Luna 子 Agent 开始处理；先完成接入验证，不将协议发现当作正式聊天功能。

## 本轮范围

- Terra：环境基线调查、统一接口草案与后续任务边界。
- Luna：独立的无登录 Codex 协议检查脚本与 fake 子进程测试。
- 主 Agent：本机 schema 检查、真实脚本验证、证据整理与验收。

起点为 `dev` commit `db40f6fbf9c86a184707f7ff8259a5bac361bb24`。本轮没有修改生产 Chat 接入、用户配置或聊天数据库。S0 尚不能作为 S1–S3 已完成的依据。

## 环境调查

| 项目 | 本机观察 | 仓库 CI | 结论 |
| --- | --- | --- | --- |
| Node | 24.20.0 | 22 | 不同；不能直接把本机版本固定为项目基线 |
| pnpm | 10.12.4 | 9 | 不同；现有 lockfile v9 应继续保留 |
| Rust / Cargo | 1.97.1 | stable | CI 工具链随时间变化，尚未固定精确版本 |
| Codex CLI | 0.153.0，PATH 上的 npm 安装 | 未进行真实聊天认证 | 仅作为本轮协议样本版本 |
| 系统 | macOS / arm64 | Linux 测试，Windows/macOS 编译检查 | 不代表三平台聊天通过 |

仓库已有 `pnpm-lock.yaml`、`Cargo.lock`，CI 使用 frozen/locked 安装。尚无 Node/Rust 精确版本文件或 packageManager 声明。本轮不根据未经完整验证的本机工具修改所有 CI。T01 后续任务是选定兼容现有锁文件的精确版本，干净环境验证后同步仓库版本文件与 PR/release CI。

## 本机协议证据

本轮运行 `codex --version` 和 `codex app-server generate-json-schema --out ...`，后者使用仓库临时目录中的隔离 CODEX_HOME。生成内容留在被忽略的 `temp/chat-s0/schema/`，不提交整个自动生成目录。其他电脑可用相同版本重新生成；路径不是交接依赖。

| 功能 | 0.153.0 稳定 schema 观察 | 能证明什么 |
| --- | --- | --- |
| 会话 | `thread/start`、`thread/resume`、`thread/read` | 有协议定义；不能证明真实续接已成功 |
| 发送/补充/停止 | `turn/start`、`turn/steer`、`turn/interrupt` | 有协议定义；实际执行与停止终态待验证 |
| 确认/问答 | command/file approval、`item/tool/requestUserInput` | 有服务器请求与响应类型；允许/拒绝仍须真实实验 |
| 模型/思考强度 | `model/list`，Model 含 supportedReasoningEfforts/defaultReasoningEffort；turn/start 含 model/effort | 可以设计选项读取与单轮参数；不等于账号可调用所有返回模型 |
| 模式 | 稳定 schema 无 collaborationMode；加 `--experimental` 导出后出现 TurnStartParams.collaborationMode 与 collaborationMode/list | 属实验协议，真实生效保持 unknown；需要显式协商并验证，不能直接做“计划”菜单 |
| Skills | `skills/list`，存在技能调用相关类型 | 列表与实际加载/调用分开验证 |
| 插件 | `plugin/installed` 等方法 | 已安装列表不等于当前会话可调用；管理写入不在本轮范围 |

各类确认必须按对应 response schema 映射，不能把某个 decision 对象套用到所有方法。CLI 导出的 schema 与当前网页文档也可能有差异，后续固定方法/schema 摘要及测试样例后再生成生产类型。[Codex 官方接口](https://learn.chatgpt.com/docs/app-server)

## 检查工具与结果

已新增 `scripts/chat-codex-probe.mjs`，使用 Node 内置模块，无新增依赖。默认检查 initialize/initialized、model/list、skills/list、plugin/installed 与 ephemeral thread/start；参数使用 argv，HOME/CODEX_HOME 和工作目录隔离，继承环境采用白名单。报告仅含白名单摘要，不输出原始响应、私有路径或登录内容。请求/输出有界，结束时清理子进程和临时目录。

```text
pnpm chat:probe:codex
pnpm test:chat:probe
```

其他安装路径可运行 `node scripts/chat-codex-probe.mjs --executable <codex可执行文件路径>`；完整选项见 `--help`。检查需要本机已安装 Codex，但不需要登录或配置 API Key。没有安装时明确失败，不下载或自动换安装。

本机真实运行：`node scripts/chat-codex-probe.mjs` 退出码 0；Codex 0.153.0 / macOS，全部上述协议检查返回 `verified protocol only`，cleanup 为 ok。model/list 返回 5 个列表项；skills/list 返回 1 个目录结果、插件列表为空，不能据此断言已验证插件调用。固定值仅是当次结果，测试不要求其他机器返回相同数量。

最终用仓库入口复跑：`pnpm chat:probe:codex` 通过；`pnpm test:chat:probe` 11/11 通过。测试覆盖固定 CLI 子命令、握手顺序和分片 JSON、上游错误脱敏、超时/半行输出、异常退出、启动失败、子进程组清理、清理失败与 stderr 上限。子进程测试以实际 PID 确认退出；未确认退出时返回失败并保留临时目录。Terra 独立审查最终 APPROVED。`pnpm check:docs` 与 `git diff --check` 通过。未运行无关的应用全量测试；Windows/Linux 的进程清理仍需在对应系统实测。

检查不启动模型 turn，不读取真实登录，因此不能验证模型输出、工具批准、问答、停止或真实会话恢复。无登录 app-server 也可能自行访问公共服务，本工具不宣称网络隔离。真实任务和不同系统仍待独立验收。

## S1 接口决策草案

保留现有一次性 ChatPort 行为，新增持续聊天接口时显式区分，不把旧 chatSend 悄悄改成另一种返回语义。最终是否单独命名 port 由 T04 定稿；所有 invoke 仍限定在现有 Tauri backend。

最小操作：能力与选项读取、startTurn、getSnapshot/subscribe、respondApproval/respondInput、steerTurn、interruptTurn。所有操作关联 conversation/turn/runInstance；回复额外关联 requestId/clientRequestId，模型等设置暂不开放写入，直到对应实验通过。

统一事件和待处理请求沿用主方案的 sequence、持久化先于发布、单一状态 owner 和停止/允许先后规则。能力记录使用 supported/unsupported/unknown，并另外保存证据层级：schema、协议响应、真实执行；只有 schema 或协议响应时不得标成完整聊天支持。

建议后续顺序：

1. T02 补齐真实 turn、批准/拒绝、问答、补充、停止和恢复实验；无实际证据的功能留 unknown。
2. T04 固定统一 DTO 与事件样例，交叉核对 TypeScript/Rust，独立审查。
3. T05 先实现状态机与固定测试场景，验证重复请求和停止竞态。
4. T06 再实现事务保存与断线重放，迁移前明确 SQLite 一致性备份。
5. T07 使用同一契约实现页面与 mock，真实接入按 S2 单独验收。

此顺序不是新增权限要求；用户已授权实施，后续可以继续完成有证据的任务。未验证的协议不应先进入生产以掩盖实验缺口。

## Claude 与其他 Agent

T03 文档核对：Claude Agent SDK 是可行候选，但其第三方 claude.ai 登录/额度使用需要官方批准；不能默认替换为现有订阅登录。当前没有进行 SDK 登录或执行实验，保持主方案的 API Key/获许可路径候选，不改变当前 Claude CLI 行为。[Claude 官方说明](https://code.claude.com/docs/en/agent-sdk/overview)

ZCode 和其他 Agent 本轮没有新增实测能力，不扩大其功能声明。

## 未完成的验收

- T01 精确工具版本固定及干净环境验证。
- T02 已登录真实 turn 与 A03–A08 实验，包括批准/拒绝和恢复。
- T04 统一接口实现前定稿；当前只形成草案。
- Windows/Linux 真实协议检查、三平台聊天测试。
- S1–S5 的生产代码与页面实现。

后续不得把本文“schema 存在”或无登录检查通过改写成 S0 整体验收通过。
