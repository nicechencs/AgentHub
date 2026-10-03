---
title: AgentHub 当前实现状态
type: status
status: current
owner: maintainers
updated: 2026-10-03
---

# 当前实现状态

本页只记当前工作区已经实现的事实，不是路线图。发布包可能落后于工作区；判断某个版本时以对应 Release 和源码为准。各页交互细节见 [页面模式](ui/page-patterns.md)，Chat 机制见 [Chat 与 Agent](concepts/chat-and-agents.md)。

## 产品表面

- 桌面端基于 Tauri v2，前端 React，核心业务和 CLI 用 Rust。
- 页面：Dashboard、Agents、Connections、Sub2API、Routes、Skills、MCP、Chat、Projects、Plugins、Settings。
- Settings 有五个页签：**偏好 / 功能 / 本机 / 备份 / 关于**（`?tab=preferences|features|local|backups|about`），是页内胶囊栏，不进侧栏。
- 外观在设置 → 偏好里选 `light` / `dark` / `system` 并保存；浅色页面底只在浅色主题生效。
- CLI 提供 doctor、env、agent、provider、account、skill、usage、backup、run、config 等命令；参数以 CLI 帮助和源码为准。

### 内置 Agent

- 当前内置：Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、ZCode、DeepSeek Harness、Kiro。
- **Cursor Agent** 的代码还在，但 `dev` 通过 store-stamp（`agent_visibility.json`）默认软隐藏：不在侧栏、连接、Chat 等页面出现，可在 Agents 管理页取消隐藏。登录写入、路由目标与结构化输出修好后再开放。
- Kiro 管理 `kiro-cli`（检测、安装、登录指引、API Key）。不把编辑器当成已安装。

### 连接与连接池

产品规则（哪些登录能分享）以 [产品边界](decisions/product-boundaries.md) 为准，这里只记实现。

- Connections 是跨工具的登录列表；接到某个工具从 Dashboard「连接/切换」。
- 登录按登录方式分行保存（官方登录与 API Key 分开）。点名称打开右侧详情：状态、用量、接口 / 模型、当前使用、相关文件、时间；记录编号和导入来源收在折叠的 **更多** 里。
- WorkBuddy 自定义模型、ZCode 供应商按目录拆成多条登录；桌面套餐登录不导入。WorkBuddy 写入只认 `/v1/chat/completions`。
- 连接页没有「分享至连接池」。入池在 Routes 连接池用「从连接同步」，可一次加入多份；连接池也可以直接添加只在池里用的官方登录或 API Key。
- 同步候选由 `canSyncConnectionToPool`（`src/components/login-kernel/eligibility.ts`）决定：所有 API Key 都可同步；官方登录只有 Claude / Codex / Grok；国产官方登录不能分享；已在连接池里的登录不再列出。
- 在连接池里编辑从连接页来的官方登录时，先复制成池里自己的一份（连接页那份保留），再问要不要把模型写回连接页。连接页和连接池相互独立，回收站也分开。
- Routes 二级导航：board / pool / tokens / activity（`/routes` 进看板）。
- 2026-10-03 真窗：取消正在用的 Pi 接入已过（连接不再使用，本机文件回到接入前）。删除正在用的官方登录若没成功，本机文件和默认都不改，再次导入的确认可以点（框里说明同时有 API Key 和官方登录，按服务商分行，不猜一份当前登录）。Claude 先写进去再恢复这一段还没测到，不能算过。

### Sub2API

- 独立的站点管理页：密码登录（验证码 / 2FA 按站点要求），可记住多个账号；密码经 settings 端口写入桌面 SQLite vault（mock 为内存）。
- 登录后可按分组查看、创建、编辑、启用/禁用、删除 API Key，并导入到已安装的 Agent。
- 侧栏入口默认隐藏，在设置「功能」里打开「显示 Sub2API 页面」后显示。

## Chat

### 各 Agent 的对接方式

