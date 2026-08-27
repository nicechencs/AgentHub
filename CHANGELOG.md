# Changelog

本文件记录每个正式版本的更新内容。发版前必须在 **`dev`** 为对应版本新增一节，且至少包含一条 `-` 开头的更新说明；`pnpm release:check --require-changelog` 与 Release CI 会校验。

格式：

```markdown
## [0.0.0] - 2026-01-01

### 新增
- 说明

### 修复
- 说明
```

## [0.3.4] - 2026-08-27

### 发版与 CI
- 恢复 `release` 分支发版监控；tag 在 `dev` 打，合并到 `release` 后同一提交可触发 CI
- 新增 `release-branch-guard`：`release` 只接受来自 `dev` 的合并
- 发版必须填写本文件对应版本节；GitHub Release 正文取自此处

### 功能与修复
- Agents 列表支持分栏打开详情；dev 线默认软隐藏 Cursor Agent
- 修复 DeepSeek 新建路由健康检查与单条记录展示
- QA 批次：登录去重、Grok 回填、本机转发相关修复

## [0.3.3] - 2026-08-27

### 修复
- 固定 objc2 依赖到已发布的 0.3.2 lock 条目，修复 Linux 构建
- Release CI Linux job 固定到 ubuntu-24.04 runner

## [0.3.2] - 2026-08-26

### 修复
- Release CI 与 Linux 构建稳定性改进

## [0.3.1] - 2026-08-23

### 变更
- 早期 dest 集成线恢复版本
