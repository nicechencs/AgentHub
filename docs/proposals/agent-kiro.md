---
title: 接入 Kiro（kiro-cli）Agent
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-11
audience: contributor
---

# 接入 Kiro（kiro-cli）Agent

> 提案，不是现行实现契约。YAML 保持 `status: proposed`（STYLE 要求提案必须 proposed）。若干切片已落地，**不要把本页当成未开工**。现行行为以 [STATUS](../STATUS.md) 为准。

接线与实现纪律见 [添加 Agent](../guides/adding-an-agent.md)；半面参照 `crates/agenthub-core/src/adapters/cursor.rs`。本文只定目标、公开事实与产品决策，不重复指南里的文件清单与验收清单。

## 进度（已落地 / 剩余边界）

对照 [STATUS](../STATUS.md)。本表只防止把早期方案当成待办，不是现行契约。

| 状态 | 内容 |
| --- | --- |
| **已落地** | Agents 页检测 / 安装 / 登录指引 / API Key |
| **已落地** | 新对话走 `kiro-cli acp` 持续通道：同一进程内续聊；可点允许/拒绝、停止；生成时不能中途补充 |
| **已落地** | Chat 打印路径 HTTP 多轮，经 `kiro-http:<conversationId>` 续场（详见 [HTTP 提案](agent-kiro-http.md)） |
| **已落地** | Kiro 登录经本机路由接到 Claude / Codex / Grok |
| **已落地** | 本机路由 `stream=true`：上游 event-stream 帧完成即转发文本块（单测；真窗 TTFT 未验） |
| **剩余边界** | 企业 IdC / `profileArn` 实机验收（带上参数 ≠ 已验收；ACP 不注入该字段） |
| **剩余边界** | Chat 打印路径与本机路由 JSON 仍收齐再返回；真窗 TTFT；客户端断开不停上游读 |
| **剩余边界** | 官方 REST（不宣称、不接入） |
| **历史约束（不是现行待办）** | 「一轮一发」「不接持续通道」「本机路由后置」——早期第一波方案，见 §3 |

## 1. 背景与目标

### 1.1 Kiro 是什么

Kiro 是 AWS 系 agentic 编码产品，同一套项目上下文可跨多种界面使用：

| 界面 | 用途 | AgentHub 是否接入 |
| --- | --- | --- |
| IDE | 本地编辑器内协作 | **否**（不嵌 IDE） |
| **CLI（`kiro-cli`）** | 终端原生、可 headless / CI | **是（本提案范围）** |
| Web / Mobile / Crew | 云沙箱、移动端、常驻 agent | **否** |

官方入口：

- 产品：<https://kiro.dev/>
- CLI：<https://kiro.dev/docs/cli/>
- 安装：<https://kiro.dev/docs/getting-started/installation/>
- 登录：<https://kiro.dev/docs/getting-started/authentication/>
- 交互式 Chat：<https://kiro.dev/docs/cli/chat/>
- Headless：<https://kiro.dev/docs/cli/headless/>
- Autocomplete / Inline：<https://kiro.dev/docs/cli/autocomplete/>
- CLI 3.0：<https://kiro.dev/docs/cli/v3/>

### 1.2 本提案要解决什么

在 AgentHub 新增一家可管理的第三方 Agent，使：

1. Agents 页能发现、检测、安装指引、查看登录状态；
2. 在有证据的前提下支持 headless 发送（类似 Cursor Agent CLI）；
3. 后续按证据开放 Skills / MCP / 项目等稀疏端口；
4. **（历史约束）不**在未验证协议时接入 Chat 持续通道或本机路由。ACP 新对话与本机路由已落地，见进度表；不要再当待办。

## 2. 命名与身份

| 概念 | 取值 | 说明 |
| --- | --- | --- |
| Agent id（对内） | `kiro` | catalog key，小写 |
| 展示名（对人） | Kiro | 副标题可写命令行 |
| 可执行文件 | `kiro-cli` | PATH / 安装探测目标 |
| 解析别名 | `kiro-cli` → `kiro` | 与 `cursor` / `cursor-agent` 同模式 |
| 项目配置目录 | 项目内 `.kiro/` | steering、agents、hooks 等（跨界面共享） |
| 用户级会话等 | 待探测；文档提到 `~/.kiro/sessions/`（v3） | 写入前本机核实 |