| 会话 | 对接方式 | 说明 |
| --- | --- | --- |
| 新空 Codex | app-server 常驻进程 | 同一场对话跨轮复用；空闲 10 分钟回收；停止发 `turn/interrupt` 并保留进程 |
| 新空 Grok | ACP（`grok agent --no-leader stdio`） | 模型/思考、图片、后续轮排队 |
| 新空 Kiro | ACP（`kiro-cli acp`） | 生成时不能中途补充，可排队到下一轮 |
| 新空 Claude | stream-json 持续通道 | 同进程多轮；不支持生成中补充；没有可点的允许/拒绝 |
| 其余 Agent、所有旧会话 | 原来的一次性发送 | 有历史的 Claude 走 print + resume |

- Agents 详情写这个 Agent 的新对话怎么接；Chat 会话设置写这次对话实际走哪条。顶栏不写接法。会话字段见 [会话身份](concepts/chat-session-identity.md)。
- Kiro 旧对话没有切到 ACP 的入口。

### Codex

- 新空会话支持：持续回复、命令确认、文件审批、补充/停止、保存与同机重开、会话模型/思考强度、本地图片、「用于本次」技能协议（输入 `/` 选，工具条不画技能按钮）。不含计划模式和完整扩展管理。
- 进程在登录切换、程序路径或文件夹变化、关掉「一直允许」后，等空闲时重启；进程退出或线程关闭后下一轮自动恢复；迟到的或其他线程的通知与批准会被忽略。
- 文件写入：`workspace-write` 不含 `/tmp` 与 `$TMPDIR`。工作目录内的 `apply_patch` 直接写；写到目录外会出「修改文件」卡片（带「一直允许」），有 `diff` / `content` 时展示预览，只有路径时写「仅有路径，无内容预览」。用 shell 写文件走命令批准。
- PATH 上的 Codex 缺旁边的 `codex-code-mode-host` 时，`apply_patch` 会失败，不出卡片。
- Linux 真窗已验：图片、模型×思考强度、停止、关窗续聊、命令与文件批准。macOS 重开续聊有效。Windows 未宣称。
- 历史记录：[B1](archive/chat-codex-b1.md)、[B2](archive/chat-codex-b2.md)。

### Grok

- 不支持为本轮指定「用于本次」技能，界面也不画假按钮。真窗验收已通过。
- 只带官方旗标；`--permission-mode` 放在 `agent` 前后都会让进程提前退出，所以不用。
- `session/new` 带 `_meta.yoloMode=false`，覆盖本机的 always-approve；只有会话打开自动批准时才加 `--always-approve` 和 `yoloMode=true`。
- 握手按官方 ACP 声明本机可读写文件，不发 `initialized`。
- 工作目录外写文件走本机 `fs/write_text_file`：先出「修改文件」卡片再写；目录内直接写。
- 进程退出时写「Grok 已退出」。

### Kiro

- 同一进程内续聊；进程退出或换模型、思考等级、权限时，在同一场对话里重新拉起进程，并把之前的对话内容带进这次提问（Kiro 接不回原会话）。生成中不能改模型和思考等级。
- 本机登录或 `KIRO_API_KEY` 可用时，列模型和打印路径走 AgentHub 自己的 HTTP 调用（非官方 REST，不改工作目录）。多轮用 `kiro-http:<conversationId>` 续同一对话；失败直接报错并保留会话，不回退成新的 CLI 会话。
- 会话设置里的「完全访问权限」是启动时的 `--trust-all-tools`，和卡片上的「一直允许」不是一回事。
- 真窗已验：ACP 新对话；打印路径 HTTP 多轮（Builder ID / 本机登录）。企业 IdC / `profileArn` / `runtime.*.kiro.dev` 未验：登录里有区域和 profile 时会带上，但不等于已验收。

### Claude（新空会话）

