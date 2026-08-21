# Chat UI ↔ Agent 处理机制对照（AgentHub × DSH Desktop）

> **定位**：对照笔记。记录 AgentHub Chat 与参考仓库 DeepSeek Harness Desktop 的 UI↔Agent 处理机制，以及可学习经验。  
> **日期**：2026-08-21（同日补 §6 逐条深挖）  
> **不是**产品方案、不是 backlog、**不派生实施任务**。过程落库 / 交互式 tool 审批 / print+resume 范围仍以既有契约为准。  
> 总览在 §0–§5；逐条机制、不变量与「结构差 / 实现差」在 **§6**。  
> **真源关系**：
>
> | 主题 | 真源 |
> |---|---|
> | Chat 页 IA / chrome | [ui-design.md](ui-design.md) §4.4、[chat-page-redesign.md](chat-page-redesign.md) |
> | 过程协议与 `ProcessStep` | [chat-process-streaming.md](chat-process-streaming.md) |
> | 目录、Service、invoke 边界 | [architecture.md](architecture.md) |
> | DSH 作为第八家 Agent 的接入 | [deepseek-harness-integration.md](deepseek-harness-integration.md) |
> | 登录接法 / 本机路由 | [product-decisions.md](product-decisions.md) |
>
> 冲突时以上表为准，不以本文覆盖产品决策。  
> **范围外（本文亦不推导任务）**：凭据落盘加密（无必要）；国产 OAuth 开边 / OAuth→API（产品不做）。

对照对象：本仓库 Chat，与参考仓库 `deepseek-harness-desktop`（当时本机路径 `D:\demo_github\AgentHub_Ref\deepseek-harness-desktop`，上游 pin `deepseek-ai/deepseek-harness@141eb6fe` / `0.1.0-rc.8`）。当时 `deepseek-harness/` 子模块未检出；Harness Loop / Web Client 机制以该 commit 的官方文档、桌面壳源码和 `@deepseek-ai/dsh-*` 类型合同为准。

---

## 0. 先分清产品

两者处理的不是同一类「Agent」。对照时先认清这个差，否则会把 DSH 的 Loop 抄进 Hub，或把 Hub 的「拼 prompt 跑 CLI」误当成 DSH 缺能力。

| | AgentHub Chat | DSH Desktop |
|---|---|---|
| 产品 | 八家 CLI 的连接、登录、技能、用量工作台 | 把官方 DeepSeek Harness 打成可安装的 Electron 应用 |
| Agent 是什么 | 磁盘上的二进制 + `AgentAdapter::build_run_spec` | `ctx.agents` 里的活对象，`id === SessionId` |
| 谁跑 Loop | **没有 Loop**。多步工具发生在 CLI 内部 | `@deepseek-ai/dsh-agent-loop`（包内 `ReactLoopAgent`） |
| UI 拥有什么 | 自己的 SQLite 会话 + 气泡终稿 | 不另建聊天库；官方 profile 共用 DSH home |
| 桌面壳 | Tauri：业务在 `agenthub-core`，GUI 薄命令 | Electron：窗口 / 托盘 / profile / pnpm；**不把 Electron API 给页面** |

AgentHub 对 DSH 的接入是 headless 外包，不是把 Harness 嵌进来。`DshAdapter::build_run_spec` 发的是 `dsh --profile headless "<prompt>"`，不发明 always-approve flag。`StructuredStream` / `SessionResume` 对 `dsh` 仍是 Planned（见接入方案与 `capability()`）。

同一份 Harness：在 Desktop 里是一等运行时，在 Hub 里只是第八个 CLI。

现行 Chat **一会话一 Agent**（core `require_single_agent` + UI `selectConversationAgent` 单选）。文档 / 对照条里的「多 Agent 同 turn」是历史表面残留，对比时以代码为准。

---

## 1. 用户点发送之后

### 1.1 AgentHub：一次 invoke，绑死一轮子进程

```
ChatComposer.onSend
  → useChatPage.sendPrompt            // 乐观插入 user 气泡
  → lib/api/chat.ts chatSend
  → lib/backend/tauri/chat.ts         // 唯一 invoke：Channel<ChatEvent>
  → src-tauri/src/commands/chat.rs    // spawn_blocking
  → ChatService::send_inner
       · single-flight CancelToken
       · insert_turn_messages（user + running 占位）
       · print+resume 或 build_agent_prompt 拼 Hub 历史
       · RunService::run_each
            Adapter.build_run_spec
            StreamingProcessRunner
            StreamSession：NDJSON → ProcessStep + assistant 正文
  → ChatEvent 经 Channel 回 applyEvent / reduceProcessEvent
  → 结束后 listChatMessages 全量对账
```

| 层 | 路径 | 职责 |
|---|---|---|
| 页编排 | `src/pages/chat/index.tsx` | 只编排 |
| 页 hook | `src/pages/chat/use-chat-page.ts` | 会话 CRUD、发送、事件折叠、连接切换 |
| 纯函数 | `src/pages/chat/chat-model.ts`、`src/lib/chat-process.ts` | blockers、picker、`reduceProcessEvent` |
| façade | `src/lib/api/chat.ts` | 页面唯一应走的 API |
| Tauri port | `src/lib/backend/tauri/chat.ts` | `Channel` + `invoke('chat_send')` |
| 命令 | `src-tauri/src/commands/chat.rs` | 薄包装 |
| 会话 | `crates/agenthub-core/src/services/chat_service.rs` | turn、cancel map、prompt stitch |
| 运行 | `crates/agenthub-core/src/services/run_service.rs` | `run_each` + `StreamSession` |
| 解析 | `crates/agenthub-core/src/utils/stream_parse/` | Claude / Codex / Kimi / Grok / Pi |

连接 picker **不是**模型选择器：选项来自 `listAccounts` + `listProviders`。切账号 / provider 写的是该 Agent 的 live 登录；下一轮 `build_run_spec` 才吃到。`/routes` 本机转发不在 `chat_send` 路径上。

### 1.2 DSH Desktop：RPC 叫醒 inbox，UI 订阅读日志

桌面**不自建聊天 runtime**。兼容模式加载官方 `dsh-web-app`；高级模式只替换 `root` 槽（`AdvancedFrame`），`sidebar` / `conversation` 仍是上游。

```
InputBar → ConversationController.send
  → ISession.prompt('queue' | 'steer')
  → HTTP POST /api/session.prompt      // 不是 Electron IPC
  → Agent.followup() 或 Agent.steer()
AgentLoop
  turn/start
    claim inbox
    agent/pre-step（可改写 / 拒绝）
    deriveMessages(session log)
    llm.stream → assistant/chunk*
    tool/call → tools/pre-execute → execute → post-execute → tool/result
  turn/end
Mux WebSocket session/event
  → SessionRuntime → ConversationNodeAssembler
  → ConversationSnapshot → keyed Chat 节点
```

