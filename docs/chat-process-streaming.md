# Chat 过程流式展示设计

> 状态：**Phase 0–2 是现行契约**（2026-08-03）；Phase 3 **展示层已落地**（2026-08，见 [chat-page-redesign.md](chat-page-redesign.md)）。协议侧未做：过程落库、过程内 usage、Pi rpc 审批、diff 预览落库。  
> **现行状态**：本文管过程协议与展示契约，不是 Chat 页 IA。Chat **没有**独立模型选择器；chrome 见 [ui-design.md](ui-design.md) §4.4。Chat **一会话一 Agent**（过程仍按 `(turn, agent)` 隔离）。§12 三条实现缺口已于 2026-08-21 收口。  
> 范围：GUI Chat 的「Cursor 式过程」——命令、状态、stderr、结构化工具/thinking 步骤  
> 非目标：接管各 CLI 原生多轮 session、交互式 tool 审批（RPC）、凭据加密  
> mock 路径：`src/dev/mocks/chat.ts`。`src/lib/api/chat.ts` 是生产 façade，不是 mock。  
> 与 DSH Desktop 的 UI↔Agent 对照见 [chat-ui-agent-mechanism-comparison.md](chat-ui-agent-mechanism-comparison.md)（对照笔记，不改本文契约）。

## 1. 背景与问题（历史；Phase 0–2 已修复主体）

Chat 链路支持子进程流式（`StreamingProcessRunner` → `RunEvent` → `ChatEvent` → Tauri `Channel`）。早期 UI 只拼 stdout，过程信息丢失。

| 能力 | 当前（Phase 0–2 后） |
|------|------|
| 文本流式 | ✅ |
| 启动命令 | ✅ 过程面板展示 `agentStarted.command` |
| stderr / 日志 | ✅ 进过程区，不进气泡正文 |
| 细粒度状态 | ✅ queued/running/终态 |
| 工具 / thinking 步骤 | ✅ Claude/Codex/Kimi/Grok/Pi 结构化；WorkBuddy/Cursor 仍 text |

根因曾是强制 text + 无过程模型；现已由 `ProcessMode::Auto` + `stream_parse/*` + `ProcessStep` 解决五家结构化流。

## 2. 目标

1. **统一过程模型**：前端与 core 用同一套语义事件展示「过程」，与各家原始 JSON schema 解耦。  
2. **按 Agent 能力分级**：有结构化 parser 的 Agent 走结构化流；否则 text；未知事件降级为 raw。  
3. **可回退**：解析失败或旧 CLI 仍可按 text 流展示，不中断对话。  
4. **过程按 `(turn, agent)` 隔离**：禁止混成单时间线。现行产品一会话一 Agent；隔离键保留是为了历史行与 `run_each` 形状。

## 3. 现状数据流（真源）

```
chat_send (Tauri Channel<ChatEvent>)
  → ChatService::send
      · insert turn + running placeholders
      · RunService::run_each (Parallel)
          · Adapter::build_run_spec (Chat: ProcessMode::Auto → 结构化 flag)
          · StreamingProcessRunner
              · RunEvent::{Started,Chunk,Step,Finished}
              · stream_parse/* 行缓冲 → ProcessStep
      · map → ChatEvent::{Started,AgentStarted,AgentChunk,AgentProcess,AgentFinished,Finished,Error}
  → 前端 applyEvent + reduceProcessEvent 更新 messages / 过程面板
```

`ChatEvent` 已含 `agentProcess` + `ProcessStep`（core 与 `src/lib/types.ts` 同步）。CLI `run` 保持 text 模式。

## 4. 统一过程模型（目标态）

### 4.1 内部类型（建议 Phase 1 落入 core + TS 同步）

```ts
/** 归一化后的过程事件；与各家 CLI schema 解耦 */
export type ProcessEvent =
  | { type: 'status'; phase: ProcessPhase; detail?: string }
  | { type: 'command'; command: string }
  | { type: 'thinking'; text: string; done?: boolean }
  | {
      type: 'tool';
      id?: string;
      name: string;
      input?: unknown;
      status: 'start' | 'update' | 'end';
      result?: string;
    }
  | { type: 'text'; text: string; stream?: 'stdout' | 'stderr' }
  | { type: 'raw'; text: string; note?: string }
  | { type: 'error'; message: string };

export type ProcessPhase =
  | 'queued'
  | 'starting'
  | 'running'
  | 'ok'
  | 'failed'
  | 'cancelled'
  | 'timeout';
```

### 4.2 与现有 ChatEvent 的关系