不要把 Agent id 做成 `kiro-cli`：CLI 是安装形态，不是产品身份。

## 3. 早期第一波能力立场（历史方案）

本节是早期第一波方案，**不是现行待办**。「一轮一发」「不接持续通道」「本机路由后置」均已过期：新对话走 ACP，本机路由已接入。旧打印对话仍可 headless。

第一波对标 **Cursor Agent CLI 半面**：检测 / 安装 / 登录指引 / API Key / headless 发送。

| 维度 | Cursor（已有） | Kiro（建议） |
| --- | --- | --- |
| 产品卡 | Cursor Agent，不管 IDE | Kiro，不管 IDE/Web |
| 二进制 | `agent` / `cursor-agent` | `kiro-cli` |
| 安装 | curl / PowerShell 官方脚本 | 见 §4.1 |
| Headless | `agent -p …` | `kiro-cli chat --no-interactive "…"` |
| 危险模式 | `--force` | `--trust-all-tools` / `--trust-tools=…` |
| 结构化输出 | Unsupported（仅 text） | `stream-json` → 第二波再评 |
| 登录 | API Key / login 指引 | 浏览器/设备码 + `KIRO_API_KEY` |
| 配置写入 | fail-closed | 第一波同样 fail-closed |

**明确不做（第一波，历史约束）：** 假 `ChatRuntime`（允许/拒绝/补充按钮）、本机路由、配置表单写入、Usage 精算、插件管理页、把终端 Autocomplete / Inline 说成对话页能力。其中持续通道与本机路由已按进度表落地；其余仍按能力矩阵，不是从本页抄待办。

Chat 持续通道后置（**历史约束**）：当时 Kiro headless 无中途输入，与 Codex app-server 不是同一协议。现行新对话已走 `kiro-cli acp`，见进度表。先例见 [Claude Chat B3](../archive/chat-claude-b3.md)。

本机路由默认不做（**历史约束**）。现行 Kiro 登录已可接到 Claude / Codex / Grok；剩余边界见进度表。接线纪律仍见 [添加 Route Adapter](../guides/adding-an-adapter.md)。

### 3.1 对话页立场（第一波 = 一轮一发；历史约束）

官方把 CLI 分成两套体验，不能混谈：

| 官方表面 | 入口 | 对话页能不能当成同一套 |
| --- | --- | --- |
| 交互式 Chat / TUI | `kiro-cli`、`kiro-cli chat`、`/model` `/agent` 选择器、中途输入 | **否**。第一波不接持续通道（**历史约束**），也不做假选择器 |
| Headless | `kiro-cli chat --no-interactive "…"`，需 `KIRO_API_KEY` | **是**。对标 Cursor `agent -p`，一轮发完等结果 |

