# DeepSeek Harness 接入方案

> 状态：**P1–P5 已接入代码**（2026-08-15）  
> 调研依据：官方站点、开发者文档、GitHub `deepseek-ai/deepseek-harness`（MIT，developer preview）。  
> 真源关系：本文是 **DSH 接入** 的唯一设计真源。实施时按 [adding-an-agent.md](adding-an-agent.md) 走生产接入轨；能力声明按 [capability-matrix.md](capability-matrix.md)；票面与跨 Agent 边按 [product-decisions.md](product-decisions.md)、[connection-binding-model.md](connection-binding-model.md)、[provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)。DeepSeek API 票走 **① API 直连**，不要把 DSH 当 ③ 协议桥。  
> 实现状态以 adapter `capability()`、已注册的 `platform/*/sources` 与测试为准。本文保留设计约束；能力级别以代码声明为准。

---

## 0. 结论（先读）

DeepSeek Harness（产品命令 `dsh`）是 **第八个已接入 Agent**，不是「DeepSeek API 票」本身，也不是通用协议桥。

| 对象 | 身份 | 现在做什么 |
|---|---|---|
| **DeepSeek Harness** | Agent，`AgentKey` / 兼容期 `AgentId` = `dsh`，展示名 **DeepSeek Harness** | 按稀疏端口接入：安装、配置、账号、Skills、用量、会话/项目、headless run |
| **DeepSeek API** | 票面（API Key，含官方 Responses，属 ①） | 接到 `dsh` 走 `config_sync`（`deepseek-api-to-dsh-v1`）；接到 Claude 走 experimental `native_endpoint`（`deepseek-api-to-claude-v1`，已开）；接到 Codex 走 experimental `native_endpoint`（`deepseek-api-to-codex-v1`，官方 Responses） |

不要用 `deepseek` 当 Agent id：它会和票面、模型名、官方 API 混在一起。命令与 npm 包都以 `dsh` 出现，和现有 `pi` / `claude` 一样用 CLI 名做 key。

首期只接 **npm 全局 `dsh`**。不把 `npx … web`、源码 clone、Python SDK 写成可执行安装渠道。凭据继续沿用现有存储方案；**凭据落盘加密无必要 / 范围外**。

---

## 1. 调研摘要

### 1.1 产品是什么

DeepSeek Harness 是 DeepSeek 开源的 agent harness（developer preview，MIT）。设计口号是 **everything is a plugin**：模型、工具、Skills、会话、沙箱、存储、loop、调度、UI 都是 Cordis 插件，没有需要打补丁的特权内核。

运行时由 **profile** 叠出来：

1. 各 bundle 的 patch（`dsh-base` → 其余组合包）
2. profile 自己的 `cordis.patch.yml`
3. home 级 `$DSH_HOME/cordis.patch.yml`
4. `--patch` overlay

`dsh-base` 提供模型适配、工具、持久化、沙箱/审批、设置、凭据、遥测。`dsh-web-app` 加浏览器 UI；`dsh-headless` 是一次性 runner，不启 HTTP。

预设模式（产品能力，不是 AgentHub 安装渠道）：

| 模式 | 含义 |
|---|---|
| Standard | 完整 coding agent：编辑、shell、搜索、skills、plan、goals、subagents |
| Code | Standard + 用生成的 TypeScript 编排多轮工具 |
| Minimal | 仅 persistent bash + `str_replace_editor` |
| Creator | Standard + 运行时检查与插件实验 |

### 1.2 对外入口

| 入口 | 用途 | AgentHub 态度 |
|---|---|---|
| `npx @deepseek-ai/dsh web` | 体验 Web UI（默认 `127.0.0.1:3080`） | 启动方式，**不是** install channel |
| 全局 `dsh`（npm `@deepseek-ai/dsh`） | 产品 launcher：`--profile`、`web`、`headless`、`plugin` | **P1 唯一安装/检测对象** |
| 源码 `pnpm dsh` | 贡献者工作流 | 不进产品渠道 |
| Python `deepseek-harness-sdk` | 捆绑 runtime，可不依赖系统 Node | **P0 不接**（第二套 detect / home / 会话根） |

### 1.3 和现有七家的差别

