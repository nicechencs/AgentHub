---
title: 隐私与发布边界
description: 开源发布、截图、测试数据、OAuth 配置和漏洞披露的安全边界。
type: reference
status: current
owner: maintainers
updated: 2026-08-31
---

# 隐私与发布边界

本文适用于提交、Pull Request、Issue、截图、测试 fixture 和对外文档。默认假设仓库内容会被公开复制；任何只在本机可见的资料都必须先脱敏或不提交。

## 禁止提交

| 类别 | 示例 |
|---|---|
| AgentHub 本机数据 | `~/.agenthub/`、`agenthub.db`、SQLite WAL、备份、导出包、session 索引 |
| 运行时缓存 | `project_session_index.json`、`project_metadata.json`、临时下载和日志目录 |
| 凭据与环境文件 | `.env`、`*.pem`、`auth.json`、`credentials.json`、OAuth token、API key、bearer |
| 用户识别信息 | 邮箱、用户名、真实 home 路径、组织名、订阅/账单信息 |
| 本地审计和工作区 | `audit/`、`.codegraph/`、`.grok/`、`.pi-subagents/` 及其他本地 agent 状态目录 |
| 第三方私有细节 | 未公开的凭据文件清单、IDE 私有库、反编译/解包路径、本机实测体积和版本记录 |

`.gitignore` 只能作为第二道防线；发现敏感文件时先移出提交范围并轮转已经暴露的凭据，不要依赖“文件被忽略”来继续使用真实值。

## 文档、PR 和提交消息

可以写：产品能力、架构分层、Adapter/Service 边界、公开的产品路径（例如 `~/.agenthub`、`~/.agents/skills`）和脱敏的错误码/耗时。

不要写：完整的第三方凭据落点清单、OAuth `client_id`/回调端口/端点抄录、未公开的协议逆向细节、真实网络请求或带账户信息的日志。路径与解析细节以 adapter 源码为准，不在公开文档维护易漂移的秘密清单。

能力“能不能”以 [能力参考](capabilities.md) 和 `agenthub agent capabilities` 为准；Route 兼容性以 [Route compatibility](route-compatibility.md) 的源码快照和矩阵真源为准。

## 截图和屏幕录制

1. Skills、Connections、Routes 等演示优先使用 `pnpm dev:mock`，让路径显示 `C:\mock\...`、`/tmp/mock/...` 或明确的占位符。
2. 使用真实桌面截图时，先裁掉账号列表、邮箱、订阅/账单、组织名、浏览器地址栏和系统通知；不得只用模糊或打码后仍可推断的原图。
3. 检查 Windows、macOS、Linux 路径中的用户名、home、工作区和磁盘卷标；检查日志面板、终端和环境变量是否露出 token。
4. 第三方产品截图必须裁掉账号、账单和邮箱区域。仓库不存实机账户截图，功能演示使用 mock 状态或占位文本。
5. 提交前用图像外的文本搜索再次检查文件名、OCR 文本、Markdown alt 文本和 PR 描述，不要把真实路径留在图片说明中。

## 测试数据和 fixture

- 使用最小、合成、可重复的样例；fixture 只描述配置形态，不承载真实账户状态。
- URL 使用 `https://api.example.test/v1` 等保留域名；key 使用 `sk-test-placeholder`、`token-placeholder` 等不可用值；路径使用 `C:\mock\agent` 或 `/tmp/mock/agent`。
- 单元测试不得访问真实供应商、真实用户 home 或真实凭据文件；网络行为用本地 loopback server、fake executor 或固定响应。
- 测试报告只记录 `profile_id`、`request_id`、rule id、错误码、状态、耗时和通过/失败，不记录 Authorization、prompt、工具参数、响应正文或 query 中的秘密。
- 发现 fixture 含真实值时，立即从工作树和历史提交中隔离并轮转，不通过“继续脱敏后提交”掩盖已经暴露的凭据。

Provider detect fixture 的具体写法见 [Provider Detect Fixtures](../../src/lib/provider-detect/__tests__/fixtures/README.md)。

## OAuth 常量

`crates/agenthub-core/src/oauth/providers.rs` 是各 CLI 的 public client 运行时真源（PKCE 或设备码），没有 `client_secret`。这些值仍属于实现配置：

- 不要把 `client_id`、回调端口、授权端点、token 端点或完整 OAuth payload 抄到 README、Issue、PR 或对外方案；
- 不要在日志、截图、fixture 或错误消息中记录 authorization code、access token、refresh token 或 cookie；
- 修改 OAuth 配置时只引用源码路径和行为，不复制可被误用的运行时常量。

国产 OAuth 适配和 OAuth 转 API 不在产品范围内；文档不得把它们列为计划、风险或实现建议。

## 漏洞披露

安全问题包括凭据泄露、任意文件读写、路径穿越、命令注入、错误的 loopback 暴露和日志脱敏失效。报告渠道、支持版本和披露范围以根目录 [SECURITY.md](../../SECURITY.md) 为准。

不要在公开 Issue、PR 或聊天中贴复现细节、真实凭据、完整日志、可利用 payload 或用户数据。公开讨论只保留不敏感的影响摘要；复现材料按 SECURITY.md 的私下渠道发送。

## 开源致谢

上游项目的 attribution、许可证和链接统一保留在根 [README.md](../../README.md) 及仓库的许可证文件中。实现注释和文档避免逐文件“对齐某仓库”的对照叙述；需要说明来源时写公开项目名、许可证和官方链接，不复制上游私有实现或受限资料。