发送语义拆成三种，不是「再发一条」：

| 语义 | 作用 |
|---|---|
| `followup` / `queue` | 下一 turn，FIFO |
| `steer` | 插进当前 turn 的下一 step |
| `inject` | 进 inbox，不单独 wakeup |

Slash command（`/`）走 `session.command`，不进模型。

传输是 Host loopback：HTTP 一元 RPC + WebSocket mux。Renderer 不直连模型 SSE，也不走 Electron IPC 调 Agent。

---

## 2. 机制对照

### 2.1 真相源：消息表 vs 事件日志

**AgentHub** 持久化的是终稿：`chat_messages.content`。过程（命令、stderr、thinking、tool）只在内存 `processMap`，key 是 `` `${turn}:${agent}` ``。切会话清空；刷新不能回放工具时间线。这是 [chat-process-streaming.md](chat-process-streaming.md) 的现行契约，不是疏漏清单。

**DSH** 的不变量是：**模型看见的，必须能从 log 重建**。`deriveMessages()` 从 append-only `SessionEvent` 投影历史，不另存一份 message 表。词汇包括：

`turn/start|end` · `step/start|end` · `user/message` · `assistant/chunk` · `assistant/message` · `tool/call` · `tool/result` · `todo/write` · `session/end-seed`

`tool/call` **在 `tools/pre-execute` 之前**落日志，所以 UI 能立刻画 pending 卡片，崩溃也能区分「没开始 / 开始但结果未知」。它 **不是** Surface 事件，**不进** `deriveMessages()`。模型看见的是后来的 `tool/result`（投影成 user-role）。`assistant/chunk` 同样不进模型表面（只保回放）。空 content 的 `assistant/message` 也不进派生历史，但仍记下 usage。

审批审计（`approval/asked|decided`）与 `approval/requested` 帧故意不进 session log：前者是 log-only 插件事件，后者是带 `rpcId` 的 server-request，实例化前活在 `pendingBuffers`。

这是两边最大的设计差：Hub 把 CLI 当黑盒，只捞可见文本；Harness 把每一步当成可回放事实。

### 2.2 UI 状态：页内 hook vs React-free 对象层

AgentHub Chat **没有全局 store**。`use-chat-page.ts` 持有 conversations / messages / sending / processMap。离开 `/chat` 就丢订阅，后端子进程仍可能在跑，回来靠 refetch。这和「Channel 绑在一次 `chat_send` 生命周期」是配套的。流式事件不是 Tauri `listen`。

DSH 浏览器是第二棵 Cordis 树：

1. **对象层**（零 React）：`ConnectionController` → `SessionManager` → `Session` 吃全部 mux 帧；`ConversationSnapshot` 是不可变快照，未变的 Node 保持引用。
2. **渲染层**：`useSyncExternalStore` / `useSession(selector)` 投影。
3. **槽**：`ui-renderer` 只渲 `'root'`。Desktop 高级模式只占 `root`。

流式 UI 的核心经验：token 不能震整棵 React 树；业务窗口、重连、pending 审批必须在对象层。AgentHub 的 `streamingRef` + `reduceProcessEvent` 已经是这个方向的雏形，但还绑在页面实例上。

Chat 节点用 `ConversationNodeDefinition` 装配：每个业务节点必须有**稳定 id**（禁止「最新未完成」）。重复 start 同一 id 会把 replay 抛死——上游已知坑。

### 2.3 工具：解析展示 vs 带审批的执行管道

AgentHub：

- Claude / Codex / Kimi / Grok / Pi：各家 NDJSON → 统一 `ProcessStep`
- WorkBuddy / Cursor：纯 text
- `ChatProcessPanel` 渲染 thinking / tool / stderr
- **没有** tool 审批 RPC；`allowDangerous` 只是给 CLI 加 skip-permissions 一类 flag（Kimi / DSH headless 无对应 flag）
- 过程不落库

DSH 工具是带策略的管道：

```
tool/call 落日志 → presentCall(args)
  → tools/pre-execute（hook / permission / sandbox）
      ask → ctx.approval（缺 answerer = deny，fail-closed）
  → monotonic guards
  → tools/execute（timeout / 并行）
  → tools/post-execute
  → presentResult(args, result)
  → tool/result 落日志
```

`presentCall` / `presentResult` 是**纯函数**，live 流和 log replay 共用，视图意图不进日志。

审批走 **server-request**：`approval/requested` 带稳定 `rpcId`，重连重放同一 id；UI 的 `PendingWait.respond()` 把 rpc 细节藏起来。缺审批器 fail-closed，不默许。

### 2.4 取消、并发、离开页面

| | AgentHub | DSH |
|---|---|---|
| 取消 | `CancelToken` 杀进程树，best-effort | `Agent.cancel(cause, { keepInbox })`，first-cause-wins |
| 队列 | 页级全局一 turn；后端按 conversation single-flight | 取消当前 turn，FIFO 队列 idle 后续跑 |
| 观察完成 | `chat_send` Promise settle | `whenIdle()` 看整个 Agent |
| 离开 Chat 页 | Channel 回调丢了；过程 UI 没了 | Session 对象常驻，切走再回来立刻渲 |

Hub 的「一页同时只跑一轮」（`sendBlockers` 的 `sendingElsewhere`）更简单，也更符合工作台。DSH 的 inbox 是给真正的多步 Agent 用的。

删除进行中的会话：Hub 在 UI 与 core 都会先 `cancel`。

### 2.5 历史从哪来

AgentHub：

- 默认 `build_agent_prompt`：只拼 **user + 该 Agent 的 ok 回复**，有字数上限
- Claude / Codex：抓到 `native_session_id` 后后续轮只发本轮用户文本（print+resume）
- cwd / agent 中途变更则丢弃 native session；resume 硬失败同样清掉

这能隔离历史串台，但 **Hub 拼出来的上下文 ≠ CLI 自己看见的上下文**（skill、MCP、系统提示、压缩摘要都不在 Hub 库里）。DSH 用「模型可见 = 已记录」把这个问题消掉。

### 2.6 扩展点

AgentHub 加一家 Agent：实现 `AgentAdapter` + 可选 `StreamParser`。产品主轴是登录 / 绑定 / 本机路由，Chat 只是其中一个面。

DSH 加能力不改 Loop：

| 目标 | 机制 |
|---|---|
| 模型 | `ctx.llm` |
| 工具 | `ctx.tools`（schema 自动进 prompt） |
| Chat 新节点 | `ConversationNodeDefinition` |
| 持久状态 | 扩展 `SessionEventMap` |
| 拦截 turn / tool | `agent/*`、`tools/*` waterfall |

