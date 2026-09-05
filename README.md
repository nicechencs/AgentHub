# AgentHub

**简体中文** · [English](README.en.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D6.svg)](#下载)
[![Release](https://img.shields.io/github/v/release/nicechencs/AgentHub?label=version)](https://github.com/nicechencs/AgentHub/releases)

AgentHub 是一个本地运行的多 Agent 桌面工具。一个 GUI 和 CLI 管理 AI coding agent 的安装、登录与连接、Skills、用量和本地会话。由 Tauri v2、React、Rust 组成，支持 Windows、macOS 和 Linux。

## 能做什么

- 管理 Claude Code、Codex、Kimi、Grok、Pi、WorkBuddy、ZCode、Cursor Agent 和 DeepSeek Harness 等本机 Agent。Cursor Agent 默认不出现在侧栏和连接页，可在 Agents 里取消隐藏。
- 查看跨工具的登录与连接，优先直接写入目标工具配置；只有存在开放规则和受测协议转换时才使用本机路由，否则明确显示「当前不支持」，不会静默转发。
- 登录 Sub2API 站点，管理站点 API Key，并将可用 Key 导入已安装的 Agent。
- 管理共享 Skills、查看已发现的 MCP、浏览项目与会话。
- 在桌面端启动本机会话并查看流式过程，解析本地会话日志中的用量与成本估算。
- 通过 CLI 执行 `doctor`、`env`、`agent`、`provider`、`account`、`skill`、`usage`、`backup`、`run` 和 `config` 等操作。

页面右上角问号或 <kbd>F1</kbd> 可看当前页说明。当前实现边界见 [实现状态](docs/STATUS.md)，产品决策见 [文档索引](docs/README.md)。

## 界面

截图来自真实桌面端；邮箱、本机路径、API Key 等已遮挡。

**总览** — 各 Agent 是否就绪，以及从本机日志解析出的用量和成本估算。

![总览](docs/assets/screenshots/dashboard.png)

**连接** — 跨工具的登录列表：导入本机已有授权、用浏览器做官方登录，或添加 API Key。接到其他工具时优先直连；两边说的话不同、并且已有可用规则时才走本机路由，否则显示「当前不支持」。

![连接](docs/assets/screenshots/connections.png)

**Chat** — 在桌面里和已安装的 Agent 对话。先选 Agent 和工作目录再发送。

![Chat](docs/assets/screenshots/chat.png)

**Agents** — 安装或升级环境需要的软件，再按渠道安装或升级各 Agent。Cursor Agent 可在这里取消隐藏。

![Agents](docs/assets/screenshots/agents.png)

**Skills** — 用户技能放在共享库，可以启用到各工具；项目技能按工作区管理。也可以从技能市场安装。

![Skills](docs/assets/screenshots/skills.png)

**Projects** — 按 Agent 浏览本机项目和会话。可以打开目录、预览摘录，或在 Chat 里继续。

![Projects](docs/assets/screenshots/projects.png)

**设置** — 偏好（语言与外观、启动与关闭、侧栏是否显示路由 / 插件 / Sub2API）、本机数据目录与日志、备份、关于（版本与检查更新）。

![设置](docs/assets/screenshots/settings.png)

**本机路由** — 看板、连接池、入口 Key、监控。登录信息仍在连接页，需要时在连接池「从连接同步」。

![路由看板](docs/assets/screenshots/router-board.png)

**Sub2API** — 登录站点后按分组管理 API Key，并把可用 Key 导入已安装的 Agent。侧栏入口可在偏好中显示或隐藏。

![Sub2API](docs/assets/screenshots/sub2api.png)

**MCP** 目前只扫描并列出已经发现的项，不会安装，也不能在这里改各工具的设置。**插件**查看 Claude、Grok 和 Pi 已经装好的包；Claude 和 Grok 的包可以启用或停用，安装仍在各工具里做。

## 路线图

本机路由、插件以及类似的管理能力会继续补，目前标为**开发中**。

- **插件**：现在列出 Claude / Grok / Pi 的插件包；Claude 和 Grok 已装的可以启用或停用。没有安装按钮。
- **MCP**：现在只是只读扫描；写入和管理还没做。
- **本机路由**：已经能转发部分连接；混合供应商和部分协议转换仍在做，不要当成已经做完的万能路由。

更细的实现边界见 [实现状态](docs/STATUS.md)。

## 下载

从 [GitHub Releases](https://github.com/nicechencs/AgentHub/releases) 下载对应平台的安装包：

| 平台 | 安装包 | 说明 |
| --- | --- | --- |
| Windows | NSIS `.exe` 或 MSI | 安装包与 updater 已签名 |
| macOS | `.dmg` | updater 已签名；Apple 公证不作承诺 |
| Linux | `.deb` 或 AppImage | 安装包可以未签名；自动更新只在发布清单含签名 Linux 条目时启用 |

## 从源码运行

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
| `pnpm test` | 运行前端测试 |
| `pnpm build` | 运行生产前端构建，强制使用真实桌面后端 |
| `pnpm tauri:build` | 构建桌面安装包 |
| `cargo test -p agenthub-core --locked` | 运行 Rust 核心测试 |
| `cargo run -p agenthub-cli -- --help` | 查看 CLI 帮助 |
| `pnpm check:docs` | 检查文档链接、元数据、标题锚点和过时术语 |

完整验证步骤见 [贡献指南](CONTRIBUTING.md)。

## 数据与隐私

AgentHub 默认只处理本机数据。常见位置是 `~/.agenthub/`（状态、设置、日志和备份）、`~/.agents/skills/`（用户技能）与项目里的 `.agents/skills/`（项目技能）。用量只读解析本地会话或日志，不通过代理截取请求，也不上传云端；本机路由不记录请求或响应正文。

登录信息沿用现有本地存储，界面、CLI 和日志会脱敏。登录信息落盘加密不在当前产品范围内。发布截图、测试数据和禁止提交项见 [隐私与发布](docs/reference/privacy-and-release.md)，漏洞披露见 [安全策略](SECURITY.md)。

## 开发与文档

- [贡献指南](CONTRIBUTING.md)：分支、开发环境、验证、PR 和发布流程。
- [项目约定](AGENTS.md)：架构红线、产品范围和协作要求。
- [文档索引](docs/README.md)：按用途进入设计、实现、运维和历史文档。
- [文档状态](docs/STATUS.md)：当前实现事实与未实现边界。

日常开发和 PR 使用 `dev` 分支。正式发布在 `dev` 升版并填写 `CHANGELOG.md`，合入 `release` 后在 `dev` 打 `vX.Y.Z` tag 触发 CI。

## 许可证

本项目采用 [MIT License](LICENSE)。用量解析与配置切换部分借鉴了 [ccusage](https://github.com/ccusage/ccusage) 和 [cc-switch](https://github.com/farion1231/cc-switch)；路由部分借鉴了 [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI)。
