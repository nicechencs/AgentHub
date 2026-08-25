# 安全策略

AgentHub 是一个本地桌面应用（Tauri + Rust core）。本页说明漏洞披露渠道、支持范围和有意的产品边界。

相关参考：[隐私与发布](docs/reference/privacy-and-release.md)、[路由兼容性](docs/reference/route-compatibility.md)、[Adapter dogfood](docs/guides/adapter-dogfood.md)。

## 支持版本

| 版本 | 支持 |
| --- | --- |
| [GitHub Releases](https://github.com/nicechencs/AgentHub/releases) 中的最新版本 | 是 |
| `dev` | 尽力支持 |
| 更早版本 | 没有固定支持窗口，请尽快升级 |

## 报告漏洞

不要在公开 Issue、Discussion 或 PR 中放置漏洞细节、利用代码、真实凭据、生产邮箱、完整日志或个人路径。

优先使用仓库的 [GitHub Private Vulnerability Reporting](https://github.com/nicechencs/AgentHub/security/advisories/new)。如果该入口不可用，只创建标题为 `Security: request private contact` 的最小公开 Issue，不要包含技术细节，等待维护者转到私下渠道。

报告中请提供：

- 受影响的版本或 commit，以及操作系统。
- 影响摘要，例如凭据泄露、任意文件读写或命令注入。
- 只使用合成数据的复现步骤。
- 可选的修复思路或临时缓解措施。

请不要提供真实 API key、OAuth token、session cookie、第三方 Agent 认证文件、`~/.agenthub/` 完整转储或含真实路径的截图。只影响第三方 Agent 且不需要 AgentHub 代码触发的问题，应同时向上游报告。

## 处理预期

| 阶段 | 目标 |
| --- | --- |
| 初次确认 | 尽力在 7 日内确认收到 |
| 严重性评估 | 完成复现后进行 |
| 修复、缓解与公开披露 | 在修复可用后协调进行，不承诺固定 SLA |

愿意署名的报告者会在修复说明中获得致谢，也可以要求匿名。

## 范围

### 范围内

AgentHub 自有代码造成的以下问题属于范围内：

- GUI、CLI、日志、备份导出或 IPC 泄露凭据或 token。
- 逃逸预期 live/backup allowlist 的路径穿越、任意文件读写。
- 调用安装脚本、Agent 或 shell helper 时的命令/参数注入。
- 本项目控制的应用更新链被篡改或伪造。
- 绕过破坏性写入确认等应用安全模型造成的权限提升。
- XSS 或不安全的 `invoke` 桥接，使页面获得未授权的原生能力。

### 范围外

| 主题 | 原因 |
| --- | --- |
| 缺少凭据落盘加密（keyring、AES、主密码） | 产品明确沿用现有本地存储方案；见 [隐私与发布](docs/reference/privacy-and-release.md) |
| 只有第三方 Agent 的漏洞 | 应向对应上游报告 |
| 社工、物理访问、已解锁的用户会话 | 不属于应用安全边界 |
| 没有现实本地影响路径的依赖 CVE | 通过正常依赖升级跟踪 |
| 单用户本机上因主动重负载造成的磁盘/CPU 耗尽 | 不属于本工具的单用户安全边界 |
| 能力矩阵中标为 Planned 或 Unsupported 的功能 | 尚未提供的功能不是漏洞 |
| 文档错误、UI 润色、功能请求 | 使用普通 Issue |

## 有意的产品边界

以下是设计决策，不应作为“缺少功能”的安全报告：

- 凭据使用现有本地存储方案，API、CLI 与日志输出脱敏；当前不规划 at-rest 加密。
- 用量统计只读解析本地 Agent 日志，不运行本地 MITM 代理。
- Skills 和配置写入遵循备份与路径 allowlist；发布截图和提交禁止项见 [隐私与发布](docs/reference/privacy-and-release.md)。

## 安全港

只要研究者善意披露、避免隐私伤害，并且不超出证明影响所需的最小利用范围，我们不会因该研究追究法律责任。

一般 bug、文档和功能请求请使用 [GitHub Issues](https://github.com/nicechencs/AgentHub/issues)。

---

## 中文摘要

- **渠道：** 优先 GitHub 私有漏洞报告；不可用时只创建「请求私信」的空 Issue，不公开复现步骤。
- **支持：** 最新 Release 和 `dev` 尽力支持；旧版本请升级。
- **范围内：** 本仓库代码导致的凭据泄露、任意文件读写、命令注入、更新链问题和不安全的前端到原生桥。
- **范围外：** 凭据落盘加密未实现、纯第三方 Agent 漏洞、物理/社工、无实际路径的依赖 CVE 和规划中的功能。
- **响应：** 尽力 7 日内确认；修复和公开披露协调进行，无硬性 SLA。
- **关联：** 仓库禁止提交项和截图规范见 [隐私与发布](docs/reference/privacy-and-release.md)。