Desktop 自己只公开 `desktopProfiles` / `desktopPnpm`。第三方拿不到 `BrowserWindow`。这和 AgentHub「只有 `lib/backend/tauri/` 能 `invoke`、生产 build 禁止打进 mock」是同一类边界意识。

### 2.7 流式合并规则

桌面回归 `dsh-plugin-desktop/tests/deepseek-streaming-tool-call.spec.ts`：后续 delta 里空 `id` / `name` 必须粘住第一次非空。Hub 的 `ChatStreamToIr::push_tool_delta` 与 `mergeThinkingText`（Codex 快照 vs Claude 增量）属于同一层——**流式合并要写成纯函数并单测**，不要散落在 UI。

### 2.8 心智图

```
DSH:  prompt → approval → sandbox → session events → UI 投影
      意图      可不可以    实际够得着   发生了什么      怎么画

Hub:  prompt → (可选 skip-permissions) → CLI 子进程 → 解析 stdout → 气泡 + 过程面板
      意图      粗粒度危险开关            黑盒 Loop      尽力还原      当轮内存
```

DSH 把这四层拆开，所以插件能在不改 Loop 的情况下换模型、换沙箱、换卡片。Hub 把 Loop 留在各家 CLI 里，所以必须做好：适配器边界、过程归一、取消、连接真源、以及承认自己看不见 CLI 内部。

---

## 3. 两边已经各自做对的

**AgentHub 不必自卑的部分：**

- 页面不 `invoke`；生产 build 扫到 mock 直接失败
- `chat-model.ts` / `chat-process.ts` 纯函数 + vitest，和 DSH 的 presenter / assembler 同族
- `sendBlockers` 把「隐藏 / 未登录 / 环境未就绪 / 别的会话在跑」做成 composer 内嵌引导，而不是点了再 toast
- 结构化模式下 **强制** 用 decoder 的 assistant 文本替换 NDJSON，气泡不喷协议垃圾
- cancel 在 core 删除会话时也会挂钩
- 对 DSH 不发明 always-approve（fail-closed）
- Channel 比「全局 listen 所有会话事件」更贴一轮 send 的寿命

**DSH Desktop 作为壳的克制：**

- 不重写官方 Chat
- 不另造 Desktop 数据库
- 兼容模式不覆盖 conversation 槽
- profile / 模式切换 = dispose 整代 generation，禁止跨代缓存窗口 / service
- 工具卡片视图不进 log，避免 UI 和模型 transcript 绑死

---

## 4. 可学习经验（观察，不是派工）

下列是对照观察。落实与否由既有契约与产品决策决定；**本文不授权开工**。过程落库、Pi rpc 审批等协议侧未做项仍只列在 [chat-process-streaming.md](chat-process-streaming.md)。

### 4.1 不改产品模型也能对齐的经验

1. **过程事件至少能回放当轮。** `ProcessStep` 已有 wire 类型；切会话 / 刷新丢掉 thinking / tool 是与 DSH 体验差最大的一块。若做，按过程文档的协议侧走，不另开产品。
2. **节点身份用稳定 id，不要「最新 running」。** 工具卡用 `turn + agent + toolId`。`mergeToolStep` 已按 id merge，保持这条。
3. **presenter 与 reducer 继续纯函数。** 新 Agent 的 parser 只产出 `ProcessStep`；UI 不要再 `switch(agentId)` 画卡片。
4. **Chat 状态可以从页面实例里拔出来。** 不必上 Cordis。一个「当前 in-flight turn」的模块级投影（messages + processMap + sendingConversationId）就能让离开 `/chat` 再回来不丢过程。这接近 DSH「Session 常驻、React 只投影」。
5. **连接切换要有事件。** [architecture.md](architecture.md) 写了 `provider-switched`，实现仍靠 Chat 自己 refetch。Hub 至少可让 Dashboard / Connections / Chat 订同一条「live 登录变了」。
6. **流式工具 id / name 的粘连规则**写成跨 Agent 的共享测试，不要每家 parser 各写一遍。

### 4.2 要动会话模型才谈得上的经验

7. **「模型可见 = 已记录」只适用于 Hub 自己拼的那条历史。** print+resume 的 Agent 不要再用 `build_agent_prompt` 假装 Hub 库等于 CLI 上下文。UI 可标明：这轮走官方 session，还是走 Hub 拼接。
8. **若要支持「生成中再说话」，先引入 queue vs steer，不要偷偷并行第二个 `chat_send`。** 现在用 `sendingElsewhere` 硬拦是对的。
9. **HITL 不要从 CLI stderr 猜。** 若做审批，只对有稳定 RPC 的 Agent；没有契约的 CLI 继续 `allowDangerous` 或只读展示。现行非目标（交互式 tool 审批）不变。
10. **工具结果的 `meta` 必须 JSON 可序列化。** 若开始落过程，不要把 React 节点、循环引用塞进 DB。

### 4.3 不要照搬

- **不要在 `agenthub-core` 里实现通用 Agent Loop。** Hub 的价值是连接八家 CLI 和登录图，不是再造一个 Harness。DSH 在 Hub 里继续是 `dsh --profile headless`。
- **不要把 Chat 改成 Cordis 插件树。** `#backend` adapter 已经给了测试 / 生产边界。
- **不要为了「像 Cursor」去做交互式 tool 审批**，除非某家 CLI 有稳定契约。
- **不要另造一套 Desktop 聊天库去镜像 CLI 的原生 session。** 对 Claude / Codex 用 `native_session_id` 才是正确粒度。
- **国产 OAuth / 凭据落盘加密**：项目范围外。对照 DSH 的 credentials / 审批 seam 也不推导实施任务。

---

## 5. 若只记五条

1. **UI 订阅真相，不拥有真相。** DSH 订 session log；Hub 应让 SQLite 终稿（及若落地的过程事件）成为真源，React 只投影。
2. **工具调用先记录再执行。** 才能画 pending 卡、才能 replay。
3. **视图意图是纯函数，不进日志。** `presentCall` / `reduceProcessEvent` 同一原则。
4. **壳不要重写运行时。** Desktop 不重写 Chat；Hub 不该把 DSH 嵌成 in-process Loop。
5. **扩展走登记，不走巨型 switch。** Adapter / StreamParser / ConversationNodeDefinition 都是这个。

AgentHub Chat 作为「多 CLI 工作台里的一条会话」已经分层清楚。从 DSH 最值得搬的不是插件宇宙，而是：**事件源、稳定节点 id、过程可回放、对象层与 React 分离**。这些能在不改变「一会话一 CLI」产品模型的前提下，把 Chat 从「跑命令看 stdout」抬到「可回放的一轮工作」——是否抬、何时抬，由过程契约与产品方案决定，不由本文派工。

