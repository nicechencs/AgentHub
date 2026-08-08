# AgentHub

多 Agent 桌面管理中枢：统一检测/安装运行时与 Agent、管理 Provider 与账号、Skill 投影、备份、Usage 与 Chat（Tauri v2 + React + Rust core）。

面向 Claude Code、Codex、Grok、Kimi、Pi、WorkBuddy、Cursor Agent 等本机 AI Agent，提供安装引导、配置切换、账号池、技能投影、用量统计与对话入口。

## 快速启动（Windows）

双击根目录 **`run.bat`**（推荐），或在 PowerShell 中：

```powershell
.\run.ps1
```

等价命令：

```powershell
pnpm install
pnpm tauri:dev
```

需要已安装 [Node.js](https://nodejs.org/)、[Rust/Cargo](https://rustup.rs/)。`run.bat` / `run.ps1` 会在缺少 pnpm 或 `node_modules` 时自动补齐。

## 常用命令

| 命令 | 说明 |
|---|---|
| `pnpm tauri:dev` | 桌面端（真实 Tauri 后端） |
| `pnpm dev:mock` | 浏览器 mock（无 Tauri） |
| `pnpm test` | 前端单测 |
| `pnpm build` | 前端生产构建（强制 Tauri adapter） |
| `cargo test -p agenthub-core` | Rust core 测试 |
| `cargo run -p agenthub-cli -- --help` | CLI |

## 仓库结构（摘要）

```text
AgentHub/
├── run.bat / run.ps1     # 本机一键启动（保留在根）
├── AGENTS.md             # 项目约定 + Agent 协作规则（真源）
├── agent.md              # 兼容入口 → 指向 AGENTS.md
├── Cargo.toml            # Rust workspace
├── package.json          # 前端 (pnpm)
├── crates/               # agenthub-core / agenthub-cli
├── src-tauri/            # Tauri GUI 壳
├── src/                  # React 前端
├── docs/                 # 当前有效的设计、规范与状态文档
└── scripts/              # 运维脚本
```

图标等桌面资源在 `src-tauri/icons/`；前端/Core 预设分别在 `src/config/presets/` 与 `crates/agenthub-core/src/presets/`。

## 文档

完整索引见 [docs/README.md](docs/README.md)。实现状态与**未实现清单**见 [docs/agenthub-plan.md §8](docs/agenthub-plan.md)。

- 架构与目录：[docs/architecture.md](docs/architecture.md)
- 产品方案：[docs/agenthub-plan.md](docs/agenthub-plan.md)
- 能力矩阵：[docs/capability-matrix.md](docs/capability-matrix.md)
- 发布与隐私：[docs/privacy.md](docs/privacy.md)
- 项目约定：[AGENTS.md](AGENTS.md)

## 致谢与开源借鉴

AgentHub 在配置切换、用量解析等方向上借鉴了下列开源项目，感谢原作者与社区的贡献。

| 项目 | 链接 | 主要借鉴方向 |
|---|---|---|
| **ccusage** | [github.com/ccusage/ccusage](https://github.com/ccusage/ccusage) | 会话/日志 Usage 解析策略、成本估算思路、解析健康度呈现 |
| **cc-switch** | [github.com/farion1231/cc-switch](https://github.com/farion1231/cc-switch) | 多应用配置切换、原子写与 backfill、SQLite 自管状态 |

以及其他相关开源项目。更细的设计说明见 [docs/agenthub-plan.md](docs/agenthub-plan.md)、[docs/architecture.md](docs/architecture.md)。
