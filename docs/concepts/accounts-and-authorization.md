---
title: Accounts 与 Authorization Pool
type: explanation
status: current
owner: maintainers
audience: product, core, and connection UI contributors
source-of-truth: AccountService, Account/LiveAccount models, adapter authorization hooks, and ConnectionService
updated: 2026-09-29
---

# Accounts 与 Authorization Pool

本页解释 AgentHub 怎样保存和去重“登录”。登录接到哪个 Agent、走哪条路线，见 [Connections、Routes 与绑定](connections-and-routing.md)；分享规则的原文见 [产品边界](../decisions/product-boundaries.md)。

## 身份与授权是两件事

| 概念 | 含义 | 例子 |
| --- | --- | --- |
| Identity（身份） | “是谁”的稳定标识 | email、user_id、sub、principal_id |
| Authorization（授权） | 一次登录拿到的登录信息 | refresh/access token，或一把 API Key |

界面把两者都叫一份“登录”，但去重和刷新必须分开看。代码里：`Account` 是存一份授权的一行；`LiveAccount` 是 adapter（各 Agent 的对接代码）读写本机配置时用的临时快照。未打码的登录信息不能返回给界面，也不能写日志。

## 去重规则

- 同一 Agent + 同一稳定 OAuth 身份只留一行；重新登录覆盖登录信息、名称和更新时间。
- 同一身份在不同 Agent 上各留一行，不跨 `agent_id` 合并。
- API Key 按密钥指纹分行；展示名相同不合并。同一把 Key、同一地址存成两份时，不会悄悄把其中一份送进回收站。官方登录与 API Key 永远分行。
- 身份不明确时 fail closed（拒绝合并），不按名称、token 预览或猜测合并。
- adapter 的 `authorization_key` 识别“是不是同一份授权”（通常是 token/Key 的哈希）；OAuth 同身份覆盖由 service 按稳定身份字段判断。不要把 email 当 `authorization_key`，也不要拿能力枚举当去重规则。

各家特例：

- WorkBuddy 自定义模型按 `models.json` 一行一份登录；ZCode 按 `~/.zcode/v2/config.json` 的一条供应商一份。两家的桌面套餐登录都不导入。
- Pi 按官方登录槽区分；同一人在不同 provider 槽里是不同的行。
- Cursor 可以把本机已有登录导入列表，但不能写回 Cursor；切换失败时给中文说明。
- Kimi 切换会写出带模型表的完整 `config.toml`。
- Kiro 支持官方登录（`kiro-cli`）、导入本机 sqlite 登录、再登一份并切换；详情可看官方积分。

## 本机正在用的配置

- **导入**：以 adapter 能识别的当前登录类型为准。同时存在 API Key 与官方登录时，返回 `alsoPresent` 让用户确认，不把两类合成一份。
- **切换**：先备份，再写目标 Agent 的官方文件，更新“当前使用”，池里其他行保留。每个 Agent 最多一份正在用的登录。
- **详情**：登录记下关键词和整份配置；“相关文件”列出路径（打码后可复制、可打开所在目录），不含明文 Key。
- 自动生成给目标 Agent 的配置不能再导入成新登录。

刷新归属：谁拥有登录文件谁续期。目标 CLI 自己的 OAuth 由它自己刷新，AgentHub 只重新读取；只有 AgentHub 自己持有的授权，才由 AgentHub 按行加锁刷新（同一行同时只刷一次）。并发写入依赖 revision/锁，不能用一次列表刷新覆盖别处刚写入的新登录。

## 连接页与连接池

连接页和 Routes 的连接池各自管理自己添加的登录，回收站也分开；从连接页同步到池里的登录不复制，仍出现在连接页。完整规则见 [Connections、Routes 与绑定](connections-and-routing.md#登录列表与-routes)。

## 数据与安全边界

- 登录信息沿用项目现有存储，**不做额外的落盘加密**；这是产品决定，不是遗漏的任务。
- 服务返回和日志只允许打码摘要、指纹、末几位或 source/revision 等非密钥信息。Ticket/Binding DTO 不带完整登录信息；adapter profile 和自动生成的 Provider 只保存对登录的引用。
- 所有 API Key 都可以分享；国产官方登录（Kimi 会员 OAuth、GLM、DeepSeek、通义、豆包等）不能分享、不能接到其他工具，也不转成 API。原文见 [产品边界](../decisions/product-boundaries.md#api-key-可分享国产官方登录不可分享)。

## 相关页面

- [Connections、Routes 与绑定](connections-and-routing.md)
- [产品边界](../decisions/product-boundaries.md)
- [测试参考](../reference/testing.md)
