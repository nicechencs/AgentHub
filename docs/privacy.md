# 发布与隐私边界

面向开源/对外发布时，避免把**本机隐私、逆向细节、凭据落点清单**写进仓库或 issue。

## 禁止提交

| 类别 | 示例 |
|---|---|
| 本机数据目录 | `~/.agenthub/`、`agenthub.db`、session 索引 |
| 运行时缓存 | `project_session_index.json`、`project_metadata.json`（见 `.gitignore`） |
| 真实用户路径截图 | 含 `C:\Users\<你>`、邮箱、订阅账单区的 UI 截图 |
| 密钥与环境文件 | `.env`、`*.pem`、`auth.json` / `credentials.json` 实文件 |
| 本地审计目录 | `audit/`、`.codegraph/`、`.grok/` |

## 文档与 PR 文案

- **可以写**：产品能力、架构分层、Adapter/Service 边界、本产品路径（`~/.agenthub`、`~/.agents/skills`）。
- **不要写**：各第三方 Agent 的完整凭据文件清单、OAuth `client_id`/回调端口/端点抄录、IDE 私有库文件名、反编译/解包路径、本机实测体积与版本号。
- 能力「能不能」以 [capability-matrix.md](capability-matrix.md) / CLI `agent capabilities` 为准；路径与解析细节以 **adapter 源码** 为准，不在 docs 展开。

## 截图规范

- Skills / Connections 等演示优先 **`pnpm dev:mock`**，路径应出现 `c:\mock\…` 或占位符。
- 第三方产品截图裁掉账号、账单、邮箱区域（参见 `ui-experience-alignment` 对 Cursor 的裁剪说明）。
- 发现真实用户名路径：先脱敏再提交，或改用 mock 截图。

## OAuth 常量

`crates/agenthub-core/src/oauth/providers.rs` 中为各 CLI 的 **public client** 配置（PKCE，无 client_secret）。  
这些值属于运行时真源，**不要**再抄进 README、issue、对外方案文档。

## 漏洞披露

安全问题（凭据泄露、任意文件读写、命令注入等）的报告渠道与范围见根目录 [SECURITY.md](../SECURITY.md)。  
**不要**在公开 Issue 中贴复现细节或真实凭据。

## 开源致谢

对上游开源项目的 attribution 保留在根 [README.md](../README.md)；实现注释中避免逐文件「对齐某仓库」的对照叙述。