---

## 6. 逐条深挖

读法：每条先钉两边事实，再分清 **结构差**（产品模型决定，抄了会错）和 **实现差**（同一模型下可以学）。仍不派工。

对照时不要把「消息」当成一种东西。DSH 实际是四层，Hub 把其中几层压扁了：

| 层 | DSH | Hub |
|---|---|---|
| 耐久日志 | `SessionEvent[]`，`seq === index`，单 writer | SQLite `chat_messages` 终稿行 |
| 模型表面 | 仅 `user/message` · `assistant/message` · `tool/result`（`deriveMessages`） | `build_agent_prompt` 的拼接文本，或 CLI 自己的 session |
| 人读 transcript | append-origin 事件；compaction 的 `replace` 改模型表面但不改 raw log | 气泡 `content` + 当轮内存过程条 |
| 活控制面 | inbox / `session/queue` / `approval/requested`（不进 log） | 页级 `sending` + `CancelToken` map |

### 6.1 产品与所有权：工作台包 CLI，还是壳托管 Loop

**Hub**

Chat 是 Workspace 里的一个面，和 Connections / Skills / Routes 并列。`AgentAdapter` 的工作是 detect、读写 live 配置、`build_run_spec`。Chat 并不拥有各家的 Agent Loop。对 DSH 的诚实边界写在 `DshAdapter`：`dsh --profile headless "<prompt>"`，`StructuredStream` / `SessionResume` 为 Planned，不发明 danger flag。

**DSH Desktop**

Desktop 的产品声明是：不重写 Harness，不另造聊天库，官方 profile 共用 DSH home。Electron 只做 generation：窗口、托盘、profile、pnpm。高级模式 `applyAdvancedShell` 只 `slots.register('root')`，子槽 `sidebar` / `conversation` / `details` 仍由上游插件填充。兼容模式连 root 都不占。

**对照**

「UI 与 Agent」在两边指的不是同一对主语。Hub 的 UI 驱动的是 **子进程规格**；Desktop 的 UI 订阅的是 **已经在跑的 Agent**。同一份 `dsh` 二进制：Desktop 把它当 Host 嵌进 loopback Web；Hub 把它当第八家 CLI 拉起来。这不是完成度差距，是所有权差距。

| | 结构差 | 实现差 |
|---|---|---|
| 不把 DSH 嵌成 in-process Loop | 结构：Hub 要同时面对八家 CLI | — |
| 壳不暴露原生 API 给页面 | — | 同类边界：Hub 已用「仅 `lib/backend/tauri/` 可 invoke」 |

**误抄**：把 Cordis / slot / profile generation 搬进 AgentHub，等于换产品。

---

### 6.2 发送单元：一次 invoke 的寿命 vs inbox 叫醒

**Hub**

`createTauriChatPort().chatSend` 新建 `Channel<ChatEvent>`，`invoke('chat_send', { onEvent: ch })` 直到 `ChatService::send` 返回才 settle。发送单元 = **这一轮子进程**。`send_inner` 在分配 turn 之前把 `CancelToken` 插入 `active` map：同会话已有 in-flight 则 `InvalidArg`，避免半插入消息。占位行与 user 行在同一 `BEGIN IMMEDIATE` 事务里由 `insert_turn_messages` 分配单调 `MAX(turn)+1`。

前端 `sendPrompt` 先用 `turnGuess = max(turn)+1` 插本地 user 气泡；DB 分配的 turn 可能与 guess 短暂不一致，靠 `agentFinished` 换真实 `ChatMessage`、最后 `listChatMessages` 对账。

**DSH**

`ISession.prompt('queue'|'steer')` 是 HTTP RPC：把消息放进 inbox 并 wakeup。RPC 返回不表示 turn 结束。真正的工作在 `AgentLoop`：`followup` 开新 turn，`steer` 插下一 step，`inject` 不 wakeup，`/` 走 `session.command` 不进模型。SDK 若要可回放 transcript 应订 `session/event`；`agent/*` 只是活的协调面（队列、拦截、状态）。

**对照**

Hub 把「一次用户点击」和「一次进程寿命」焊死，取消、超时、Channel 卸载都自然对齐。DSH 把「一次用户点击」变成 inbox 上的一条事实，Loop 可以跨多个 step、在工具后继续、在 steer 后改道。

Hub 没有 `steer` 不是漏做：没有活 Agent 对象，就没有「当前 step 的下一拍」。`sendingElsewhere` 硬拦「生成中再说话」，是在这个模型里唯一安全的选择。若放开并行第二个 `chat_send`，两轮 prompt 会打进同一 CLI 或抢 `active` lock。

**结构差**：发送单元。  
**实现差**：乐观气泡的 `turnGuess` 与 DB turn 的窗口——DSH 用 log seq，根本没有这层对账。

---

### 6.3 真相源与崩溃：终稿表 vs 可修补的事件日志

**Hub**

真源是 SQLite：`conversations` + `chat_messages`（`0002_chat.sql`），后来加 `native_session_id`（`00015_chat_native_session.sql`）。消息列只有 `content` / `status` / `exit_code` / `duration_ms` / `error`。**没有**过程表、没有 seq、没有 `tool/call`。

`native_session_id` 不是 Hub 的日志，是指向 Claude/Codex **别人家日志** 的指针。切 agent 或 cwd 立刻清掉（`update_conversation`）；resume 硬失败（`Failed`/`Timeout`）也清；**用户取消不清**（`Cancelled` 不是 `is_hard_failure`）。send 过程中 cwd/agent 被改，persist 会丢弃新 sid，避免把 A 目录的 session 接到 B。

过程 UI 的 `ProcessMap` 纯内存。切 `activeId` 的 effect 里 `setProcessMap({})`。刷新 / 杀进程后只剩终稿气泡。

崩溃窗口：占位行以 `status=running` 写入后，若 GUI 被杀，`CancelToken`（`Arc<AtomicBool>`）与子进程监督同死。没有 cold repair 把 running 收成 interrupted。再次打开 Chat，`listChatMessages` 会把 running 行渲染成永久转圈（`ChatMessageBubble` 见 `status === 'running'` 就转 Loader），也没有 Stop——因为页级 `sending` 是 false。

DSH 崩溃语义更细：只有 assistant 请求、没有耐久 `tool/call` → `TOOL_NOT_STARTED`；有 `tool/call` 无 `tool/result` → `TOOL_OUTCOME_UNKNOWN`（禁止盲着重试副作用）。Hub 只有一种悬挂：`status=running` 的气泡，无法区分 CLI 是否已经动过文件系统。

**DSH**

`Session` 是 typed `SessionEvent` 的 append-only 日志。`deriveMessages()` 投影模型历史；`assistant/chunk` 保留 token 级回放。持久化是 **seam**（`ctx.sessionPersistence`），不是第二种事件类型：JSONL 与 SQLite 后端共用同一词汇。