DSH 更像 **Pi**（Node + 插件 + 多 provider），但配置真源是 **Cordis patch 层**，不是单一 `settings.json` / `models.json`。会话真源是 **append-only `SessionEvent` 日志**（JSONL 或 SQLite），resume / fork / replay / Trajectory / telemetry 都从同一条流派生。模型可见的事实必须能从日志重建。

---

## 2. 身份、路径与平台边界

### 2.1 身份

| 项 | 值 |
|---|---|
| `AgentKey` / 兼容期 `AgentId` | `dsh` |
| `display_name` | DeepSeek Harness |
| 检测二进制 | `dsh`（PATH；Windows 另认 `.cmd` / `.exe`） |
| 家目录 | `$DSH_HOME`，缺省 `~/.dsh` |
| 配置目录 | 同 home（home 级 patch + profiles） |
| Skills 投影根 | `$DSH_HOME/skills`（官方 rank 400 `user-dsh`） |
| 前端装饰 | `src/config/agents.ts` 的 `AGENT_DISPLAY` + `src/styles/tokens.ts` 品牌色；未知 key 已有 fallback，**登记时再补色** |

兼容期仍要改 `AgentId` 枚举、`ALL`、`register_all()`。这是现有 façade 的成本，不是新的封闭模型。test-only 轨继续用独立 `AgentKey`，不借用 `dsh`。

### 2.2 公开路径（产品级，细节以 adapter 源码为准）

文档只写官方已公开的产品根，不展开凭据文件内部字段（见 [privacy.md](privacy.md)）：

- Home：`$DSH_HOME` 或 `~/.dsh`
- Home 级用户 patch：`$DSH_HOME/cordis.patch.yml`
- Profile：`$DSH_HOME/profiles/<name>/`（`web` / `headless` 等）
- 凭据缝：配置里只存 **引用**（环境变量名）；本地 provider 的默认文档在 home 下
- Skills：见 §7
- 会话：见 §5；具体落点以 `persistenceRoot` / profile 配置为准，写入 adapter 后用本地样例锁定

### 2.3 平台

AgentHub 以 Windows 为交付重点。官方事实：

- Node `dsh web` 是主产品入口，依赖 Node.js。
- Python SDK 的 `danger-full-access` + 持久 PTY **不支持 Windows**。
- 沙箱：Linux Landlock、macOS Seatbelt、Windows ACL restricted-token。

因此：

- **安装 / detect / 配置只读 / Skills 投影 / 用量扫描** 可以按 Windows-first 做。
- **headless run、危险模式、结构化流** 必须本机验证后再标 Full；未验证前标 Planned / Partial，并写清 Windows PTY 限制。
- 不发明不存在的 native `install.ps1` / `install.sh` 渠道。

---

## 3. 端口映射（按现有稀疏端口）

平台 service / 页面 **禁止** 再写 `dsh` 分支。差异只进 adapter 与 `platform/*/sources`。

| 端口 | P1 | 目标 | 说明 |
|---|---|---|---|
| `platform/paths` | 必做 | Full | `DSH_HOME` → `~/.dsh` |
| `platform/install` | 必做 | npm only | `@deepseek-ai/dsh`；`requires = [nodejs, npm]` |
| `platform/detection` | 必做 | `dsh --version` | 不把 `npx` 缓存、源码 checkout、Python wheel 判成已安装 |
| `platform/config` | P2 | 先只读，写 fail-closed | 投影 home patch 的 provider / model / maxTokens；整行替换语义未测通前不写 |
| Account / auth | P2 | ApiKey Full；Switch Partial | 只接 DeepSeek API Key；经凭据引用写入，不复制密钥到 patch |
| `platform/skills/target` | P3 | Full | 投影到 `$DSH_HOME/skills` |
| `platform/usage` | P3 | 有 fixtures 后 Full | 解析会话日志里的 provider usage；启发式 Token Meter **不当** 计费真源 |
| `platform/projects` | P3 | History Full；Delete Partial | 从 session header 的 `cwd` / `id` / lineage 建项目树 |
| `platform/stream` | P4 | 未验证前 Planned | 等 headless 事件契约稳定 |
| `platform/lifecycle` | 随 install | 复用 coordinator | 不改 runtime start/stop |
| Adapter 规则 | P5 | DeepSeek API → `dsh` native | 不把 DSH 当协议网关 |

绑定入口走现有 `AdapterCapabilityMatrix` + `AdapterApplyService`（**不**另建 `accepts`/`writer` 模型）。`dsh` 可写 live：DeepSeek API Key → 官方 provider 槽 + `.credentials.yaml`。无 writer 的 Agent（如 Cursor）不能当 bind 落点。