- 参数：`-p --input-format stream-json --output-format stream-json --include-partial-messages`；支持本地图片（base64）、模型/思考强度。
- 正文边写边出，整条 assistant 与已出增量去重，不重复显示。
- 停止发 `control_request` interrupt 并保留进程；10 秒内没结束才结束进程。
- 默认 `dontAsk`，危险模式 `bypassPermissions`。
- print 路径出过正文后不再把 `result` 拼进气泡；只有没见过正文时才用 `result`。Linux 真窗已验。见 [Claude B3](archive/chat-claude-b3.md)。

### 通用行为

- **工作目录**：沿用当前对话的文件夹，没有就用最近一次的；仍为空时桌面端用用户主目录（命令层兜底，mock 不兜底）。空对话缺文件夹时显示「选择文件夹」。
- **流式显示**（所有持续通道）：约 80ms 读一次快照，每次拿完整的 `currentMessage`，不猜增量、不假装逐字打出。首字前显示「正在想」，之后「正在写」。
- **过程面板**：主列一行摘要（正在读取 / 修改 / 执行 → 已读取 / 已修改 / 已执行）。点开在右侧栏看思考、工具、等待允许、本轮用量；右侧栏不随发送自动打开。Grok / Kiro 推了 `plan`、或 Claude 用 TodoWrite / Task 更新清单时，输入区上方出现计划条，换轮清掉。约定见 [过程事件](concepts/chat-process-events.md)。
- **用量**：只显示协议给的数字，不估算费用。Codex 取当前轮 `last`；Grok 取 `turn_completed.usage`（有时没有，此时不画）；Grok / Kiro 的 `context_usage` 非零才显示；Kiro 没有 token 累计来源。本轮结束后才用小字显示输入 / 输出。
- **`/` 菜单**：新建对话、复制最近回复；换模型/思考/技能要搜到才列出。Grok / Kiro 声明了命令时另列「Agent 自带命令」（Grok 用 `available_commands_update`，Kiro 用 `_kiro.dev/commands/available`），选中后当作一轮发出。有可启动的命令行时可列「启动命令行」（DeepSeek 为「打开网页会话」），在外部打开。Kiro 的展示 Linux 真窗已验；选中发送和其他 Agent 未验收。
- **对话标题**：先用首条消息提炼；一轮结束后改用对方会话记录里的标题（Codex `local_thread_catalog.display_title`，退回 `session_index.jsonl`；Grok `summary.json`；Kiro `sessions/cli/<id>.json`；DSH 的 `session/title`）。Claude 没有标题来源。手动改过名的不再覆盖。
- **停止**：按钮显示「正在停止」并禁用，直到这一轮真正结束；取消失败时恢复可点。失败或停止后不出现「重试」，可以继续发送。
- **图片**：所有持续通道都有「添加图片」；有历史的旧 Claude 没有。普通文件和 `@` 未接。
- **允许 / 拒绝 / 一直允许**（Codex / Grok / Kiro）：
  - 卡片始终有允许和拒绝；「一直允许」只在请求带了该选项时出现（Codex 会补上；Grok / Kiro 只认对方给的 `allow_always` / `allow_always_tool`）。待处理请求会入库，重开后仍可点。
  - 点「一直允许」后只在本次对话内记住，不写数据库。文件卡记住后，后续文件修改不再出卡；命令和其他工具只记同一条。文件卡的「一直允许」不放行命令。
  - 对话里写「本会话已一直允许」；会话设置里可以关掉，不能从设置里打开。回传失败时写「没法回传允许或拒绝」。
  - 机制见 [Chat 与 Agent](concepts/chat-and-agents.md#允许-拒绝-一直允许)。

## 连接、路由与用量

- 登录的来源、目标和可做的写入由 `plan` / `bind` / `unbind` 契约表达；代码里仍叫 Ticket / TicketPort。
- 本机路由在桌面进程内运行，提供 `/v1/messages`、`/v1/responses`、`/v1/chat/completions` 和 `GET /models`。`/v1/messages` 默认连接池目前只接 Claude，不会把现有 Claude 池改成多 Agent Messages。Codex 与 Grok 都走 Responses 口，格式跟路由一起保存、由入口 Key（本机令牌）选中，不按请求正文猜。接到 Codex / Grok 时写入的是本机令牌和 Responses 接口，不是上游官方登录。见 [连接与路由](concepts/connections-and-routing.md)。
- Kiro 本机路由按请求的 `stream` 返回 JSON 或 SSE；`stream=true` 时上游帧一完成就转发（真窗首字延迟未验）。使用连接池里当前登录的令牌、区域和 profile，不在路由里刷新令牌。
- 用量只读本地 Agent 会话或日志；优先用日志里的官方成本，否则用内置价表估算，不联网拉价格，不换算汇率。Grok 把 `grok-4.6` 与 `grok-4.6-build`（及 `[grok]` / `xai/` 前缀）算作同一模型。

## Skills、MCP 与插件

- **Skills**：分用户技能、项目技能和市场。用户技能在共享目录 `~/.agents/skills/`，可启用到各工具；项目技能读写该项目的 `.agents/skills/`（也列出 `.claude/skills` 等已有目录）。安装支持本地目录、zip、git 地址（需含 `SKILL.md`），只写入目标库、不自动启用。切换前自动备份。
- **MCP**：可扫描本机 MCP；对 Claude / Codex / Grok / Cursor / WorkBuddy 支持目录模板 → 探测 → 写入 / 启用（无 OAuth），`Capability::Mcp` 为 Partial，其余 Planned。Codex 关闭即删条目，Grok 关闭写 `enabled = false`。见 [MCP inventory](reference/mcp-inventory.md)。
- **插件**（`/plugins`）：列出 Claude / Grok / Pi 的包。Claude / Grok 可启用/停用/安装/卸载（卸载默认保留数据目录；Grok 确认后才带 `--trust`），Linux 真窗已验。Pi 只列已装包并标出与配置版本不一致的。不查线上最新版本；没有 `Capability::Plugins`。见 [插件、MCP 与技能](concepts/plugins-and-mcp.md)。

## 各 Agent 的已知细节

- **DeepSeek Harness**：StructuredStream 仍是规划项。检测会跳过缺 `@deepseek-ai/dsh-scope` 的残缺命令（常见 `~/.local/bin/dsh`），提示用官方 npm 装到 `~/.npm-global`。写 `cordis.patch.yml` 时，以 `@` 开头的插件 id 经 `yaml_quote` 加引号。
- **npm 安装位置**：写到 `~/.npm-global`（Windows `%APPDATA%\npm`）。`~/.agenthub/npm` 只是遗留，不是安装目标；DSH 在 PATH 残缺且遗留目录完整时可用它启动，界面标为启动后备。
- **WorkBuddy**：本机安装只打开官网并给中文指引；用量读 `projects/**/*.jsonl` 的 `providerData.usage`（兼容旧 `message.usage`）。
- **ZCode**：安装只打开官网；API Key 按目录追加到 `~/.zcode/v2/config.json`，不替换其他条目，自定义行必须带模型名单。Chat 优先 PATH 上的 `zcode`。Projects 只读，删除提示到 ZCode 里做。用量取 `model_usage`。
- **Kimi**：官方路径写出带模型表的完整 `~/.kimi-code/config.toml`；自定义中继保留上游 `/v1/models` 目录，补全 `[models.<alias>]`，合并重复别名；供应商 `type` 按地址补全。认 `KIMI_CODE_HOME`。Kimi 自己读共享技能库，不再另写一份。
- **Cursor**：能从本机导入登录，但不能写回 Cursor；切换失败给中文说明。

## 通用界面行为

- 「使用官方服务」默认勾选，不禁用智能识别。高级编辑器不显示明文 Key。
- 切换成功提示说明已写入本机配置；安全备份默认在切换/导入时保留副本（可关闭堆积，当次切换仍保留一份用于回滚）。备份标题为「切换前自动 / 手动 + 时间」。
- 官方登录等待页不显示内部状态或文件路径；失败时「重试」是主按钮。Windows 上子进程无窗启动。
- 日志只记 Key 后四位。日志分类见 [日志参考](reference/logging.md)。

## 已知边界

- **未验收**：Windows 上的 Codex 对话；Kiro 企业 IdC 场景。
- **Codex 文本问答**：Chat 能处理 `item/tool/requestUserInput`，但 Codex 0.148 在 Default / `on-request` 下不会发出该请求。产品决定：等上游变成稳定默认再跟，不打开实验开关，不造假卡片。计划模式不在范围内。
- **Codex Computer Use**：Linux 不可用，官方只在 macOS / Windows 桌面端提供。不做假接线，不宣称支持。
- **本机同口授权池**：已默认开启。每个目标 Agent 一个默认池，共用本机入口和令牌；默认 `priority_failover`，可改 `round_robin`；官方直连不自动入池。能力矩阵写 `multi_account=false` 时，已入索引的 `v2_pool` 仍允许多成员，这是已授权行为。配额类冷却按 Retry-After 或重置提示，否则配额约 15 分钟、额度约 30 分钟（上限 1 小时）。混合供应商复合路由和 Codex↔Grok 双向 Responses 仍是实验开关、默认关闭。保存的本机入口和格式必须和当前端点一致，否则启动失败，不会悄悄直通。见 [本机 Routes API](reference/local-route-api.md)，设计稿见 [归档](archive/unified-loopback-pool.md)。
- **路由决策**：`AdapterRouteService::plan()` 是唯一决策者；`adapter-capability-contract.json` 是它的只读快照，Rust 测试保证二者一致。浏览器 mock 只查表，未命中一律 unsupported。见 [Adapter 路线内核](architecture/adapter-route-kernel.md)。
- **未实施**：live/default `agenthub-adapterd` sidecar（隔离目录下的 Messages 探测切片已有，见 `go/agenthub-adapterd` 与 `scripts/route-runtime-probe/messages-isolated.sh`，不是现行网关）；托盘低内存后台模式；插件包更新与 Codex/Pi 插件安装（见 [插件管理](proposals/plugin-management.md)）；其余 Agent 的 MCP 与 OAuth Connector。
- **构建**：不用 sccache，不拆 `agenthub-core`；CI 用 `Swatinem/rust-cache`；Windows worktree 不得共享 `target/`。
- **范围外**：凭据落盘加密、国产 OAuth 适配、OAuth 转 API。见 [产品边界](decisions/product-boundaries.md)。

## 验证与发布

- 前端与 backend 的分层规则见 [AGENTS.md](../AGENTS.md#前端-backend-分层)，命令与测试选择见 [测试与验证](guides/testing-and-validation.md)。
- PR CI 跑前端类型检查、构建、测试、三个 Rust crate 测试和 Playwright Chromium 冒烟（只覆盖 `dev:mock`）。
- 版本以 `package.json` 为准，`pnpm release:sync-version` 同步 Cargo 文件。正式发布由 `dev` 上推送的 `v*` tag 触发，tag 指向的提交必须已在 `release` 上。
- 自动更新优先读镜像 `https://updates.agenthub.qooo.io/latest.json`，失败回退 GitHub；签名私钥只在 CI。见 [国内自动更新镜像提案](proposals/update-mirror-r2.md)。

## 以谁为准

1. 源码、测试和 `package.json` / Cargo 配置决定当前行为。
2. 本页记录跨模块事实；领域契约以对应参考文档为准。
3. 方案和路线图必须写明 `proposed` 或 `historical`，不能覆盖当前事实。
4. 已完成的一次性方案进入 [archive/](archive/README.md)，不再派工。
