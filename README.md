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
| **Connections** | 跨工具登录列表。顶部按 Agent 筛选；OAuth 用头像、API Key 用钥匙。一份登录接到另一个工具只有三种做法：直连、写进对方登录、本机转发。切换前自动备份当前配置。白话说明见 [产品决策](docs/product-decisions.md) |
| **Routes** | 本机转发运行时（启停、恢复）；列表与详情显示 127.0.0.1 和端口。日常连接走 Dashboard / Connections |
| **Skills** | 以共享目录 `~/.agents/skills` 向各 Agent 同步技能 |
| **MCP** | 只读查看各工具已配置的额外 MCP 能力（不会改它们的设置） |
| **Chat** | 在桌面端调用一个或多个本机 Agent，并展示流式过程 |
| **Projects** | 浏览、整理和汇总各 Agent 的本地项目与会话 |
| **Settings** | 四个分区：偏好 / 本机 / 备份 / 关于。本机含数据目录与日志；备份是独立分页（`/settings?tab=backups`，`/backups` 会跳过来），不占侧栏 |
| **CLI** | 提供 doctor、env、agent、provider、account、skill、usage、backup、run 等命令 |

当前内置适配：**Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、Cursor Agent、DeepSeek Harness**。各家能力不同，可用以下命令查看：

```powershell
cargo run -p agenthub-cli -- agent capabilities
```

> 把已有登录接到另一个编程工具，只可能是三种做法：直接改配置、写进对方认的登录、本机转发。能改配置或写进对方认的登录，就不做转发。白话与图见 [产品决策](docs/product-decisions.md)。Cursor 只支持公开的 Agent CLI，不能作为写入目标。

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
| `pnpm tauri:build:linux` | 构建本地使用的 Linux `.deb` 与 AppImage（未签名；正式包由推送 `v*` tag 的 GitHub Actions 发布） |
| `./run.sh --check` | 检查 macOS/Linux 源码运行依赖 |
| `cargo test -p agenthub-core` | 运行 Rust core 测试 |
| `cargo run -p agenthub-cli -- --help` | 查看 CLI 帮助 |

正式 Release 由推送到仓库的 `v*` tag 触发 GitHub Actions 生成和发布（tag 必须指向 `release` 分支上的提交），本地发布命令已禁用。日常 PR 合入 `dev`。要出新版本，须同时 bump `package.json`、`Cargo.toml` 的 `[workspace.package]`、`src-tauri/tauri.conf.json`（当前都是 0.2.3；已有 tag `v0.2.2`，workflow 会拒绝覆盖已有 tag），先更新 `release` 分支，再打并推送匹配的 `vX.Y.Z` tag。`dev` 与 `release` 是无关历史（`dev` 于 8 月中改写过），不要把 `dev` 合并进 `release`。

## 数据与隐私

AgentHub 默认只处理本机数据：

| 路径 | 内容 |
|---|---|
| `~/.agenthub/` | SQLite 状态、设置与日志等应用数据 |
| `~/.agenthub/backups/` | 切换或修改前创建的配置快照 |
| `~/.agents/skills/` | Skills 共享目录 |

- Usage 只读解析本地会话或日志，不通过代理截取请求，也不上传云端。
- 凭据沿用项目现有存储方案；界面、CLI 和日志输出会进行脱敏。
- 把一份登录接到另一个工具时不复制凭据；本机转发不记录请求或响应正文。

完整边界见 [隐私规范](docs/privacy.md) 和 [安全策略](SECURITY.md)。

## 架构

```text
React GUI ── Tauri ─┐
                    ├── agenthub-core ── 各 Agent 适配 / SQLite / 本机文件
agenthub CLI ───────┘
```

- `agenthub-core` 集中业务逻辑，GUI 与 CLI 是薄入口。
- `pnpm dev:mock` 只用于浏览器演示；生产构建不会静默回退到 mock。

```text
crates/agenthub-core/   Rust 业务核心
crates/agenthub-cli/    CLI
src-tauri/              Tauri 桌面壳
src/                    React 前端
```

## 文档与贡献

### 给使用者

- [把已有登录接到另一个工具](docs/product-decisions.md)（白话三种接法）
- [隐私规范](docs/privacy.md)
- [安全策略](SECURITY.md)

### 给贡献者

以下是开发文档，不是产品说明书。

- [文档索引](docs/README.md)

贡献者开发约定在仓库根目录 `AGENTS.md`。欢迎提交 Issue 或 PR。提交前请运行改动范围内的测试和类型检查。安全问题请按 [SECURITY.md](SECURITY.md) 私下披露，不要创建公开 Issue。

## 许可证

本项目采用 [MIT License](LICENSE)。

AgentHub 在用量解析与配置切换方面借鉴了 [ccusage](https://github.com/ccusage/ccusage) 和 [cc-switch](https://github.com/farion1231/cc-switch)，感谢相关作者与社区。