崩溃：cold load 若见 `turn/start` 无 `turn/end`，**不截断**（长任务的 tool 输出已经落盘），而是补一条合成 `turn/end { reason: interrupted }`。`interrupted` 是唯一 Loop 自己不 emit 的结束原因。live id 的 `load` 拒绝给未闭合 turn 做合成修补，避免双主。`SessionHeader`（cwd、lineage、seed 边界）走日志旁边的元数据，不进 `SessionEventMap`，因此也不进 `deriveMessages()`。

队列 `session/queue`、jobs、审批卡片是 process-local 投影，重连靠 mux 快照，不进 durable log——因为它们不是模型可见事实。

**对照**

| 问题 | Hub | DSH |
|---|---|---|
| 刷新后工具卡还在吗 | 不在 | 在（log replay + `presentResult`） |
| 崩溃后 turn 平衡吗 | running 行可永久悬挂 | cold repair 合成 interrupted |
| 模型下一轮看见什么 | Hub 拼的文本，或 CLI 自己的 session | 只能是 log 能重建的 |
| 指针 vs 日志 | `native_session_id` 是外键到 CLI | 没有外键，log 就是历史 |

Hub 不落过程是契约（过程文档 Phase 3 协议侧未做），不是 parser 没写完。若将来落过程，DSH 的教训是：**不要另造平行事件类型**；同一 `ProcessStep` 词汇写入、投影、replay。崩溃修补要区分 cold（可合成结束）和 live（不可抢）。

**误抄**：把 Hub 的 `chat_messages.content` 扩成「什么都塞进正文」。DSH 把 usage 附在 `assistant/message` 上、把 todo 做成 last-wins 快照、把审批故意留在 log-only，正是为了不污染 `deriveMessages()`。

---

### 6.4 UI 投影：页内 hook vs 常驻对象层

**Hub**

没有 Chat store。`useChatPage` 持有 conversations / messages / sending / processMap。流不是 `listen`：Channel 回调进 `applyEvent`。

`applyEvent` 不变量：

- `activeIdRef !== sendConvId` → 丢掉除 `error` toast 外的所有事件。**过程与 chunk 不缓冲。**
- 同会话：`reduceProcessEvent` 与 messages 并行。
- `started` 补 running 占位；`agentChunk` stdout 写 `streamingRef` 再刷气泡；`agentFinished` 删 local/running 换后端行。
- `text` 型 `ProcessStep` 不进时间线（已在气泡）。

离开 `/chat`：hook 卸载，Channel 的 `setState` 失去合法订阅者；后端子进程仍跑，回来只靠 refetch 终稿。切会话清空 `processMap`，即使 send 还在原会话上跑——切回去也接不住已经过去的 step。

滚动：距底 ≤80px 才跟随，避免流式打断回看。这是对象层不存在时，UI 自己做的一点「投影纪律」。

**DSH**

浏览器第二棵 Cordis 树。对象层零 React：`Session` 常驻，吃 mux 帧；`session/event` 按 **seq 去重**（唯一去重键）；`open` 进行中的帧进 buffer，open 结束后按 seq stitch。重连 = 清窗口 + 再 `open`（拉尾页历史）。`ConversationSnapshot` 要求未变 Node 保持引用，否则 uSES / memo 失效。

`ConversationNodeDefinition`：`(kind, id)` 至多一次 start；增量必须自带稳定业务 id，禁止「最新未完成」。history 窗口若只有 update 没有 start，assembler 挂起 Context，不猜。Desktop `AdvancedFrame` 用 `useSyncExternalStore` 订 layout，用 `useSessions` 决定 details 列——frame 自己不碰 transcript。

**对照**

Hub 的 Channel 对「这一枪」更干净：没有全局事件风暴，send 结束 Channel 就结束。DSH 的常驻 Session 对「人还在这个产品里走动」更干净：切会话、刷新、审批重连都是同一套 seq 窗口。

Hub 已经做对的投影纪律：`reduceProcessEvent` 未知事件返回同一引用（React bail-out）；thinking/tool merge 纯函数且有单测。缺的是 **窗口**：没有 seq，没有 open-buffer，没有「切走仍把帧接到那个 conversation 的投影上」。

另外两条竞态（实现差）：

1. **切走再切回**：`applyEvent` 把中间 chunk 全丢；`loadMessages` 可能在后续 chunk 之后 resolve，用 DB 里仍为空的 `running` 行盖掉刚恢复的流式正文，直到下一 chunk 或结束 refetch。
2. **`Finished.ok` 与取消**：`RunStatus::is_hard_failure` 只有 Failed/Timeout，取消时报告 `ok=true`。若过程项仍停在 running，`reduceProcessEvent('finished')` 会按 ok 标成成功。mock 取消则 `ok=false`，生产/mock 不一致。
3. **Channel `send` 失败静默**（`let _ = on_event.send(ev)`）：前端挂了后端仍跑完并落库，UI 只能靠事后 list 看见终稿。

DSH reconnect = **rebuild**（清窗再 `open` 尾页），没有 resume cursor；mux `since` 是预留座位。审批/提问帧从不进 history，靠 `pendingBuffers` 在 Session 实例化时 replay。Hub 没有对等缓冲，切走等于丢控制面。

把 Chat 状态拔到模块级 store，不必抄 Cordis。最小形状就是 DSH Session 的缩影：`byConversationId: { messages, processMap, sending }`，页面只 `useSyncExternalStore`。Channel 仍可当传输，只是 onEvent 写入模块而不是写入即将卸载的 hook。

**结构差**：没有活 Agent，就没有跨页面的「当前 turn」对象——但 **可以** 有跨页面的「当前 in-flight 子进程投影」。那是实现差。

---

### 6.5 工具：stdout 考古 vs 先记录再执行

**Hub**

工具不是一等实体。CLI 在子进程里自己做完 Loop；Hub 用 `StreamSession` 按 `\n` 切行，registry 里的 parser 吐 `ProcessStep`。stderr 永不进气泡。结构化模式 **强制** `apply_structured_stdout` 用 decoder 的 assistant 文本覆盖 runner 抓到的 NDJSON——哪怕截断、哪怕只有 tool 事件。

降级：

- 无 parser → 整段 text chunk
- JSON 但未知 shape → `ProcessStep::Raw`，不进正文
- 非 JSON 行在 structured 模式 → 既进 assistant 文本，又记 raw（兼容）
- 单行 >256KiB / 行缓冲溢出 → raw 并丢弃残行
- 步骤 cap：core `MAX_EMITTED_STEPS=2000`（Error/Tool 可突破），UI reducer `MAX_STEPS=200` 滑窗。前后端 cap 不一致：长 turn 的早期 tool 卡会被 UI 丢掉，即便 core 发过。

