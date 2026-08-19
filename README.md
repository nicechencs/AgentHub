# AgentHub

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D6.svg)](#快速开始)
[![Release](https://img.shields.io/github/v/release/nicechencs/AgentHub?label=version)](https://github.com/nicechencs/AgentHub/releases)

AgentHub 是一个本地运行的多 Agent 桌面管理工具。它用统一的 GUI 和 CLI 管理 AI Coding Agent 的安装环境、连接、Skills、用量与本地会话。

技术栈：**Tauri v2 + React + Rust**。Windows、macOS、Linux 都是正式桌面平台。GitHub Releases 发布三端安装包：Windows 安装包与 updater 已签名；macOS updater 已签名（Apple 公证未承诺）；Linux 发布 `.deb` 与 AppImage，安装包可以未签名。Linux 自动更新只有在 Release 的 `latest.json` 含已签名 `linux-x86_64` 条目时可用，否则请手动下载新安装包。

## 主要功能

以下描述当前源码工作区；发布包可能滞后，请以对应版本的 [Release notes](https://github.com/nicechencs/AgentHub/releases) 为准。

| 模块 | 用途 |
|---|---|
| **Dashboard** | 查看 Agent 状态、Token 趋势、成本估算与解析健康度；已安装 Agent 可直接连接/切换 |
| **Agents** | 检测 Node/npm/Git 等运行环境，安装、升级或卸载 Agent |
| **Connections** | 管理官方登录与 API Key，查看一份登录正用于哪些编程工具，并接到其他工具。白话说明见 [产品决策](docs/product-decisions.md)；领域模型见 [连接绑定](docs/connection-binding-model.md)。切换前自动备份 live |
| **Routes** | 本机路由运行时（端口、启停、恢复）；侧栏英文 Routes，有本机路由才出现。日常绑定走 Dashboard / Connections；规则见[厂商、API 与 OAuth 规则](docs/provider-api-oauth-adaptation.md) |
| **Skills** | 以 `~/.agents/skills/` 为共享真源，向各 Agent 同步技能 |
| **MCP** | 只读汇总各 Agent 的本机 MCP 配置；管理与注入仍在规划中 |
| **Chat** | 在桌面端调用一个或多个本机 Agent，并展示流式过程 |
| **Projects** | 浏览、整理和汇总各 Agent 的本地项目与会话 |
| **Settings** | 四个分区：偏好 / 本机 / 备份 / 关于。本机含数据目录、日志与本机路由入口；备份是独立分页（`/settings?tab=backups`，`/backups` 会跳过来），不占侧栏 |
| **CLI** | 提供 doctor、env、agent、provider、account、skill、usage、backup、run 等命令 |

当前内置适配：**Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、Cursor Agent、DeepSeek Harness（dsh）**。各家能力不同，请以 [能力矩阵](docs/capability-matrix.md) 或以下命令为准：

```powershell
cargo run -p agenthub-cli -- agent capabilities
```

> 把已有登录接到另一个编程工具，只可能是三种做法：直接改配置 / 写进对方认的登录 / 本机转发。界面芯片是「直连 / 用这份登录 / 本机路由 / 当前不支持」。能改配置或写进对方认的登录，就不做转发。白话与图见 [产品决策](docs/product-decisions.md)。现在能不能写上去见[适配规则矩阵](docs/provider-api-oauth-adaptation.md#4-当前实现矩阵)。本机转发仍有未收口路径，端到端验收未完成；界面不再标「实验」。Cursor 只支持公开的 Agent CLI，不能作为写入目标。

## 快速开始

### 使用发布包

从 [GitHub Releases](https://github.com/nicechencs/AgentHub/releases) 下载对应平台的桌面安装包：

| 平台 | 安装包 | 签名 |
|---|---|---|
| Windows | NSIS `.exe` / MSI | 安装包与 updater 已签名 |
| macOS | `.dmg`（另有 updater `.app.tar.gz`） | updater 已签名；Apple 公证未承诺 |
| Linux | `.deb` 与 AppImage | 安装包可以未签名；自动更新仅在该版本提供了签名 AppImage 时可用 |

Debian / Ubuntu 可用 `.deb`；其他发行版优先 AppImage。也可以按下面的源码路径运行真实桌面端（不是 mock）。

### 从源码运行

需要 [Node.js](https://nodejs.org/) LTS、[Rust](https://rustup.rs/) 和 pnpm。

Windows：

```powershell
.\run.ps1
```

也可以双击 `run.bat`。脚本会检查开发依赖并启动 Tauri 桌面端。

macOS / Linux：

```bash
chmod +x ./run.sh
./run.sh --check    # 只检查依赖，不启动
./run.sh            # 启动 Tauri 桌面端（真实后端）
```

Linux 还需要 Tauri 的系统库（脚本缺项时会打印发行版命令，**不会**自动 `sudo`）：

```bash
# Debian / Ubuntu
sudo apt-get update
sudo apt-get install -y \
  build-essential curl wget file pkg-config \
  libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev
```

Fedora / Arch 命令见 `scripts/check-linux-prereqs.sh --print-packages`。openSUSE / Alpine 等其他发行版不要套用 `apt-get`，请用本机包管理器或看该脚本里的说明。桌面端需要图形会话（`DISPLAY` 或 `WAYLAND_DISPLAY`）。无桌面时可用 `pnpm dev:mock` 看前端演示。

仅查看前端和演示数据，无需真实 Agent：

```bash
pnpm install
pnpm dev:mock
```

## 常用命令

| 命令 | 说明 |
|---|---|
| `pnpm tauri:dev` | 启动桌面端，连接真实 Tauri 后端 |
| `pnpm dev:mock` | 启动浏览器 mock 环境 |
| `pnpm typecheck` | 前端类型检查 |
| `pnpm test` | 前端测试 |
| `pnpm build` | 前端生产构建，强制使用 Tauri adapter |
| `pnpm tauri:build` | 构建桌面安装包 |
| `pnpm tauri:build:macos` | 构建本地使用的 macOS `.app` |
| `pnpm tauri:build:linux` | 构建本地使用的 Linux `.deb` 与 AppImage（未签名；正式包由 `release` 分支的 GitHub Actions 发布） |
| `./run.sh --check` | 检查 macOS/Linux 源码运行依赖 |
| `cargo test -p agenthub-core` | 运行 Rust core 测试 |
| `cargo run -p agenthub-cli -- --help` | 查看 CLI 帮助 |

正式 Release 由 `release` 分支的 GitHub Actions 生成和发布，本地发布命令已禁用。

## 数据与隐私

AgentHub 默认只处理本机数据：

| 路径 | 内容 |
|---|---|
| `~/.agenthub/` | SQLite 状态、设置与日志等应用数据 |
| `~/.agenthub/backups/` | 切换或修改前创建的 live 配置快照 |
| `~/.agents/skills/` | Skills 共享真源 |

- Usage 只读解析本地会话或日志，不通过代理截取请求，也不上传云端。
- 凭据沿用项目现有存储方案；界面、CLI 和日志输出会进行脱敏。
- Adapter 引用已有 Connection，不复制凭据；Bridge 不记录请求或响应正文。

完整边界见 [隐私规范](docs/privacy.md) 和 [安全策略](SECURITY.md)。

## 架构

```text
React GUI ── Tauri commands ─┐
                             ├── agenthub-core ── Agent adapters / SQLite / local files
agenthub CLI ────────────────┘
```

- `agenthub-core` 集中业务逻辑，GUI 与 CLI 是薄入口。
- Service 负责编排备份、切换、投影和聚合；Agent Adapter 负责路径、配置格式和能力差异。
- Adapter 的 `local_bridge` 目标由同包用户级 `agenthub-adapterd` sidecar 长驻托管；当前版本仍由 Tauri 进程内宿主，详见[迁移方案](docs/adapter-sidecar-design.md)。Connections 数据域不随 sidecar 拆分。
- 前端只有 `src/lib/backend/tauri/` 可以调用 Tauri `invoke`。
- `pnpm dev:mock` 使用浏览器 mock；生产构建不会静默回退到 mock。

```text
crates/agenthub-core/   Rust 业务核心
crates/agenthub-cli/    CLI
src-tauri/              Tauri 桌面壳
src/                    React 前端
docs/                   设计、契约与测试文档
scripts/                构建与发布脚本
```

## 文档与贡献

- [文档索引](docs/README.md)
- [产品决策（把已有登录接到另一个工具）](docs/product-decisions.md)
- [当前实现状态](docs/agenthub-plan.md#8-当前实现状态以代码与测试为准)
- [架构说明](docs/architecture.md)
- [Adapter Sidecar 目标架构](docs/adapter-sidecar-design.md)
- [能力矩阵](docs/capability-matrix.md)
- [厂商、API 与 OAuth 适配规则](docs/provider-api-oauth-adaptation.md)
- [CLI 与配置](docs/cli-and-config.md)
- [测试约定](docs/testing.md)
- [新增 Agent 指南](docs/adding-an-agent.md)

欢迎提交 Issue 或 PR。修改代码前请阅读 [AGENTS.md](AGENTS.md)；提交前至少运行改动范围内的测试和类型检查。安全问题请按 [SECURITY.md](SECURITY.md) 私下披露，不要创建公开 Issue。

## 许可证

本项目采用 [MIT License](LICENSE)。

AgentHub 在用量解析与配置切换方面借鉴了 [ccusage](https://github.com/ccusage/ccusage) 和 [cc-switch](https://github.com/farion1231/cc-switch)，感谢相关作者与社区。