| 阶段 | 策略 |
|------|------|
| Phase 0 | **不改** `ChatEvent` wire 格式；前端用 `reduceProcessEvent` 从现有事件推导 `AgentProcessView` |
| Phase 1+ | 新增可选事件，例如 `agentProcess` / `agentStep`（camelCase + tag），或在 `AgentChunk` 旁并行推送；**旧前端忽略未知 type** |
| 持久化 | Phase 0–1 **不**把过程步骤写入 `chat_messages`（仅 stdout 终稿进 `content`）；后续若需要回放再加 `chat_process_events` 表 |

### 4.3 UI 视图模型（Phase 0 已用）

```ts
type AgentProcessView = {
  turn: number;
  agent: AgentId;
  phase: ProcessPhase;
  command?: string;
  stdout: string;
  stderr: string;
  steps: ProcessStep[]; // 已落地；真源 `src/lib/chat-process.ts`（`ProcessStep` 在 `src/lib/types.ts`）
  updatedAt: number;
};
```

Key：`` `${turn}:${agent}` ``。会话切换时清空；同会话多轮可保留，便于当轮回看命令/日志。

## 5. 各 Agent 能力与接入计划

| Agent | Chat 过程 | Parser 位置 | 备注 |
|-------|-----------|-------------|------|
| Claude / Codex / Kimi / Pi / Grok | 结构化流（`ProcessMode::Auto`） | `stream_parse/<agent>.rs` | 事件 schema 以 parser 源码为准 |
| WorkBuddy / Cursor | text only | — | 能力矩阵 StructuredStream = Unsupported |

CLI flag 与事件字段随上游版本变化，**不在本文抄写完整映射**；以 adapter `build_run_spec` 与 parser 实现为真源。

**兼容开关（建议）**：`RunOptions` 或会话级 `processMode: 'text' | 'structured' | 'auto'`；`auto` = 有 parser 用结构化，否则 text。

## 6. 架构变更（Phase 1+）

```
adapters/*
  build_run_spec(..., ProcessMode)  // 按 mode 换 flag

utils/stream_parse/                 // 行缓冲 + 各家 parser（现行）
  claude.rs / codex.rs / kimi.rs / grok.rs / pi.rs
platform/stream                     // 兼容层：StreamParser port + registry；
                                    // 各家 NDJSON 解码在 integrations/agents/<key>/
                                    // 不是 stream_parsers/

run_service
  行缓冲 stdout → parser.feed(line) → on_process(ProcessEvent)
  同时可保留原始 chunk 供 debug

chat_service
  ProcessEvent → ChatEvent 扩展变体 / 或复用 AgentChunk + agentStep

frontend
  reduceProcessEvent → Timeline + ToolCard + Thinking + 正文
```

### 6.1 行缓冲约定

- 结构化模式按 **`\n` 切行**；残缺行留在 buffer。  
- 单行超长（如 > 256KiB）截断并记 `raw`。  
- 继续遵守 `max_output_bytes`（当前 2MiB）；过程事件可单独 cap 条数（如 2000 steps / turn）。

### 6.2 失败回退

1. JSON 解析失败 → 该行变 `ProcessEvent::raw`，不中断。  
2. 连续 N 行无法识别且不像 JSON → 整段 fallback 为 text 模式。  
3. CLI 不支持 flag（exit 非 0 且 stderr 含 unknown option）→ 可选自动重试 text（需明确产品策略，默认 **不静默重试**，仅报错）。

## 7. 分阶段交付

### Phase 0 — 可见过程底座（本分支）

- [x] 设计文档（本文）  
- [x] 前端消费 `agentStarted.command`  
- [x] 前端累积并展示 `stderr`  
- [x] 状态：排队/启动/生成中/完成/失败/取消  
- [x] 可折叠「过程」面板（命令 + 状态 + stderr）  
- [x] 纯函数 `reduceProcessEvent` + 单测  
- [x] Phase 0 不强制改 Adapter flag（Phase 1 起 Auto）  

### Phase 1 — Claude + Codex 结构化（本分支已实现）

- [x] `ProcessMode` + Chat 默认 `Auto`；CLI `run` 保持 `Text`  
- [x] Claude / Codex 结构化 flag 与 parser（细节见源码）  
- [x] `utils/stream_parse`（line buffer + 各家 parser）  
- [x] `RunEvent::Step` / `ChatEvent::AgentProcess` + `ProcessStep`  
- [x] UI：步骤时间线 + 工具/thinking 卡片  
- [x] 结构化模式下正文只拼 assistant text，不落 raw 事件流  
- [x] 日志：stream session open/close、agent_started、process step(trace)  

### Phase 2 — Kimi + Pi + Grok（本分支已实现）

- [x] Kimi / Pi / Grok 结构化 parser  
- [x] `ProcessMode::Auto` 覆盖支持结构化流的 Agent（WorkBuddy / Cursor 仍 text）  

### Phase 3 — 体验打磨

