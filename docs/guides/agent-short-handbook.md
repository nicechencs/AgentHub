---
title: 自动化 Agent 短手册
type: guide
status: current
owner: maintainers
audience: automation agents and new contributors
updated: 2026-09-15
---

# 自动化 Agent 短手册

新会话先读本页，再按根 [AGENTS.md](../../AGENTS.md) 的「按任务读取」打开**一页**相关文档。不要把提案抄进 [STATUS](../STATUS.md)。

## 禁止（hard bans）

- 日常改动只进 **`dev`**。不要在 `release` 上提交。
- 不要做凭据落盘加密（keyring / AES / 主密码 / 密文迁移）。
- 不要做国产官方登录分享、OAuth 转 API、或把官方登录伪装成 API Key。API Key 都可以分享；国产官方登录不能分享。
- 不要做公网多人网关、动态插件 ABI、第二套领域库、Agent 互调、跨 Agent 搬运原生会话。
- 不要把伪终端当对话底座。不要把未出现在协议目录里的斜杠项画进 `/`。
- **仅** `src/lib/backend/tauri/` 可以 `invoke`。生产构建禁止静默回退 mock。
- 对用户、界面、提示：用登录、连接、路由、供应商、共享库、会话。不要写票 / 凭据 / 真源 / live / Adapter。

## 该读哪份

| 你在做 | 打开 |
| --- | --- |
| 改 Chat 会话是谁、何时新建/续聊 | [会话身份](../concepts/chat-session-identity.md) |
| 改过程面板 / 用量小字 / 事件种类 | [过程事件](../concepts/chat-process-events.md) |
| `/` 菜单、ACP 目录、允许/拒绝 | [Chat 与 Agent](../concepts/chat-and-agents.md)、[STATUS](../STATUS.md)；加深切片见 [Chat 宿主加深](../proposals/chat-host-depth.md)（提案，剩余边界不是现行 Full） |
| 验证命令 | [测试与验证](testing-and-validation.md) |
| 新文档 | [文档索引](../README.md)、[文档规范](../STYLE.md) |
| 提交 / PR | [贡献指南](../../CONTRIBUTING.md) |

完整红线与协作表仍在根 [AGENTS.md](../../AGENTS.md)。本页不替代 [产品边界](../decisions/product-boundaries.md)。
