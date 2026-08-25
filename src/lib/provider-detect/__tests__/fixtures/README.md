---
title: Provider Detect Fixtures
description: provider-detect 单元测试样例的边界、占位值和扩展方式。
type: reference
audience: contributor
status: current
updated: 2026-08-25
---

# Provider Detect Fixtures

本目录仅供 Vitest 回归测试，生产入口不会导出或加载这些样例。fixture 表示用户可能粘贴的**形态**，不表示任何真实用户配置。

## 安全边界

- 所有 URL、API key、token、邮箱、用户名和本机路径必须是明显的占位值，例如 `https://api.example.test/v1`、`sk-test-placeholder`、`C:\mock\agent`。
- 不得复制真实账号、真实 OAuth token、真实 bearer、真实 home 路径或可用服务端点。
- 占位 key 不得通过正则意外匹配为生产秘密；测试只验证识别结构、来源和脱敏摘要。
- fixture 不会写入用户数据目录，也不应触发网络请求或真实命令。

## 内容约定

当前样例覆盖环境变量、shell export、`settings.json`、`config.toml` 等用户提供形态。每个样例应尽量小，并保留能说明格式的字段；无关字段和真实业务内容不要带入。

新增样式时：

1. 在相应 `*-samples.ts` 添加命名常量；
2. 挂到该文件导出的 `*_SAMPLES` 集合；
3. 在 fixture test 中增加正向和负向断言；
4. 若包含凭据形态，增加脱敏断言；
5. 运行相关 Vitest 和 `pnpm typecheck:test`。

测试代码与生产 provider-detect 代码分文件；不要在生产模块里添加 `__reset*ForTests` 或 fixture 分支。