官方 Headless 限制（[headless](https://kiro.dev/docs/cli/headless/)）：必须带初始 prompt；**会话中途不能再输入**；交互式斜杠命令（`/model`、`/agent` 选择器）不可用；TUI 关闭。`--trust-all-tools` / `--trust-tools=…` 是发之前预先批准，不是中途点允许/拒绝。`--output-format stream-json` 要 V2/V3，第一波不当成已接入的过程流。

因此第一波对话页对 `kiro`（**历史约束，不是现行待办**）：

- **就是一轮一发 / 发出去等结果**，走 headless，**不是**持续 `ChatRuntime`。现行新对话已走 ACP，见进度表；旧打印对话仍可 headless。
- **不要**做中途允许/拒绝/补充界面；没有真实通道就不要假按钮（与 Claude B3 同一条红线）。ACP 落地后允许/拒绝来自真实通道，不是假按钮。
- **不要**因为交互式 CLI 有 `/model`、`/agent` 选择器，就在 AgentHub 做一套假的。
- `kiro` **不是** `isRuntimeChatAgent`（当时只有 `codex` / `grok`）。残留的持续会话快照不得给 Kiro 打开请求面板、补充或斜杠换模型。现行 ACP 新对话以 STATUS 为准。

主 Agent 为 `kiro` 时，第一波对话页应表现为（**历史约束**；现行新对话见进度表与 STATUS）：

| 位置 | 行为 |
| --- | --- |
| 横幅 / 文案 | 说清：这里一轮一发、需要 API Key、不能中途补充或点允许/拒绝；终端里的模型/Agent 选择、命令补全和灰色提示不在本页 |
| 输入框 | 有工作目录且已配置 API Key 就可写、可发；发送中只显示停止，不出现「补充」或「本轮结束后发送」 |
| 自动批准 | 打开时映射 `--trust-all-tools`（或实现时再收窄到 `--trust-tools=…`）；文案说「跳过工具确认」，不要说成中途审批 |
| API Key | Headless 官方要求 `KIRO_API_KEY`。未配置授权时沿用现有「未配置」拦截，不要假装已登录的交互式会话能在本页续聊 |
| 这一轮结束 | 结果留在对话记录；失败/停止用现有结果条。再发是新的一轮，不是同一场交互式会话的下一句 |
| 斜杠菜单 | 不要弹出暗示 Kiro 交互式 CLI 的 `/model`、`/agent` 选择器；用户打 `/model` 就当普通正文发出 |

目录里还没有 `kiro` 时，不要在界面里假装已安装一家 Kiro（**历史约束**；catalog 已有 `kiro`）。助手与测试按 id `kiro` 先落地；catalog 出现后再被选中。

### 3.2 CLI Autocomplete / Inline（终端能力，不是对话页）

官方 [Completions & autocomplete](https://kiro.dev/docs/cli/autocomplete/) 写的是 **`kiro-cli` 自己的终端/shell 能力**，和对话页、headless 发送不是同一件事：

| 能力 | 是什么 | 开关 / 命令 |
| --- | --- | --- |
| Autocomplete 下拉 | 打命令时在光标右侧出现选项、子命令、参数，方向键选择，Tab / Enter 采纳 | 安装后默认开；`kiro-cli settings autocomplete.disable false\|true`；主题 `kiro-cli theme dark\|light\|system` |
| Inline 灰色提示 | 输入时出现 ghost text，右方向键或 Tab 采纳 | 与下拉**互相独立**；`kiro-cli inline enable\|disable\|status\|set-customization\|show-customizations` |

它们覆盖数百个命令行工具（`git` / `npm` / `docker` / `aws` 等），排错也是终端侧：查 `kiro-cli --version`、`settings autocomplete.disable`、重启终端、换 shell；Inline 查 `inline status` 后再 `enable`。

**AgentHub 对话页不得宣称已有 Kiro 命令补全或灰色提示**，除非真的嵌了 PTY 终端（第一波明确不做）。可选后续：只提供「打开外部终端」或文档链接，仍不把这两项标成对话页已支持。不以伪终端当对话底座的候选见 [Chat 宿主加深](chat-host-depth.md)。

## 4. 公开事实（截至 2026-09-06；≠ 本仓库已验证）

### 4.1 安装与平台

官方支持 macOS、Linux、Windows 11。

| 平台 | 官方安装方式 | 落地路径 / 门槛 |
| --- | --- | --- |
| macOS | 与 Linux 同一 `curl -fsSL https://cli.kiro.dev/install \| bash`（脚本内 Darwin 分支；bash only） | 产物 `Kiro CLI.dmg`；`hdiutil` → `ditto` 到 `/Applications` → `open … --no-dashboard`（需 GUI）；Intel / Apple Silicon 分支；已有同名 app 时可能 interactive `read`。**IDE.app ≠ `kiro-cli`**，探测只认 CLI 二进制 |
| Linux | 同上 | zip（gnu/musl）→ `~/.local/bin`；glibc x86_64 ≥ 2.34、aarch64 ≥ 2.39，否则 musl；替换时可能 interactive；`--force` 主要针对 Q CLI legacy |
| Windows 11 | `irm 'https://cli.kiro.dev/install.ps1' \| iex`（需 PowerShell） | MSI → `C:\Program Files\Kiro-Cli\`；`msiexec /quiet` |

- Unix 脚本：<https://cli.kiro.dev/install>；Windows：<https://cli.kiro.dev/install.ps1>
- 备选 MSI：`https://desktop-release.q.us-east-1.amazonaws.com/latest/kiro-cli-x86_64-pc-windows-msvc.msi`
- CLI 3.0 early：`kiro-cli --v3`；与 2.x 并存；**AL2 不支持 3.0**；会话格式 v2/v3 不兼容（升级前备份 `~/.kiro/sessions/`）

跨平台是第一波目标（三端均需覆盖）。安装渠道形态可对齐 Cursor（Unix sh + Windows ps1）并保留 WorkBuddy 式指引回退；具体落点与失败处理由实现者按 [添加 Agent](../guides/adding-an-agent.md) 判断。注意：Unix 脚本可能 TTY `read`；macOS 可能需 GUI；Windows MSI 可能需提升——失败时勿谎称已装。

### 4.2 登录

| 方式 | IDE | CLI | 备注 |
| --- | --- | --- | --- |
| Google / GitHub / AWS Builder ID | ✓ | ✓ | 浏览器或设备码 |
| IAM Identity Center / 组织 IdP | ✓ | ✓ | 需 Start URL + region |
| API Key | | ✓ | headless / CI 用 `KIRO_API_KEY` |

CLI：`kiro-cli login`；远程/SSH 可用 `--use-device-flow`。

产品红线：可做打开登录 / 设备码指引 / 检测已登录 / API Key 录入；不做官方登录→API Key 转换；不做国产 OAuth。

### 4.3 Headless（第一波核心）

```bash
kiro-cli chat --no-interactive "your prompt"
kiro-cli chat --no-interactive --trust-tools=read,grep "…"
kiro-cli chat --no-interactive --trust-all-tools "…"
kiro-cli chat --no-interactive --trust-all-tools --output-format stream-json "…"
```

官方限制：必须带初始 prompt；无会话中途输入；交互式 `/model`、`/agent` 选择器不可用；无 TUI；`stream-json` 需 engine V2/V3。与 Cursor `-p` 同级，**不等于**持续 ChatRuntime，也**不等于**终端 Autocomplete / Inline。

诊断（探测时可用）：`kiro-cli doctor`、`whoami`、login 状态类；以本机实测为准。

### 4.4 项目级资产（第二波候选）

| 资产 | 路径/形态 | 备注 |
| --- | --- | --- |
| Steering | `.kiro/steering/` | 只读候选 |
| Custom agents | `.kiro/agents/*.yaml` | 勿与 AgentHub「Agent」混淆 |
| Hooks | `.kiro/hooks/*.json`（v3） | 范围外或只读 |
| Skills / MCP | 官方称可用 | 目录稳定后再评能力级别 |
| 日志 | Linux `$XDG_RUNTIME_DIR/kiro-log/…`；可 `KIRO_CHAT_LOG_FILE` | Usage 需脱敏 fixture |

## 5. 分波与能力起点

**第一波（已落地）：** Agents 管理面：检测 / 安装 / 登录指引 / API Key。早期对话页曾按 headless 一轮一发（§3.1，历史约束）；现行新对话走 ACP。

**第二波（部分落地）：** 项目只读、用量、列模型已有 Partial/Full，见 [capabilities](../reference/capabilities.md)。Skills / MCP 仍 Planned。

**第三波（ACP 新对话已落地，不是未立项）：** 新空会话走 `kiro-cli acp`。旧打印对话保留原方式。剩余边界见进度表（企业 IdC / `profileArn`、Chat 打印收齐、真窗 TTFT、官方 REST）。

能力矩阵诚实起点（**历史草稿**；现行以 [capabilities](../reference/capabilities.md) 为准，不要按本表派工）：

| Capability | 建议 | 原因草稿 |
| --- | --- | --- |
| ConfigWrite | Unsupported | 无稳定 round-trip |
| AccountSwitch | Unsupported | 由 Kiro 登录体系管理 |
| ApiKeyAccount | Partial | `KIRO_API_KEY`；官方登录走指引 |
| Skills / Mcp / Usage / ModelSelect / ProjectHistory / StructuredStream | Planned | 待路径与契约核实 |
| DangerousMode | Partial / Full | 映射 trust 旗标（`--trust-all-tools` / `--trust-tools`）；文案说清风险 |
| ProjectDelete / ProviderPresets / LiveBackup | Unsupported | 无安全契约前不做 |
| SessionResume | Unsupported / Planned | 当时：headless 无中途输入、持续聊另立项（**历史**；ACP 新对话已落地，现行 Partial 见 capabilities） |
| Autocomplete 下拉（对话页） | Unsupported / 范围外 | `kiro-cli` 终端补全，不是 AgentHub 对话 UI；未嵌 PTY 不得宣称支持 |
| Inline 灰色提示（对话页） | Unsupported / 范围外 | 与下拉独立的终端 ghost text；同上，最多外链/打开终端 |

## 6. 开干前建议核实（历史探测清单）

第一波探测已做过，不要把本节当成「尚未开工」。下列是当时的事实缺口，不是强制命令矩阵。剩余边界（企业 IdC / `profileArn`、Chat 打印收齐 / 真窗 TTFT、官方 REST）仍需按进度表核实，不要从本清单推导新待办：

- 各平台官方安装后：`kiro-cli` 绝对路径、版本（2.x vs 3.x / `--v3`）
- macOS：arch；CLI 二进制位置（勿把仅有 IDE.app 当成已安装）
- Linux：gnu vs musl、glibc 版本；替换是否 interactive
- Windows：MSI / PATH / 是否需提升
- `doctor` / 登录态（设备码是否适合无桌面）/ 登录态文件路径（脱敏，勿把密钥写入仓库）
- `KIRO_API_KEY=… chat --no-interactive …` 的 argv、退出码；可选 `stream-json` 事件形状
- `.kiro/` 与 `~/.kiro/` 真实布局；PATH 是否另有同名

探测笔记可放本机或后续 `docs/status/agent-kiro-s0.md`（验证后再建）。

## 7. 产品相关风险

- **2.x / 3.0 行为分裂** → detect 记版本，能力按版本降级
- **IDE ≠ CLI** → 产品卡与 detect 只认 `kiro-cli`；文案区分 Applications 里的 Kiro IDE
- **不要假能力** → 无协议不接线持续 Chat（**历史约束**；ACP 新对话已落地）；不做假 `/model` `/agent`；不把终端 Autocomplete / Inline 写成对话页能力；ConfigWrite fail-closed
- 安装脚本管道、TTY/`open`/UAC 失败 → 与现有 native 渠道同样失败文案，勿谎称已装

**非目标：** 凭据落盘加密、国产 OAuth、官方登录转 API Key、嵌入 IDE、Crew/Web 云沙箱托管。

## 8. 决策记录与剩余边界

下列 1–4 已按产品面落地，不再是「是否开干」的待批项。本页 YAML 仍为 `proposed`，因为剩余边界尚未成为现行契约。

1. **第一波半面（检测/安装/登录指引/API Key）已落地**；跨平台安装渠道以实现与 STATUS 为准。
2. 展示文案用 **Kiro**，副标题写命令行。
3. 云电脑登录可走设备码（实现以 `kiro-cli` 登录指引为准）。
4. **ACP 新对话已落地**，不再问「第三波是否进路线图」。§3.1 的一轮一发是历史约束。
5. **仍是提案边界（未写成现行契约）：** 企业 IdC / `profileArn` 实机验收；Chat 打印路径收齐再返回与真窗 TTFT；官方 REST。HTTP 切片细节见 [agent-kiro-http.md](agent-kiro-http.md)。

（安装 UX 细节——如自动 ps1 vs MSI 指引、自动 sh vs `setup_guide`——实现时可对齐 Cursor/WorkBuddy 惯例，不必在提案层钉死。）

本页保持 `status: proposed`。已落地切片的现行行为只写在 STATUS / capabilities / 概念页；不要从本页抄成实施清单。

## 9. 参考

- [添加 Agent](../guides/adding-an-agent.md)（真实接线指南）
- [添加 Route Adapter](../guides/adding-an-adapter.md)（早期「默认不启用」是历史约束；现行本机路由见进度表）
- [capabilities](../reference/capabilities.md)
- [Chat 统一体验](chat-unified-experience.md)
- [Claude Chat B3](../archive/chat-claude-b3.md)
- Cursor 半面：`crates/agenthub-core/src/adapters/cursor.rs`
- 官方 Headless：<https://kiro.dev/docs/cli/headless/>
- 官方 Autocomplete / Inline：<https://kiro.dev/docs/cli/autocomplete/>
- 官方交互式 Chat：<https://kiro.dev/docs/cli/chat/>
- 官方 CLI 3.0：<https://kiro.dev/docs/cli/v3/>