`ChatProcessPanel`：tool 行展示 name/status/input/result；result 用 `DiffAwarePre` 做 `+/-/@@` 着色。截断（input 短、result 4000 字 / 200 行）。**不能**批准、不能重跑单步、不能点开文件。

无 id 的 tool 步骤不 merge（`pushStep` 只在 `step.id` 为真时找旧卡）。无名增量依赖 parser 先给出稳定 id。

**DSH**

时序是不变量：assistant 消息里出现 tool-call block → **先** `session.append('tool/call')`（原始 arguments 字符串，未 parse；**不是** Surface，不进模型）→ UI `presentCall` → `tools/pre-execute`（**禁止**改 `exec.arguments`，否则 logged args ≠ 实跑）→ 才 `execute`。denied / 审批拒绝仍走 post-execute，再写 `tool/result`（这才进 `deriveMessages`）。卡片用 `presentCall(args)` / `presentResult(args, result)`，纯函数，live 与 replay 共用。canonical `value` 只活在执行期；replay 权威是渲染后的 content。`meta` 必须是 JSON；`Session.append` 运行时校验，非 JSON 在源头拒绝。`schemas()` 白名单只有 name/description/parameters，timeout / 并行 / presenter 永不进模型请求。

并行：`isConcurrencySafe` 默认互斥，只有显式 `true` 才进 rolling pool。Code Mode 的子调用带着 parent token 走同一管道，并记 `tool/code-dispatch`。

**对照**

Hub 看到的「⚙ web_search · end」是事后考古：CLI 已经执行完（或正在执行，Hub 无法拦）。DSH 看到的 pending 卡是 **执行前的法律文件**：`tool/call` 已在 log 里，审批可以否决，否决结果仍对模型可见。

所以 Hub 的过程面板再精美，也变不成 HITL。缺的不是 UI 组件，是「执行前的记录点」。对各家 CLI，那个记录点在 CLI 进程内部；Hub 除非做官方 RPC，否则不应假装有。过程文档把 Pi rpc 列为非目标，与这条结构差一致。

可学的实现差：

1. 稳定 `tool.id`（已在 merge 路径里，parser 必须给）
2. UI cap 不要 silently 丢掉早期 tool（滑窗 vs 保留 tool/error）
3. presenter 继续纯函数，不要在 `ChatProcessPanel` 里 `switch(agentId)`

---

### 6.6 危险开关 vs 审批缝

**Hub**

`allowDangerous` 是会话布尔。`autoApproveEffect` 把「开了会实际怎样」对齐到各 adapter 的 headless flag：

| Agent | 效果 |
|---|---|
| Claude / Codex / Grok / WorkBuddy / Cursor | `skip`（skip-permissions 一类） |
| Pi | `project-trust` |
| Kimi / DSH / 未知 | `none`（TUI 里或许有 yolo，headless **故意不加**） |

切到 `none` 的 Agent 会把已开的自动批准清掉（`selectConversationAgent`）。二次确认文案按 effect 分。这是 fail-closed：不发明 DSH/Kimi 的 always-approve。

**DSH**

`tools/pre-execute` 可 `allow | deny | ask`。`ask` → `ctx.approval.request`。无 answerer 或不可答 → deny（`unavailable`），不是默许。审批走 server-request，稳定 `rpcId`，重连重放同一 id；UI `PendingWait.respond()` 不暴露 rpc。审计事件 `approval/asked|decided` **不是** SurfaceEvent，不进模型 transcript。权限预设把 sandbox + approval 捆在一起（如 `workspace-write` / `danger-full-access`），是 log fold，resume/fork 自动恢复。

**对照**

Hub 的危险是 **进程启动参数**；DSH 的审批是 **每一次 tool 的瀑布**。前者粒度是整轮 CLI，后者粒度是单 call。把 DSH 的审批 UI 画在 Hub 过程面板上，没有对应的 `ask` 停顿点，按钮是假的。

Hub 已经从 DSH 学到的那一半：Kimi/DSH headless 不开 yolo。缺的那一半（按 call 审批）不是 Chat 页能做的，除非某家 CLI 提供稳定 RPC。

**误抄**：从 stderr 里 grep `Allow?` 然后弹窗。没有 rpcId 就无法重连、无法幂等、无法 fail-closed。

---

### 6.7 取消、队列、并发粒度

**Hub**

两把锁，松紧相反：

| 锁 | 粒度 | 效果 |
|---|---|---|
| `ChatService.active` | per conversation | 同会话不能第二发 |
| 页级 `sending` + `sendingElsewhere` | 整个 Chat 页 | 别的会话 composer 也停 |

取消：`chat_cancel` 不走 blocking pool，只把 `AtomicBool` 置位。`run_spec_streaming` 每 50ms poll，true 则 `kill_process_tree` + wait，状态 `Cancelled`。UI toast「已请求取消」，终态等进程死。删除会话 core 也会 `cancel`。

取消 **不是** hard failure：resume sid 保留。这是对的——用户 Stop 不等于官方 session 坏了。Timeout/Failed 才清 sid。

没有 inbox。没有 `whenIdle()`。没有「取消当前 turn、队列接着跑」。超时默认 300s（`DEFAULT_RUN_TIMEOUT`）。输出 cap 2MiB。

刷新或离开 `/chat`：页级 `sending` 丢失。后端可能仍 inflight。新挂载既看不到 `sendingElsewhere`，也没有取消按钮。再点发送会打到 `conversation already has an in-flight send`。`messagesLoading` 期间仍可发送，`turnGuess` 在 `messages=[]` 时为 1，与 DB 的 `MAX+1` 更容易分叉。

**DSH**

`Agent.cancel(cause, { keepInbox })`：当前活动 first-cause-wins；无活动则 no-op，不预武装未来工作。`keepInbox` 为 false 时清 queued+steering。Host `session.cancel` **保留 inbox**（等价 keepInbox）：只停当前 turn，flush 后 FIFO claim 下一条 waking 消息。`whenIdle()` 等到 driver 与 maintenance 都静下来，不标识某一条消息。

发送原语是 2×2，不是一条 `sendMessage`：

| | wakeup | 安静 |
|---|---|---|
| `next-turn` | `followup` | 排队不开车 |
| `next-step` | `steer` | `inject`（idle 一直 pending） |

`inject` 进 inbox，只有 `agent/pre-step` 把它放进 entering batch 才变成 `user/message`。已 claim 的 batch 若被 abort **不回队**。blank session 才能换 agent preset；跑过 turn 后 `agent-preset-locked`。

