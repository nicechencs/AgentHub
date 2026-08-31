# 贡献指南

感谢参与 AgentHub。红线在根目录 [AGENTS.md](AGENTS.md)。本页是提交和发版步骤；Agent 已加载红线时，不要为了遵守本页再通读文档树。

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
- `release` 是正式发版线，不用于日常集成；**禁止直接在 `release` 上提交**，只能将 `dev` 合入 `release` 后再打 tag。守卫说明见 [release 分支保护](docs/guides/release-branch-protection.md)。
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

Rust CLI 或 GUI 改动分别补跑 `cargo test -p agenthub-cli --locked` 和 `cargo test -p agenthub-gui --locked`。不要为页面或纯函数改动默认编 GUI crate 或跑全部 Rust crate。查完整命令表或 CI 矩阵时再打开 [测试参考](docs/reference/testing.md)。失败时保留失败用例和原始错误，不要通过削弱测试来绕过问题。

## 文档变更

文档是实现事实、产品决策和历史记录的入口。新增或重写文档时：

1. 先在 [docs/README.md](docs/README.md) 选择目标类别，避免再创建没有归属的平铺说明。
2. 按 [docs/STYLE.md](docs/STYLE.md) 添加 `title`、`type`、`status` 和 `updated` 元数据。
3. 更新实现事实时同步 [docs/STATUS.md](docs/STATUS.md)；改变命令、路径、界面或产品边界时同步对应参考文档。
4. 完成的一次性方案和旧排期移到 `docs/archive/`，不要把归档内容当作当前任务清单。
5. 提交前运行 `pnpm check:docs`，修复本地链接、标题锚点、元数据和历史术语检查错误。

## 发布流程

正式发布由推送 `v*` tag 的 GitHub Actions 完成，本地发布命令不会上传发行物。顺序是：**在 `dev` 升版并写更新说明 → 合入 `release` → 在 `dev` 打 tag 触发 CI**。

1. 确认 **`dev`** 上待发版改动已合并完成。
2. 在 **`dev`** 准备发布提交：
   - **只改 `package.json` 的 `version`**，运行 `pnpm release:sync-version` 同步 Rust 侧版本（或使用 `pnpm release:bump` 自动升版并同步）；
   - 在 **`CHANGELOG.md`** 新增对应版本一节（至少一条 `-` 更新说明）。
3. 运行发版预检：`pnpm release:preflight`（或至少 `pnpm release:check --require-changelog`、`pnpm typecheck:test`、`pnpm test`、`cargo test --workspace --locked`）。
4. 提交并推送 **`dev`**。
5. 将 **`dev` 合入 `release`**（推荐 PR；使用 merge commit 或 fast-forward，不要用 squash）。
6. 在 **`dev` 的当前提交**（与 `release` 合并后指向同一 SHA）创建并推送匹配的 `vX.Y.Z` tag。
7. 等待 GitHub Actions Release workflow 完成构建与发布；若失败，在 **`dev`** 修复后重新从步骤 2 闭环：
   - 尚未对外发布的 tag 可以删除并重建；
   - 已发布的 tag 不可覆盖，需用新的 patch 版本继续。
8. 确认 GitHub Release 已发布、资产齐全、Latest 标记正确，且 Release 正文来自 `CHANGELOG.md`。

**关于 tag：** tag 绑的是提交 SHA，不是分支名。在 `dev` 打 tag 后，把同一提交合入 `release`，tag 无需在 `release` 上重打；推送 tag 前必须先完成 `dev` → `release` 合并。

GitHub Actions 会校验版本一致性、tag 是否同时在 `dev`/`release` 上、`CHANGELOG.md` 是否有对应版本说明，并生成 Windows、macOS 与 Linux 发行物。
