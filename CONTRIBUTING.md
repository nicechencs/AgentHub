# 贡献指南

感谢参与 AgentHub。先阅读根目录 [AGENTS.md](AGENTS.md) 的红线，再按本页准备开发环境、验证改动并提交 PR。

## 开发环境

需要 Node.js LTS、pnpm、Rust stable、Git，以及 Tauri 在当前操作系统所需的桌面库。安装依赖后可用以下命令选择工作模式：

```bash
pnpm install
pnpm dev              # 普通 Vite 前端开发服务器
pnpm dev:mock         # 浏览器 mock 演示
pnpm tauri:dev        # 真实 Tauri 桌面端
```

运行桌面端前请确认当前 shell 能找到 Rust、Node、pnpm 和 Tauri CLI；Linux 依赖检查见 `scripts/check-linux-prereqs.sh --print-packages`。

## 分支与 PR

- 日常开发分支是 `dev`。从 `dev` 创建短期工作分支，PR 的目标分支是 `dev`。
- `release` 是发版成功后的同步线，不用于日常集成；发版闭环前不要提前把 `dev` 合入 `release`。
- PR 描述应说明行为变化、影响范围、验证命令和文档变化。涉及界面时使用 `pnpm dev:mock` 的合成数据，不提交真实账号、令牌、路径或日志。
- 安全问题不要创建公开 Issue；按 [SECURITY.md](SECURITY.md) 私下披露。

## 验证

风险分级以 [AGENTS.md](AGENTS.md) 为准，命令选择见 [测试与验证](docs/guides/testing-and-validation.md)。日常改动先跑过滤测试；提交前按改动面从下列命令中选择，CI 执行完整矩阵：

```bash
pnpm typecheck
pnpm typecheck:test
pnpm test
pnpm test:e2e:browser
pnpm build
cargo test -p agenthub-core --locked
pnpm check:docs
```

Rust CLI 或 GUI 改动分别补跑 `cargo test -p agenthub-cli --locked` 和 `cargo test -p agenthub-gui --locked`。不要为页面或纯函数改动默认编 GUI crate 或跑全部 Rust crate。前端 backend 分层、mock 边界和 CI 矩阵见 [测试参考](docs/reference/testing.md)。失败时保留失败用例和原始错误，不要通过削弱测试来绕过问题。

## 文档变更

文档是实现事实、产品决策和历史记录的入口。新增或重写文档时：

1. 先在 [docs/README.md](docs/README.md) 选择目标类别，避免再创建没有归属的平铺说明。
2. 按 [docs/STYLE.md](docs/STYLE.md) 添加 `title`、`type`、`status` 和 `updated` 元数据。
3. 更新实现事实时同步 [docs/STATUS.md](docs/STATUS.md)；改变命令、路径、界面或产品边界时同步对应参考文档。
4. 完成的一次性方案和旧排期移到 `docs/archive/`，不要把归档内容当作当前任务清单。
5. 提交前运行 `pnpm check:docs`，修复本地链接、标题锚点、元数据和历史术语检查错误。

## 发布流程

正式发布由推送 `v*` tag 的 GitHub Actions 完成，本地发布命令不会上传发行物。顺序是：**先在 `dev` 打 tag 并闭环，再合 `release`**。

1. 在 **`dev`** 准备发布提交：同时更新 `package.json`、`Cargo.toml` 的 `[workspace.package]`、`src-tauri/tauri.conf.json` 和 `Cargo.lock` 中的 workspace 版本。
2. 运行发版预检：`pnpm release:preflight`（或至少 `pnpm release:check`、`pnpm typecheck:test`、`pnpm test`、`cargo test --workspace --locked`）。
3. 提交并推送 `dev`。
4. 在 **`dev` 的对应提交**上创建并推送匹配的 `vX.Y.Z` tag。Release workflow 只接受 tag 落在 `origin/dev` 上的提交。
5. 等待 GitHub Actions Release workflow 完成构建与发布；若失败，在 **`dev`** 修复后重新闭环：
   - 尚未对外发布的 tag 可以删除并重建；
   - 已发布的 tag 不可覆盖，需用新的 patch 版本继续。
6. 确认 GitHub Release 已发布且资产齐全后，将 **`dev` 合入 `release`**，同步发版线。

GitHub Actions 会校验版本一致性、tag 是否在 `dev` 上、以及发布元数据，并生成 Windows、macOS 与 Linux 发行物。日常 PR 不应修改 `release` 或手工上传发行物。
