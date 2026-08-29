# AgentHub

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D6.svg)](#快速开始)
[![Release](https://img.shields.io/github/v/release/nicechencs/AgentHub?label=version)](https://github.com/nicechencs/AgentHub/releases)

AgentHub 是一个本地运行的多 Agent 桌面工具。它用一个 GUI 和 CLI 管理 AI coding agent 的安装环境、登录与连接、Skills、用量和本地会话。产品由 Tauri v2、React、Rust 组成，支持 Windows、macOS 和 Linux。

## 功能概览

- 管理 Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、ZCode、Cursor Agent 和 DeepSeek Harness 等本机 Agent。Cursor Agent 默认不出现在侧栏和连接页，可在 Agents 里取消隐藏。
- 查看跨工具的登录与连接，优先直接写入目标工具配置；只有存在开放规则和受测协议转换时才使用本机路由，否则明确显示“当前不支持”，不会静默转发。
- 管理共享 Skills、查看 MCP 能力、浏览项目与会话。
- 在桌面端启动本机会话并查看流式过程，解析本地会话日志中的用量与成本估算。
- 通过 CLI 执行 `doctor`、`env`、`agent`、`provider`、`account`、`skill`、`usage`、`backup`、`run` 和 `config` 等操作。

当前实现的边界与已知未实现项见 [实现状态](docs/STATUS.md)。产品决策与连接模型见 [文档索引](docs/README.md)。

## 快速开始

### 使用发布包

从 [GitHub Releases](https://github.com/nicechencs/AgentHub/releases) 下载对应平台的安装包：

| 平台 | 安装包 | 发布说明 |
| --- | --- | --- |
| Windows | NSIS `.exe` 或 MSI | 安装包与 updater 已签名 |
| macOS | `.dmg` | updater 已签名；Apple 公证不作承诺 |
| Linux | `.deb` 或 AppImage | 安装包可以未签名；自动更新只在发布清单含签名 Linux 条目时启用 |

### 从源码运行

需要 Node.js LTS、Rust、Git 和 pnpm。

```powershell
pnpm install
pnpm tauri:dev
```

Windows 也可以运行 `.\run.ps1`；macOS/Linux 可以运行 `./run.sh`。这些脚本会检查桌面端依赖并启动真实 Tauri 后端。Linux 系统库缺项时，运行 `./scripts/check-linux-prereqs.sh --print-packages` 查看对应发行版的安装提示。

只想在浏览器中查看演示数据时：

```bash
pnpm install
pnpm dev:mock
```

`pnpm dev` 是普通 Vite 开发服务器，用于前端开发；它不是 Tauri 启动命令，也不会自动提供 mock 后端。需要真实桌面后端请使用 `pnpm tauri:dev`，需要演示数据请使用 `pnpm dev:mock`。

## 常用命令

| 命令 | 用途 |
| --- | --- |
| `pnpm dev` | 启动普通 Vite 前端开发服务器 |
| `pnpm dev:mock` | 启动浏览器 mock 演示 |
| `pnpm tauri:dev` | 启动真实 Tauri 桌面端 |
| `pnpm typecheck` | 检查前端类型 |
| `pnpm typecheck:test` | 检查测试类型 |
| `pnpm test` | 运行前端测试 |
| `pnpm test:e2e:browser` | 运行 Playwright 浏览器冒烟（仅 `dev:mock`） |
| `pnpm build` | 运行生产前端构建，强制使用 Tauri adapter |
| `pnpm tauri:build` | 构建桌面安装包 |
| `cargo test -p agenthub-core --locked` | 运行 Rust 核心测试 |
| `cargo run -p agenthub-cli -- --help` | 查看 CLI 帮助 |
| `pnpm check:docs` | 检查文档链接、元数据、标题锚点和过时术语 |

前端 backend 分层、mock 边界和 `invoke` 约束见 [架构文档](docs/architecture/overview.md)。完整验证矩阵见 [测试文档](docs/reference/testing.md)。路由能力边界见 [路由兼容性](docs/reference/route-compatibility.md)，真实桌面验证见 [Adapter dogfood](docs/guides/adapter-dogfood.md)。

## 数据与隐私

AgentHub 默认只处理本机数据。常见数据位置是 `~/.agenthub/`（状态、设置、日志和备份）与 `~/.agents/skills/`（共享 Skills）。Usage 只读解析本地会话或日志，不通过代理截取请求，也不上传云端；本机路由不记录请求或响应正文。

凭据沿用项目现有的本地存储方案，界面、CLI 和日志输出会脱敏。凭据落盘加密不在当前产品范围内。发布截图、测试数据、版本发布和禁止提交项见 [隐私与发布](docs/reference/privacy-and-release.md)，漏洞披露方式见 [安全策略](SECURITY.md)。

## 开发与文档

- [贡献指南](CONTRIBUTING.md)：分支、开发环境、验证、PR 和发布流程。
- [项目约定](AGENTS.md)：架构红线、产品范围和协作要求。
- [文档索引](docs/README.md)：按用途进入设计、实现、运维和历史文档。
- [文档状态](docs/STATUS.md)：当前实现事实与未实现边界。
- [文档风格](docs/STYLE.md)：元数据、分类、链接和维护规则。

日常开发和 PR 使用 `dev` 分支。正式发布在 `dev` 升版并填写 `CHANGELOG.md`，合入 `release` 后在 `dev` 打 `vX.Y.Z` tag 触发 CI；详见 [贡献指南](CONTRIBUTING.md)。

## 许可证

本项目采用 [MIT License](LICENSE)。用量解析与配置切换部分借鉴了 [ccusage](https://github.com/ccusage/ccusage) 和 [cc-switch](https://github.com/farion1231/cc-switch)。
