---
title: release 分支保护
description: 确保 release 只接收来自 dev 的合并，禁止直接在 release 上提交。
type: guide
status: current
owner: maintainers
updated: 2026-08-27
---

# release 分支保护

`release` 是正式发版线。除推送版本 tag 外，**所有提交必须来自 `dev` 的合并**，不能在 `release` 上直接改代码或升版本号。

## 仓库内已启用的自动检查

工作流 [`.github/workflows/release-branch-guard.yml`](../../.github/workflows/release-branch-guard.yml) 会在以下情况运行：

| 事件 | 规则 |
|---|---|
| 向 `release` 开 PR | 只允许 **源分支 = 本仓库的 `dev`** |
| 直接 push 到 `release` | 新增提交必须已在 `dev` 上，或为把 `dev` 合入 `release` 的 merge commit |

因此正确做法是：

1. 在 **`dev`** 升版、跑预检、推送。
2. 用 PR 或 merge 把 **`dev` 合入 `release`**（不要用 squash，以免 guard 无法识别来源）。
3. 在 **`release` 的合并结果**上打 `vX.Y.Z` tag。

## 仓库管理员需手动开启的 GitHub 设置

Cloud Agent 没有权限直接改 GitHub 分支保护，需要仓库管理员在 GitHub 上完成一次配置：

1. 打开 **Settings → Rules → Rulesets → New branch ruleset**。
2. 目标分支：`release`。
3. 启用：
   - **Restrict deletions**
   - **Block force pushes**
   - **Require a pull request before merging**（合并 `dev` → `release` 仍建议走 PR）
4. 在 **Require status checks** 中添加：
   - `PR source must be dev`
   - `Push must come from dev merge`
5. 保存并启用 ruleset。

也可以导入模板 [`.github/rulesets/release-from-dev.json`](../../.github/rulesets/release-from-dev.json)（GitHub UI 或 `gh api repos/OWNER/REPO/rulesets --input .github/rulesets/release-from-dev.json`，需管理员 token）。

## 常见问题

**为什么不在 `release` 上直接升版本？**  
Guard 会把这类提交视为「非 dev 来源」而拒绝。版本号应在 `dev` 改好，再合入 `release`。

**可以用 squash merge 吗？**  
不建议。Squash 会产生一个不在 `dev` 历史上的新提交，push guard 会失败。请用 **merge commit** 或 **fast-forward**。

**打 tag 算不算改 `release`？**  
Tag 是发版触发器，不增加分支上的新提交，不受 push guard 限制。