子 agent：唯一续写 `ctx.subagents.followup`。inbox 接受后，父 cancel / caller signal **不得**取消已接受消息或 dispose Activation。公开停法 `interrupt` = `cancel({ keepInbox: true })`，不级联子孙。teardown child-first。

**对照**

Hub 的页级单飞行比后端更严，是产品选择：工作台同时盯两轮 CLI 容易把「当前登录 / cwd」弄乱。DSH 的 inbox 服务的是同一 Agent 的多条意图。

Hub 取消是杀树，DSH 取消是 abort signal 贯穿 prompt/工具/stream。杀树更硬、更脏（可能截在半文件）；signal 更合作、杀不死拒不观察 signal 的 in-process 代码。Hub 面对的是外部 CLI，杀树是匹配的。

实现差：Channel 取消与进程取消之间没有「已请求」的过程 phase 事件专门线——现在靠下一次 `agentFinished.status=cancelled`。过程面板能显示 cancelled，是 `phaseFromMessageStatus` 映射出来的，不是独立的 cancel ack。

---

### 6.8 历史：Hub 拼接 vs 日志投影；resume 指针

**Hub**

两条互斥路径，由 `native_session_id` + `supports_print_resume`（仅 Claude/Codex）决定：

1. **无合法 sid**：`build_agent_prompt`。只收 user + **该 agent 且 status=ok** 的回复。失败/取消/timeout 的助手行不入史。按 turn 丢最老，整段 ≤ `CONTEXT_CHAR_LIMIT`（24000 字）。渲染成 `[用户]` / `[助手]` 包在「以下是我们此前的对话记录…」模板里。超限到只剩当前问题。
2. **有合法 sid**：本轮只发 user 原文，CLI `--resume` / `exec resume`。

header 芯片可展示「在官方 CLI 里续」的 argv（`plan_native_resume`）：Kimi/Grok/Pi/Cursor 有 **TUI** resume 计划，但 Chat **print-mode** 不走它们。WorkBuddy/DSH 连 TUI 计划都没有。

cwd 必须是已存在目录才会 **写入**；send 时若 `conv.cwd` 为 Some 也会 `validate_cwd`。UI `sendBlockers` **强制**有 cwd；后端允许 `cwd=None`（测试常走这条）。GUI 与 core 不一致。

resume 硬失败当轮 **不会** 用 stitch 重试；用户看到失败气泡，下一轮才回退拼接。这是「指针失效再补偿」，不是「同一枪换路」。

**DSH**

没有「拼一段给模型的字符串」这条旁路。每一步 `deriveMessages()` 从 log 来。`user/message` 的 `source` 区分真人 prompt、`agent.inject()`、goal 续轮；`form`（instructions/catalog/snapshot/notice/relay/recall）是语义不是皮肤。compaction、skill 内容、子目录 AGENTS.md 都必须先成为 session 事件，才能被下一请求看见。运行时不变量会断言。

resume = `ctx.agents.resume` 加载持久化 session，再跑同一个 Loop。history 分页（`session.history` 尾页）**不** resume Agent，只给 UI 窗口。`session/end-seed` 划分 seed（fork/replay）与本生命周期 live 事件。fork 是 `ctx.sessions.fork`。

**对照**

Hub 的拼接是 **对 CLI 黑盒的补偿**：Kimi/Grok/Pi/DSH 没有 print-resume，就把 Hub 库当成伪 session。补偿有三层失真：

1. 模型实际吃的是带 `[用户]/[助手]` 的中文模板，不是原消息块。
2. 失败轮被丢掉，模型不知道自己刚失败过。
3. skill / MCP / 系统提示 / 压缩摘要在 CLI 里，Hub 库没有，拼出来的「历史」和 CLI 自己若有状态时并不相等。

print-resume 路径反而更诚实：Hub 承认自己不是真相。UI 今天只在 header 用 `nativeResumeCommand` 露出「可在官方 CLI 续」；Chat 气泡不标明「本轮走官方 session」。这是实现差。

**结构差**：Hub 无法对八家 CLI 做 `deriveMessages`，因为没有它们的 log。  
**误抄**：为 DSH 在 Hub 里再投影一份 SessionEvent。Desktop 明确不复制 DSH home；Hub 也不该。

---

### 6.9 流式合并与协议粘连

**Hub**

`mergeThinkingText`：若 `next.startsWith(prev)` 当快照替换（Codex `item.updated`）；若 `prev.startsWith(next)` 当回放忽略；否则拼接（Claude/Grok/Pi 增量）。有单测。

`mergeToolStep`：空名不覆盖已有名；空 result 不覆盖；id 缺则用旧 id。

Bridge 侧 `ChatStreamToIr::push_tool_delta`：按 index 建状态；空 id 用 `call_{index}`；后续 delta 只追加 arguments。这是 **本机路由** 的 Chat Completions→IR，不是 Chat 页 parser，但同一类 bug。

**DSH**

LLM 层 `StreamChunk` 是闭合判别联合：`index` 绑 delta 到 block，`block-end` 携带组装好的 `ContentBlock`，消费者不必自己拼。桌面补丁与回归 `deepseek-streaming-tool-call.spec.ts`：后续空 `id`/`name` 必须粘住首次非空（`if (call.id)` 而不是 `!== undefined`）。

**对照**

两边都在打「流式工具调用的空字符串续包」。DSH 把它放在 **LLM adapter**，一次修好所有 UI；Hub 分散在 `stream_parse/*`、`chat-process.ts`、`bridge/protocol/chat.rs` 三处。这是实现差，也是最便宜的对齐：共享「空 id/name 不是新 call」测试夹具，parser / bridge / reducer 各跑一遍。

cap 纪律也属实现差：core 2000 vs UI 200。DSH 的窗口是按 seq 的 history 页，不是「最后 200 个 step」——早期 `tool/call` 仍能通过分页回来。Hub 滑窗等于主动忘掉本轮开头的工具。

---

### 6.10 扩展点与 fail-closed

**Hub**

加 Agent：`AgentAdapter` + 可选 `StreamParser` 登记进 `builtin_stream_registry`。`StreamSession` **按 AgentKey 查 registry**，不在 feed 路径 `match agent_id`。这是平台能力改造后的纪律。

Chat 页仍有按 Agent 的表：`autoApproveEffect`、`supports_print_resume`、`extract_native_session_id` 的 key 列表。后两处是「官方 CLI 到底支不支持」的事实表，fail-closed（未知 = 不 resume、不抽 sid）。

Backend 选择：Vite alias `#backend`；生产 bundle 扫描 `src/dev` 失败。非 Tauri 调用 `assertTauriRuntime` → unavailable，禁止静默 mock。

**DSH**

