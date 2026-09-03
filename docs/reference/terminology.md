---
title: 术语表
description: AgentHub 用户界面、领域模型和内部实现术语的对应关系。
type: reference
audience: all
status: current
updated: 2026-08-31
---

# 术语表

用户文案和代码名有意分开。新增文档、页面和错误消息优先使用“用户界面术语”。

| 用户界面术语 | 内部/代码术语 | 含义 |
|---|---|---|
| Agent | `AgentId` / `AgentKey` / `AgentAdapter` | 被 AgentHub 检测、安装、配置或运行的第三方 CLI/runtime。开放注册表与新 TypeScript 契约优先 `AgentKey`；`AgentId` 为兼容别名 |
| 登录 | account / credential / Ticket（内部） | 用户可选择的授权或 API key 记录。连接页与连接池各自管理自己添加的登录；从连接分享到池里的登录仍归连接页。实现中仍可能出现 Ticket，但 UI 不说“票” |
| 官方登录 | oauth / 「官方登录」入口 | 浏览器或设备码订阅登录，与 API Key 分行保存 |
| API Key | api_key / 「添加 API Key」 | 钥匙登录；本机若是官方登录则引导改用「导入授权」 |
| 相关文件 | credential files | 登录详情里记下的配置/登录文件（打码后可复制、打开所在目录） |
| 保留本机配置副本 | `keepLiveFileCopies` | 切换/导入时把各家本机配置拷到备份目录；默认开启 |
| 供应商 | provider | 某 Agent 的 API 端点、模型和相关配置记录 |
| 路由 / Routes | adapter profile / route | 管理本机转发 listener 的产品表面；也可添加仅用于连接池的官方登录 / API Key |
| bridge | `local_bridge` / in-process Gateway | Routes 的内部协议转换实现；不作为普通用户功能名 |
| 直接配置 | `native_endpoint` | 将来源端点写入目标 Agent 自己的配置 |
| 写进对方认的登录 | `config_sync` | 将登录投影到目标 Agent 能识别的配置/登录契约 |
| 本机转发 | `local_bridge` | 通过 loopback listener 转发或转换协议 |
| 入口 Key | Hub token / local bearer（内部亦称本机令牌） | 默认池给目标客户端用的本机钥匙；不等于上游 API Key。增删池内登录不改这把 Key。界面与子导航统一说「入口 Key」，不说泛称「令牌」 |
| 连接池 | default RoutePool | Routes 里列出本机转发所用登录的页面，与连接页相互独立；每个目标 Agent/surface 一个默认池 |
| 分享至连接池 | `syncConnectionAuthorizations`（按这份登录） | 连接页把这份登录加入默认连接池；登录仍留在连接页。API Key 都可以加入（不按所属 Agent 挡掉）；国产官方登录不能分享。已经在池里、或这份登录不能加入时按钮禁用 |
| 从连接同步 | `syncConnectionAuthorizations`（多选） | 连接池页一次加入连接页里可分享的登录（所有 API Key；Claude / Codex / Grok 官方登录仍按已登记的接法）。已经在池里的会跳过。国产官方登录不进入候选 |
| 登录回收站 | `connection_trash` `home=connections` | 连接页删除的登录，保留 30 天，可恢复到连接页 |
| 连接池回收站 | `connection_trash` `home=route_pool` | 连接池移出的登录或成员关系，与连接页回收站分开 |
| 默认池 | default RoutePool | 每个目标 Agent/surface 一个连接池；Routes 日常只展示默认池 |
| 能力 | `Capability` | adapter 对某操作的静态声明，不等于安装状态 |
| 完整 | `Full` | 能力已经接入且契约完整 |
| 部分支持 | `Partial` | 可用但有降级，必须提示 |
| 不支持 | `Unsupported` | 对方契约不存在或明确不支持 |
| 计划中 | `Planned` | AgentHub 尚未接入，不得假装可用 |
| live 配置 | live files | 第三方 Agent 实际读取的配置/登录文件 |
| 数据目录 | `data_dir` / `AGENTHUB_HOME` | AgentHub 自己的 SQLite、备份、日志和缓存根目录 |
| 共享技能真源 | `~/.agents/skills` | 用户技能的共享库；各 Agent 可有自己的投影目录 |
| 项目技能 | `<工作区>/.agents/skills` | 只作用于该项目的技能；Skills 页按项目列表里已识别的工作区选择 |
| 插件 | 各家 `plugin` / `extension` 包（Claude `/plugin`、Codex `/plugins`、Grok `plugin`、Pi `pi install`） | 可安装的发行单元，常含 skills/commands/hooks，有时附带 MCP。**不是** MCP server 条目，也不是 Skills 页 |
| MCP | MCP server 条目；`/mcp` 只读 inventory | Agent 作为客户端连接的外部工具；`Capability::Mcp` 仍为 Planned |
| mock backend | `src/dev/mocks` | 仅供 `pnpm dev:mock` 和 Vitest 使用的浏览器实现 |
| Tauri adapter | `src/lib/backend/tauri` | 生产桌面 backend；唯一允许直接 `invoke` 的前端边界 |
| fixture | test fixture | 脱敏、固定、最小的测试输入，不是用户数据备份 |

## 书写规则

- 页面、菜单和用户提示用“登录”“Routes/路由”“供应商”。
- 架构和代码说明可以写 `Ticket`、`Binding`、`bridge`，但首次出现应解释它们与用户术语的关系。
- `local_bridge` 不等于所有 adapter；它只是三种 route 之一。
- `Capability::ModelSelect` 不等于 Routes 的 `/v1/models`；后者是本机默认池当前可服务模型的并集。
- 凭据落盘沿用当前实现方案；不要在文档中引入 keyring、AES、主密码或国产 OAuth 转 API 计划。