---

## 4. 安装管理

### 4.1 渠道（诚实）

对标 Pi：只登记一条可执行渠道。

| 渠道 | 包 / 命令 | 前置 | 是否登记 |
|---|---|---|---|
| npm | `@deepseek-ai/dsh` | Node.js + npm | **是** |
| npx 一次性 web | `npx @deepseek-ai/dsh web` | 同上 | **否**（启动，不是安装） |
| 源码 pnpm | clone + `pnpm install && pnpm run build` | Node + pnpm + git | **否** |
| Python SDK | `pip install deepseek-harness-sdk` | Python 3.10+ | **否**（P0） |

`min_runtime_notes` 以官方当时要求为准，写入 install contribution，不在文档抄死版本号。卸载只删 AgentHub 登记过的全局 bin 候选，不删 `$DSH_HOME`，除非用户显式 `--purge-config`。

### 4.2 detect

`detect_binary(AgentId::Dsh, &["dsh"], &["--version"], Some("npm"), env_ready)`。

拒绝：

- 只存在 `npx` 缓存、没有 PATH 上的 `dsh`
- 仓库里的 `pnpm dsh` 包装
- Python `DeepSeekHarness` 进程
- 名为 `deepseek` 但不是 harness 的其它 CLI

`doctor` 走已注册 detect，不写新分支。未安装时其它页不得假成功。

### 4.3 安装管线

