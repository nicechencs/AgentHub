---
title: 接入 Kiro（kiro-cli）Agent
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-06
audience: contributor
---

# 接入 Kiro（kiro-cli）Agent

> 提案，不是现行实现契约。未验证前不要把下列能力标成已支持，也不要在 UI 做假确认 / 假持续聊。

接线与实现纪律见 [添加 Agent](../guides/adding-an-agent.md)；半面参照 `crates/agenthub-core/src/adapters/cursor.rs`。本文只定目标、公开事实与产品决策，不重复指南里的文件清单与验收清单。

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
- Headless：<https://kiro.dev/docs/cli/headless/>
- CLI 3.0：<https://kiro.dev/docs/cli/v3/>

### 1.2 本提案要解决什么

在 AgentHub 新增一家可管理的第三方 Agent，使：

1. Agents 页能发现、检测、安装指引、查看登录状态；
2. 在有证据的前提下支持 headless 发送（类似 Cursor Agent CLI）；
3. 后续按证据开放 Skills / MCP / 项目等稀疏端口；
4. **不**在未验证协议时接入 Chat 持续通道或本机路由。

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

## 3. 能力立场（诚实半面）

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

**明确不做（第一波）：** 假 `ChatRuntime`（允许/拒绝/补充按钮）、本机路由、配置表单写入、Usage 精算、插件管理页。

Chat 持续通道后置：Kiro headless 无中途输入，与 Codex app-server 不是同一协议。有官方稳定双向协议或经验证 ACP 后再单独立项；先例见 [Claude Chat B3](../status/chat-claude-b3.md)。

本机路由默认不做；若有稳定 writer 与备份策略，按 [添加 Route Adapter](../guides/adding-an-adapter.md) 另立项。

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

官方限制：必须带初始 prompt；无会话中途输入；无 TUI；`stream-json` 需 engine V2/V3。与 Cursor `-p` 同级，**不等于**持续 ChatRuntime。

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

**第一波：** Agents 管理面 + headless 发送（推荐开干范围）。用户能安装/检测、看登录状态、用 API Key 或已登录态发一轮。

**第二波：** 路径证据齐全后，Skills / MCP / 项目只读；评估 `stream-json`。

**第三波（可选）：** 仅当有稳定交互协议或经验证 ACP 再立项。

能力矩阵诚实起点（实施时以探测改表；穷尽匹配与原因写法见指南）：

| Capability | 建议 | 原因草稿 |
| --- | --- | --- |
| ConfigWrite | Unsupported | 无稳定 round-trip |
| AccountSwitch | Unsupported | 由 Kiro 登录体系管理 |
| ApiKeyAccount | Partial | `KIRO_API_KEY`；官方登录走指引 |
| Skills / Mcp / Usage / ModelSelect / ProjectHistory / StructuredStream | Planned | 待路径与契约核实 |
| DangerousMode | Partial / Full | 映射 trust 旗标；文案说清风险 |
| ProjectDelete / ProviderPresets / LiveBackup | Unsupported | 无安全契约前不做 |
| SessionResume | Unsupported / Planned | headless 无中途输入；持续聊另立项 |

## 6. 开干前建议核实

下列是事实缺口，不是强制命令矩阵。至少在目标平台记版本与路径（云电脑可先 Linux；macOS / Windows 用真机）：

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
- **不要假能力** → 无协议不接线 Chat；ConfigWrite fail-closed
- 安装脚本管道、TTY/`open`/UAC 失败 → 与现有 native 渠道同样失败文案，勿谎称已装

**非目标：** 凭据落盘加密、国产 OAuth、官方登录转 API Key、嵌入 IDE、Crew/Web 云沙箱托管。

## 8. 决策请求

1. **是否批准第一波范围**（半面：检测/安装/登录指引/API Key/headless 发送；含跨平台）？
2. 展示文案用「Kiro」还是「Kiro CLI」？（建议：**Kiro**，副标题写命令行。）
3. 云电脑默认登录走 **设备码** 是否接受？
4. 第三波 Chat/ACP 是否进路线图，还是明确「仅 headless」？

（安装 UX 细节——如自动 ps1 vs MSI 指引、自动 sh vs `setup_guide`——实现时可对齐 Cursor/WorkBuddy 惯例，不必在提案层钉死。）

批准前本页保持 `proposed`。批准并完成必要探测后，从最新 `dev` 开 `feat/agent-kiro` 实施第一波；落地后更新 capabilities / STATUS / CHANGELOG。

## 9. 参考

- [添加 Agent](../guides/adding-an-agent.md)（真实接线指南）
- [添加 Route Adapter](../guides/adding-an-adapter.md)（默认不启用）
- [capabilities](../reference/capabilities.md)
- [Chat 统一体验](chat-unified-experience.md)
- [Claude Chat B3](../status/chat-claude-b3.md)
- Cursor 半面：`crates/agenthub-core/src/adapters/cursor.rs`