「Where new behavior goes」是一张表：模型进 `ctx.llm`，工具进 `ctx.tools`，Chat 节点进 Definition，持久状态进 `SessionEventMap`。改 Loop 必须改 architecture.md。Capability 缺则 `UNSUPPORTED_CAPABILITY` 大声失败，不接受再忽略。Fabric 社区层（Desktop 仓库里仍是 Draft）把这写成五条不变量：插件只依赖稳定 DTO；Adapter 是唯一吸收上游变化的地方；缺能力就关，不返回「看起来成功」。

Desktop 公开面只有 `desktopProfiles` / `desktopPnpm`。`installPlugin` 有 snapshot / receipt / 失败回滚；generation 切换必须 `release()`，禁止跨代缓存 BrowserWindow。

**对照**

两边的优秀经验其实同一句：**扩展走登记，缺了就关。** Hub 对 DSH headless 不发明 flag、对未知 parser 回退 text、对非 Tauri 报 unavailable，已经是这条。DSH 把它推到工具、子 agent、审批、compaction。

Hub 若在 Chat 里 `switch (agentId)` 画特殊气泡，就是在拆这条。新过程 UI 应只认识 `ProcessStep`。

---

### 6.11 多 Agent 与子 Agent

**Hub**

core `require_single_agent`：create/update 拒绝 `len>1`。打开旧多 Agent 行，页 effect 在非 sending 时写成 `[agentIds[0]]`。`run_each` 仍带 Parallel 模式，但是 jobs 长度 1。UI 残留：`turnComparisonChips`、`retryAllHint`、`connectionPickerCaption`「仅作用于首位」。文档 [chat-page-redesign.md](chat-page-redesign.md) 仍写「一个或多个 Agent」——**文档滞后于代码**。

没有子 agent。没有「父 cancel 不得杀掉 child continuation」。一轮就是一棵进程树。

**DSH**

子 agent 是可选 capability，不是 Loop 的一部分。one-shot `start` vs continuable `prepareContinuable`；缺能力的请求拒绝而不是忽略。实验性 Agent Teams 在 continuable 之上加 roster/task/mailbox。UI 子会话 Send-only。

**对照**

Hub 从多 Agent 同 turn 收到单选，是产品收口，不是技术退步：并排两家 CLI 会把「当前登录」语义打穿（连接 picker 只作用于首位，已经暴露了这个问题）。DSH 的多 Agent 是 **委派**（父子 session），不是 Hub 曾经的 **并跑**。不要用 DSH subagent 当理由把 Hub 多 Agent 并行加回来。

---

### 6.12 壳：Tauri 命令 vs Electron generation

**Hub**

`agenthub-core` 无 tauri 类型。GUI 命令：校验 → service → 序列化。Chat 流用 Channel 是例外的「长 RPC」，仍然是这一枪的一部分。目标 sidecar（`agenthub-adapterd`）管的是本机路由 saga，**不是** Chat runtime。Chat 不依赖 sidecar。

**DSH Desktop**

Host 在 `127.0.0.1` 随机端口（可固定）。Renderer 加载同源页面。无 preload 把 Host service 暴露给页面。profile/模式切换 dispose 整代：service 引用、窗口、subprocess handle 都不能跨代。启动失败回 last-known-good profile。

**对照**

两边都拒绝「页面直接操原生」。Hub 的下一层拆分是 adapter sidecar；Desktop 的拆分已经是 Host Cordis vs Electron generation。Chat 若去走 sidecar，会把「一次 CLI 子进程」再加一跳 IPC，没有 DSH 那种「Host 已拥有 Agent」的收益。

---

### 6.13 结构差一览

| 条 | Hub 单元 | DSH 单元 | 结构还是实现 |
|---|---|---|---|
| 6.1 所有权 | 八家 CLI 工作台 | 一个 Harness 的壳 | 结构 |
| 6.2 发送 | invoke 直到进程结束 | inbox 叫醒，RPC 先返回 | 结构 |
| 6.3 真源 | 终稿行 + 可选 sid 指针 | append-only 事件 + 持久化 seam | 结构（落过程则实现） |
| 6.3 崩溃 | running 可悬挂 | cold 合成 interrupted | 实现 |
| 6.4 投影 | 页 hook + Channel | 常驻 Session + seq 窗口 | 实现（可拔 store） |
| 6.5 工具 | 解析 stdout | 先 `tool/call` 再执行 | 结构（对黑盒 CLI） |
| 6.6 审批 | 启动 flag | per-call waterfall + rpcId | 结构 |
| 6.7 取消 | 杀进程树 | AbortSignal + keepInbox | 结构（对象不同） |
| 6.8 历史 | 拼接或 print-resume | deriveMessages | 结构；气泡标明路径是实现 |
| 6.9 流式粘连 | 三处各写 | adapter 一处 + 闭合 StreamChunk | 实现 |
| 6.10 扩展 | Adapter + parser registry | ctx 缝 + fail loud | 同类纪律 |
| 6.11 多者 | 已单选；无 child | 委派子 session | 结构（不要把并跑加回） |
| 6.12 壳 | Tauri 薄命令 | generation + loopback | 同类纪律 |

---

### 6.14 观察收口（仍不派工）

若只从实现差里挑「对照后最值得记住」的，顺序是：

1. **崩溃与悬挂 running**：DSH 证明「未闭合 turn 必须在 cold 路径有终结」，并区分 tool 未开始 / 结果未知。Hub 今日可出现永久转圈气泡，且离开 Chat 页后无法再点取消。
2. **in-flight 投影不要绑在页面实例**：Channel 可保留，订阅者换成按 conversation id 的模块；切走应缓冲而非丢帧。
3. **过程 cap 与稳定 tool id**：UI 200 滑窗会切掉本轮早期工具；无 id 的 tool 不 merge。
4. **三处流式粘连测试夹具**：parser / reducer / bridge。
5. **气泡标明本轮走官方 resume 还是 Hub 拼接**：避免用户以为 Hub 库等于 CLI 上下文。resume 失败当轮不重试 stitch。
6. **取消与 `Finished.ok`**：生产取消报 `ok=true`，过程 reducer 可能把仍 running 的项标成成功；与 mock 不一致。
7. **文档滞后**：Chat 重设计文仍写多 Agent 并行；以 `require_single_agent` 为准。

本地对照时 `deepseek-harness/` 子模块未检出，DSH Loop / Client 的逐行源码以 pin `141eb6fe` 官方文档与类型声明为准，不是本机 `packages/**/*.ts`。若要做字节级 diff，需先检出子模块再读 `packages/core/{session,agent,tools,agent-loop}` 与 `packages/client/runtime/src/client/sessions/`。

这些不改变「一会话一 CLI」模型，也不授权过程落库或 HITL。过程落库、Pi rpc、diff 预览仍只列在 [chat-process-streaming.md](chat-process-streaming.md)。凭据落盘加密、国产 OAuth 仍为范围外。

