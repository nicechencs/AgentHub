---
title: Accounts 与 Authorization Pool
type: explanation
status: current
owner: maintainers
audience: product, core, and connection UI contributors
source-of-truth: AccountService, Account/LiveAccount models, adapter authorization hooks, and ConnectionService
updated: 2026-09-03
---

# Accounts 与 Authorization Pool

## 两个不同概念

| 概念 | 含义 | 例子 |
| --- | --- | --- |
| Identity | “是谁”的稳定标识 | email、user_id、sub、principal_id |
| Authorization | 一次登录拿到的凭据集合 | refresh/access token 或一把 API Key |

用户界面把二者都呈现为一份“登录”，但去重和刷新必须区分它们。`Account` 是存储授权的一行；`LiveAccount` 是 adapter 在应用 live 文件时使用的临时快照；未脱敏的 credentials 不能返回给 UI 或写日志。

## Pool 规则

- 同一 Agent + 同一稳定 OAuth identity 只保留一行；重新登录覆盖 credentials/label/updatedAt。
- 跨 Agent 的同一 identity 各自保留一行，不能跨 `agent_id` 合并。
- API Key 按密钥指纹分行；不同 Key 不因展示名相同而合并。同一把钥匙、同一地址保存成两张登录时，也不会悄悄把其中一张送进回收站。官方登录与 API Key 永远分行，不会合成一张。
- WorkBuddy 自定义模型按 `models.json` 一行一份登录；ZCode 按 `~/.zcode/v2/config.json` 的一条供应商分行。桌面套餐登录不导入。
- Cursor 没有稳定的本机登录文件：登录可以留在登录列表，但不能写回 Cursor 本机配置；切换失败给出中文说明。Kimi 切换会写出带模型表的完整 `config.toml`。
- identity 不明确时 fail closed，不根据 label、token preview 或猜测合并。
- Pi 还要按官方 live slot 区分；同一人位于不同 provider 槽仍是不同账号行。
- 每个 Agent 的 live 生效位最多一条；切换不会删除池内其他授权。
- `local_bridge` 的默认池按目标 Agent/surface 唯一；本机口和本机令牌更新覆盖。池内可以有多份合格登录。从连接页入池不复制登录，也不把本机令牌当成新的登录；这些登录仍出现在 Connections。在连接池里编辑这份官方登录并保存时，会复制成连接池自己的一份（连接页那份还在），保存后可选择把模型写回连接页。连接池本页添加的登录只给连接池用。两边回收站分开。

Adapter 的 `authorization_key` 用于识别同一授权（通常是 token/key hash）；OAuth 同身份覆盖另由 service 使用稳定 identity label/字段判断。不要把 email 当 authorization key，也不要把 capability 枚举当去重规则。

## Live 与绑定

导入 live：以 Agent adapter 能识别的当前 credential family 为准；同时存在 API Key 与官方登录时，报告 `alsoPresent` 供用户确认，但不把两族合成一张登录。切换 live：备份、写入目标 Agent 的官方文件、更新 current/binding，并保留池中其他行。登录记下关键词和整份配置；详情列出相关文件（打码后可复制、打开所在目录），不含明文钥匙。接到某个工具从 Dashboard「连接/切换」，写入仍走 [bind](connections-and-routing.md)；连接页把登录「分享至连接池」。**API Key 都可以分享**（含 WorkBuddy / ZCode 等上配置的）；**国产官方登录不能分享**。不能把目标的生成配置再导入为新登录。

刷新归属遵循谁拥有登录文件谁续期：目标 CLI-owned OAuth 由目标工具刷新，AgentHub 只重新读取；Hub-owned grant 才由 AgentHub 按 account 行做 single-flight。跨进程或并发写入要依赖 revision/lock，不以最后一次列表刷新覆盖另一份新凭据。

## 数据与安全边界

当前凭据使用项目既有存储方案，**不做额外的凭据落盘加密**。这不是遗漏的实现任务，除非产品决定被明确推翻，否则不引入 keyring、AES、主密码或密文迁移。

服务返回和日志只允许脱敏摘要、指纹、尾部预览或 source/revision 等非 secret 信息。生产前端不把完整 credentials 放入 Ticket/Binding DTO；Adapter profile 和 generated Provider 只保存 Connection 引用。

## 国产 OAuth 边界

中国产 AI 的 OAuth（包括 Kimi CLI 会员 OAuth，以及 GLM、DeepSeek、通义、豆包等）不开放跨 Agent adapter 边，也不转换为 API，也不可分享至连接池或接到其他工具。国产路由只认产品明确支持的官方 API Key；这属于产品边界，不是“稍后补 writer”的 roadmap。

所有 API Key 都可以分享。入池和接到其他工具看的是「这是 API Key」，不是「这份登录挂在哪个 Agent」。WorkBuddy / ZCode 上配置的 Key 与其它工具上的 Key 同一条规则。详见 [产品边界](../decisions/product-boundaries.md)。

## 相关页面

- [Connections and routing](connections-and-routing.md)
- [Product boundaries](../decisions/product-boundaries.md)
- [Legacy document index](../archive/legacy-document-index.md)
- [Testing reference](../reference/testing.md)