复用现有两阶段：`ensure_env(nodejs, npm)` → `npm i -g @deepseek-ai/dsh`。失败降级为可复制命令，禁止假成功。官方脚本/镜像问题归「环境/网络」，给 [官方文档](https://deepseek.com/harness/en/) 入口，不替上游背锅。

`dsh plugin --profile <name>` 是 **profile 内 Cordis 插件**（pnpm 转发），不是 AgentHub Skills，也不进 install channel。

---

## 5. 会话管理

### 5.1 对方模型

会话是 append-only `SessionEvent` 流。`deriveMessages()` 从日志投影模型历史；`assistant/chunk` 保留回放与 UI。resume / fork / search / replay / Trajectory 都读同一条流。

一轮（turn）= 若干 step；一步 = 一次模型请求 + 其工具调用。日志变体包括 `turn/start|end`、`step/start|end`、`user/message`、`assistant/chunk|message`、`tool/call|result`、`request/header` 等。崩溃恢复会给未闭合 turn 补 `turn/end { reason: interrupted }`，不截断已落盘事件。

`SessionHeader` 在日志外：`id`、`createdAt`、`cwd`、`parentSession`、`seedLength`、`origin`（如 subagent）、`delegationDepth`、`agentPreset`。fork 用 `ctx.sessions.fork`；子会话用 `origin: 'subagent'` 避免项目列表重复。

持久化是可换缝：JSONL（每会话一个 artifact，`locate()` 给绝对路径）或 SQLite（共享库，`locate()` 为空）。Python 示例把 JSONL 写在 `session_root`；部分 composition 默认 `./.sessions`。

### 5.2 AgentHub 怎么管

零侵入：只扫描对方已持久化的日志，不另建会话库，不代理 Web UI。

| AgentHub 能力 | 做法 | 首期级别 |
|---|---|---|
| ProjectHistory | 扫已知 persistence 根；用 header 的 `cwd` / `id` / `createdAt` / lineage | 有样例后 Full |
| ProjectDelete | 只删确认过的单会话 JSONL artifact；SQLite 共享库 **不删整库** | Partial |
| SessionResume | DSH 自身能 resume；AgentHub Chat **仍不**接各家原生续会话 | 与其它家一样 Planned |
| Chat / `run` | 新 `session_id`；需要续跑时显式复用 id（P4） | Planned |
| 备份 | live 只含 home patch、凭据文档、profile manifest；**不含**会话正文 | Full（路径集合） |

发现顺序（实现时写入 usage/project source，不在 service 里 hardcode）：

1. `$DSH_HOME` 下 profile / persistence 配置指出的根
2. 用户显式 `persistenceRoot` / `DSH_SESSION_ROOT`
3. 工作区旁官方默认会话目录（需本地样例证实后再加）

SQLite 后端：用量与项目列表走官方只读观察 API 的落盘等价物（header + 可解析事件），禁止当普通 JSONL 扫库文件。格式版本高于本构建认识的范围时 **跳过并计数**，不标 corrupt。

---

## 6. Token 计算

### 6.1 对方有两套数

`@deepseek-ai/dsh-token-meter` 给的是 **请求压力 / 表面启发式**，不是账单：

- `totalTokens`：当前请求+响应压力
- `surfaceTokens`：表面节点启发式之和
- `baseline.kind === 'usage'`：最近一次成功调用的 provider usage 可复用
- `estimated`：没有可用 usage 锚点时的启发式

AgentHub Usage 的产品语义是 **零侵入解析本地日志里的 provider usage**（与 Claude / Pi / Grok 相同），再按模型价表估算费用。因此：

- **计费事件**：只收日志里明确的 provider usage（如 `request/header` 之后的 usage 字段、assistant 完成事件）。缺字段就 skip，计入 parser health。
- **启发式 `surfaceTokens` / `estimateMessage`**：最多做 ParserHealth 诊断，**不写入** `usage_events`，避免和真实 usage 混加。
- 不在 Rust 里嵌 Node 去调 `ctx.tokenMeter.measure()`。

### 6.2 采集

新增 `platform/usage` source，对标 Pi：`discover_files` + 逐行 `extract_dsh`。

解析纪律：

1. 先用脱敏 fixtures 锁字段；无样例不得标 Usage=Full。
2. 模型名从 `request/header` / agent options 继承到后续 usage 行。
3. 缓存 token 按现有惯例从 input 中剥离，不把 reasoning 重复加进总量（除非官方 usage 字段已分开且价表需要）。
4. 费用：日志若带官方 cost 则优先；否则 `token ×` DeepSeek 价表（`pnpm pricing:update` 增补，不手写第二份）。
5. fork / subagent：跳过 seed 前缀里的父历史 burst，避免重复计费。
6. 增量 cursor 按文件偏移；压缩 JSONL（Zstd 等）解不开就 skip + failed，不半解。

`Capability::Usage`：parser + fixtures 合并前保持 **Planned**。Cursor 那种「无稳定日志」才是 Unsupported；DSH 有日志，只是我们还没接。

---

## 7. Skill 管理

### 7.1 对方怎么发现

`ctx.skills` 合并多层 provider。本地文件系统根（rank 低者赢）：

| Rank | Source | 根 |
|---|---|---|
| 100 | `project-dsh` | `<git-root>/.dsh/skills` |
| 200 | `project-agents` | `<git-root>/.agents/skills` |
| 300 | `custom` | 配置的 `customSkillDirs` |
| 400 | `user-dsh` | `$DSH_HOME/skills` |
| 500 | `user-agents` | `~/.agents/skills` |
| 600 | `bundled` | 可选打包根 |

名字必须 kebab-case。接受目录包（`SKILL.md`）或扁平 `.md`。**不**递归 `**/SKILL.md`。模型侧 `skill({ name })` 只看到 name + description；body 按需加载。frontmatter `disable-model-invocation` / `user-invocable` 控制是否进模型目录。

DSH **已经会读** AgentHub 的共享真源 `~/.agents/skills`（rank 500）。这是对齐点，也是重复投影风险。

### 7.2 AgentHub 怎么投

真源不变：`~/.agents/skills` + lock。`dsh` 的 `skills_dir()` = `$DSH_HOME/skills`。

| 规则 | 原因 |
|---|---|
| 投影到 `user-dsh`（rank 400） | 比 `user-agents`（500）更优先，AgentHub 分配结果覆盖同名共享技能 |
| 禁止再往 `~/.agents/skills` 写一份「DSH 专用副本」 | 那里已是真源；DSH 自己会扫 |
| 不扫、不改项目级 `.dsh/skills` | 项目技能归仓库；AgentHub 已有 `skill project` |
| 不把 Cordis 插件当 Skill | `dsh plugin` 是 runtime 组合，走配置/文档，不进 Skills 市场 |
| 不启用官方默认关掉的 badge 打包技能 | 那是对方 opt-in |

 reconciler 仍用现有 copy/sync，不在本阶段做 `projection_mode=link`。DSH 对未投影技能仍能从 `~/.agents/skills` 看见——这是对方行为，不是第二真源。UI 需要的话只提示「未分配时 DSH 仍可能读到共享根」，不要为此改真源模型。

`Capability::Skills`：`skills_dir` 可用即 Full。Kimi 那种「无技能目录模型」才是 Unsupported。

---

## 8. 模型路由

这里有两层，必须分开。

### 8.1 DSH 进程内路由（对方）

`AgentOptions`：`provider` + `model` + 可选 `maxTokens`。`provider` 必须是已注册的 `ctx.llm` adapter；`model` 由该 adapter 解释，**不必**事先出现在目录里。`listModels()` 是建议目录，不是请求白名单。

已观察到的官方 adapter：

| 插件 | 典型 route | 用途 |
|---|---|---|
| `@deepseek-ai/dsh-llm-deepseek` | `deepseek-official` | 官方 DeepSeek；`apiKeyEnv` 默认 `DEEPSEEK_API_KEY`；`baseURL` 可回落 `DEEPSEEK_BASE_URL` |
| `@deepseek-ai/dsh-llm-pi-ai` | 配置字典的 key | OpenAI 兼容 / 多供应商；无配置则休眠 |

子 agent / ACP 创建时也可带 `provider` / `model`（默认常见为 `deepseek-official`）。

AgentHub **不**在 DSH 里再挂一个自研 LLM adapter，也不把 DSH 当 loopback 桥。换模型 = 改 home patch / 当前 binding 投影。

### 8.2 AgentHub 票 → Agent 路由

| 票面 | 目标 | 路线 | 何时开放 |
|---|---|---|---|
| DeepSeek API Key | `dsh` | `native` / `config_sync` | P5：写入凭据引用 + `deepseek-official` + 模型 id |
| DeepSeek API Key | Claude Code | experimental `native_endpoint` | 已开（`deepseek-api-to-claude-v1`）；官方 Anthropic 兼容入口，见适配规则文 §2.4 |
| DeepSeek API Key | Codex | experimental `native_endpoint` | 已开（`deepseek-api-to-codex-v1`）；官方 Responses 入口，AgentHub 写入 `wire_api=responses`，不启动本机路由 |
| Kimi / OpenAI / 其它 Chat 兼容票 | `dsh` | 可能 `config_sync` 到 `dsh-llm-pi-ai` | **另证**；P5 不做 |
| 任一家 OAuth | `dsh` | `unsupported` | DSH 凭据缝是 API Key 引用，不是 OAuth |

`Capability::ModelSelect`：全产品仍是 Planned（运行时目录）。DSH 的 provider/model 写在配置投影里，不单独做「模型商店」。官方「联网拉全量模型目录」仍是非目标。

---

## 9. 模型与凭据配置

### 9.1 写哪里

只改 **home 级用户层** `$DSH_HOME/cordis.patch.yml`。

不改：

- bundle 内置 patch（升级会被覆盖）
- 安装包 / `node_modules`
- 除非用户明确管理某个 profile，否则不改 `profiles/<name>/cordis.patch.yml`

Cordis patch **按 id 整行替换**，不能只补一个键。未对目标行做完整 round-trip 前，`write_config` **fail-closed**（`ConfigWrite = Partial`）。

只读投影可以先做：`dsh --profile web --dump-config` 的解析结果，或读取 home patch + 已知 settings section，给 `GenericConfigForm` 看 provider / model / maxTokens / thinking。dump-config 当诊断，不把整棵插件树当可编辑表单。

`@deepseek-ai/dsh-llm-deepseek` 已公开、可投影的字段：

- `apiKeyEnv`（默认 `DEEPSEEK_API_KEY`）
- `baseURL`
- `thinking`：`enabled` \| `disabled`
- `reasoningEffort`：`off` \| `low` \| `high` \| `max`
- `maxTokens` / `defaultContextWindow`
- `models[]`：建议目录（id / name / contextWindow / maxTokens）

### 9.2 凭据

对方规则：配置只持有 **引用**；值由 credentials provider 按次解析。空值视为未配置。环境变量挡住同一引用时，写入会被拒绝。

AgentHub：

- Connections 继续持有 DeepSeek API Key 票（现有存储，不加密）。
- apply / bind 只把 **引用名** 写进 patch（如 `DEEPSEEK_API_KEY`），值写进对方凭据缝或进程环境，**不**把密钥抄进 `cordis.patch.yml`。
- `read_auth` / `read_account` 只报告是否已配置、health、脱敏摘要。
- `build_api_key_account`：`format=api_key`，`ApiKeyAccount = Full`。
- `AccountSwitch = Partial`：能切 Key 引用；无 OAuth。
- 不扫描、不展示、不备份会话正文里的密钥。

### 9.3 live backup

`live_backup_paths`：home `cordis.patch.yml`、凭据文档、各 profile 的 `package.json` / `cordis.patch.yml`。排除 `node_modules`、会话日志、插件缓存。

---

## 10. Headless run 与 Chat（P4）

官方：`dsh --profile headless "<job>"` 开一条新的持久会话，打印最终答案后退出。Python SDK：`harness.run(prompt, session_id=…)`；同一 harness + 同一 `session_id` 会保留该会话的 bash/cwd。

P4 才做 `build_run_spec`，且必须先量：

- 非交互、可超时、可取消
- Windows 是否能跑 headless（PTY）
- stdout 是纯文本还是 NDJSON
- 危险模式对应哪条官方 flag / composition，禁止发明 `--yolo`

在此之前：`StructuredStream`、`DangerousMode` 保持 Planned / Partial；Chat 卡片可以 detect + 连接，不假装能流式对话。

---

## 11. 目标能力矩阵（接入后；现在不是事实）

无 adapter、无本地样例前，**不得**把下表写进 CLI 快照。

| Capability | 目标 | reason |
|---|---|---|
| ConfigWrite | Partial | 只写 home patch；整行替换未测通则 fail-closed |
| AccountSwitch | Partial | 仅 API Key 引用切换 |
| ApiKeyAccount | Full | DeepSeek API Key |
| Skills | Full | `$DSH_HOME/skills` 可投影 |
| LiveBackup | Full | home patch + 凭据文档 + profile manifest |
| StructuredStream | Planned | headless 事件契约未验证 |
| DangerousMode | Partial | 存在 danger composition；Windows PTY 未验证 |
| ProjectHistory | Full | 会话 header / 日志可扫（有样例后） |
| ProjectDelete | Partial | 仅单会话 JSONL；不删 SQLite 整库 |
| ProviderPresets | Partial | 内置 `deepseek-official`，不是通用预设商店 |
| Usage | Planned → Full | 有 usage fixtures 后升 Full |
| Mcp | Planned | 与其它家相同；只读 inventory 不改矩阵 |
| ModelSelect | Planned | 走配置投影，不做运行时模型商店 |
| SessionResume | Planned | Chat 不接原生 resume |

---

## 12. 分阶段落地

按端口切开，每阶段可单独 PR。不把「加 AgentId」和「Usage 解析」绑在一次提交。

### P1 — 能看见、能装

- `AgentId::Dsh` / `as_str` / `parse` / `display_name` / `ALL`
- `adapters/dsh.rs`：detect、npm channel、诚实 `capability()`、`skills_dir`、`live_backup_paths`、`build_run_spec` 先 Unsupported 或最小 text
- `register_all()` + paths + install contribution
- 前端 `AGENT_DISPLAY` + `TOKEN_AGENT_IDS` 品牌色
- 绑定入口：现有矩阵 cell（不是新的 `accepts`/`writer` 模型）
- 测试：detect fixture、catalog 含 `dsh`、capability 穷尽、install allowlist

**验收**：`doctor` 能报未安装/已安装；Agents 页能装 npm 渠道；未安装时其它页不假成功。

### P2 — 配置投影 + API Key 入池

- config projector：provider / model / maxTokens / thinking / apiKeyEnv
- `read_auth` / API Key 入池；apply 把 Key **剥进** `.credentials.yaml`，patch 只留引用
- 整棵 Cordis 树仍 fail-closed（`ConfigWrite=Partial`）

**验收**：Connections 可入 DeepSeek Key；apply 不把密钥写入 patch；无整树 round-trip 不开放 ConfigWrite=Full。

### P3 — Skills / Usage / Projects

- skill target 指向 `$DSH_HOME/skills`
- usage source + 脱敏 JSONL fixtures
- project source：header → 项目树；delete 仅 JSONL

**验收**：`skill sync --agent dsh` 只写 user-dsh 根；usage collect 有事件或明确 skip；parser health 可见。

### P4 — headless / Chat

- `build_run_spec`：`dsh --profile headless "<prompt>"`，设 `DSH_HOME`
- 未验证 NDJSON：**不**注册 stream parser，`StructuredStream=Planned`
- 不发明 `--yolo` / 未验证 danger flag

### P5 — 票绑定

- DeepSeek API → `dsh`：`config_sync`，`canApply=true`，`rule_id=deepseek-api-to-dsh-v1`
- DeepSeek API → Claude：experimental `native_endpoint` 已开（`deepseek-api-to-claude-v1`）
- Codex 走官方 Responses `native_endpoint`（`deepseek-api-to-codex-v1`）；不把 DSH 当协议桥，不做 OAuth 写入 DSH 凭据缝或二次投影

---

## 13. 明确不做

- 不把 DSH 做成 AgentHub 内嵌 Webview 或第二套 Chat 运行时。
- 不托管 Cordis 插件市场，不代执行 `dsh plugin` 装任意 npm。
- 不把 Python SDK / 源码 checkout 当已安装。
- 不建设公网网关、模型商店、负载均衡。
- 不把 Token Meter 启发式写入计费库。
- 不把 DSH 当 Messages↔Responses 桥。
- 不扫描用户家目录里未配置的随意 `.sessions`。
- 不做凭据落盘加密。
- 不在平台 service / 页面加 `dsh` 分支。

---

## 14. 风险

1. **Developer preview**：插件 id、事件变体、配置字段会变。所有解析器按版本容错，未知事件 skip。
2. **patch 整行替换**：漏字段会抹掉用户插件行。写路径必须 fixtures + 备份。
3. **双 Skills 根**：DSH 已读 `~/.agents/skills`。只投影到 `$DSH_HOME/skills`，避免双写。
4. **会话后端分叉**：JSONL vs SQLite vs 压缩。发现逻辑按 backend kind 分支，禁止当一种文件扫。
5. **Windows headless**：PTY / danger composition 可能不可用。安装成功 ≠ Chat 可用。
6. **双协议票面**：DeepSeek API 的 Chat Completions 与 Anthropic 入口不是同一条边。接到 `dsh` 用官方 provider；接到 Claude 走 Anthropic 兼容入口。
7. **用量字段漂移**：无 fixtures 不标 Full。

---

## 15. 实施位置（已按清单落地）

按 [adding-an-agent.md](adding-an-agent.md) §1.1：

| 步骤 | 位置 |
|---|---|
| Adapter | `crates/agenthub-core/src/adapters/dsh.rs` + `mod.rs` `register_all` |
| 枚举 | `models/agent.rs` `AgentId` |
| 路径 | `platform/paths/sources.rs` |
| 安装 | `platform/install/sources.rs`（npm `@deepseek-ai/dsh`） |
| 配置 | `platform/config/sources/dsh.rs`（P2） |
| Usage | `platform/usage/sources/dsh.rs` + `usage/session_jsonl.rs` 发现函数（P3） |
| Projects | `platform/projects` source（P3） |
| Stream | **未注册** parser；`StructuredStream=Planned`，headless 仅 text run spec |
| 绑定 | `AdapterCapabilityMatrix` `deepseek-api-to-dsh-v1` + apply 白名单（P5） |
| 前端装饰 | `src/config/agents.ts`、`src/styles/tokens.ts`、`src/lib/types.ts` `KNOWN_AGENT_IDS` |
| CLI 帮助 | `cli-and-config.md` 的 agent id 列表随代码改 |
| 能力快照 | 实现后重跑 `agenthub agent capabilities --markdown` |

---

## 16. 官方资料

- [DeepSeek Harness 产品页](https://deepseek.com/harness/en/)
- [GitHub deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)
- [Architecture](https://deepseek-harness.github.io/deepseek-harness/en/reference/)
- [Core / AgentOptions](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/core)
- [Session persistence](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/persistence)
- [Skills](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/skills)
- [Token Meter](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/token-meter)
- [LLM streaming / LlmAdapter](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/llm-streaming)
- [Credentials](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/credentials)
- [Plugin config catalog](https://deepseek-harness.github.io/deepseek-harness/en/reference/config-catalog)
- [Python SDK](https://deepseek-harness.github.io/deepseek-harness/en/guide/python-sdk)
- [CLI README](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/cli/README.md)
- [DeepSeek API 双协议与定价](https://api-docs.deepseek.com/quick_start/pricing/)
- [DeepSeek Anthropic 兼容](https://api-docs.deepseek.com/guides/anthropic_api)