> **展示层已落地**（摘要行 / 无边框步骤时间线 / 命令·stderr·exit 收进「运行详情」次级折叠），
> 见 [chat-page-redesign.md](chat-page-redesign.md)。属 Chat 页 UI 重设计范围，
> **不改本文的过程模型、事件 wire 与内存策略**。下列**协议侧**仍未做：

- diff 预览落库、过程内 usage  
- 过程步骤落库回放  
- 交互式 tool 审批（Pi rpc；若上游提供稳定契约）  

## 8. Phase 0 实现说明

| 文件 | 职责 |
|------|------|
| `docs/chat-process-streaming.md` | 本设计 |
| `src/lib/chat-process.ts` | `AgentProcessView` + `reduceProcessEvent` |
| `src/lib/chat-process.test.ts` | 状态机单测 |
| `src/pages/chat/ChatProcessPanel.tsx` | 过程面板渲染（摘要行 + 时间线 + 运行详情） |
| `src/pages/chat/use-chat-page.ts` | 会话切换清空 `processMap` |
| `src/lib/api/chat.ts` | 生产 façade（不是 mock） |
| `src/dev/mocks/chat.ts` | mock 路径（`dev:mock` / 测试） |

行为要点：

- `started` → 各 agent `phase=queued`  
- `agentStarted` → 写入 `command`，`phase=running`  
- `agentChunk` stdout → 正文（既有）+ process.stdout  
- `agentChunk` stderr → process.stderr（不进气泡正文）  
- `agentFinished` → phase 映射 message.status  
- 过程数据仅内存；刷新页面后历史 turn 无命令/stderr（可接受）

## 9. 风险

| 风险 | 缓解 |
|------|------|
| 各家 schema 漂移 | parser 容错 + raw；版本探测可选 |
| 结构化事件流体积大 | 行 cap + max_output_bytes |
| 多 Agent 并行写同 cwd | 现行一会话一 Agent，并跑风险已收口；过程 UI 不解决 cwd 冲突；后续 worktree |
| 危险 auto-approve | 与过程展示正交；仍默认关闭 |
| 把 tool 参数当可信 UI | 展示时转义；不自动执行 |

## 10. 验收清单

### Phase 0–2（已实现，验收对照）

- [x] 发送后气泡旁可见「过程」折叠区  
- [x] 运行中展示状态「生成中」，结束后为完成/失败/取消  
- [x] 启动命令 / stderr 在过程区  
- [x] Claude/Codex/Kimi/Grok/Pi 结构化步骤（tool/thinking/status）  
- [x] text fallback 仍可完整出字（WorkBuddy/Cursor 等）  
- [x] `chat-process` 单测  

### Phase 3 展示层（已落地）

- [x] 摘要行 = 阶段 · N 步 · 耗时（不含命令）  
- [x] 无边框步骤时间线；命令 / stderr / exit 收进「运行详情」  

### Phase 3 协议侧（未做）

- [ ] diff 预览落库、过程内 usage 展示  
- [ ] 过程步骤可选落库回放  
- [ ] Pi rpc 交互审批  


## 11. 决策记录

- **凭据落盘加密**：范围外（见 `AGENTS.md`）。  
- **过程不落库（Phase 0–1）**：降低迁移成本；终稿仍在 `chat_messages.content`。  
- **Pi 暂不默认 rpc**：json print 模式成本更低；rpc 留给交互审批。  
- **不静默跨模式重试**：避免重复扣费/双跑；错误显式暴露。

## 12. 已知问题（2026-08-21 已收口）

对照笔记 [chat-ui-agent-mechanism-comparison.md](chat-ui-agent-mechanism-comparison.md) §6.14 / §6.15 只作机制指针。下列不是 Phase 3 协议侧待办（落库 / usage / Pi rpc / diff 预览）。

1. **崩溃残留 `status=running`（已修）**：`AgentHub::open` 在 lifecycle interrupt 之后把所有 `chat_messages.status=running` 收成 `cancelled`（不清 `native_session_id`、不改 error）。不在 `list_messages` 上 interrupt（与 persist 竞态）。`Conversation.sending` 是运行时投影（存在 running 行），Chat 页 `loadList` 用它恢复 Stop。
2. **过程 cap（已修）**：UI `MAX_STEPS=200` 仍在，但 `capSteps` 优先保留 `tool` / `error`；软步（thinking / status / raw / text）从最旧丢。tool+error 自身超过 200 时只留最近 200 条。无 `step.id` 的 tool 仍不 merge（parser 必须给稳定 id）。
3. **取消时 `Finished` 信号（已修）**：`ok` 仍是 `!is_hard_failure`（取消时 true）。新增 `cancelled: bool`（任一 `RunStatus::Cancelled`）。`reduceProcessEvent('finished')`：`cancelled` → `cancelled`，否则 `ok` → `ok`，否则 `failed`。mock 与生产对齐：取消时 `ok: true, cancelled: true`。
