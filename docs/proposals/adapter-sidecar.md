---
title: Go 路由替换方案
type: proposal
status: proposed
owner: maintainers
updated: 2026-10-04
---

# Go 路由替换方案

本页说明如何用 `go/agenthub-adapterd` 逐步替换现有 Rust 路由运行模块。方案只解决路由运行替换，不建设通用扩展平台；每次只交付一个可运行、可检查、可恢复的纵向切片。

## 当前情况

- 默认本机路由仍由 Tauri 进程内的 `BridgeRuntimeHost` 提供。
- 登录、连接、路线选择、数据库和 Agent 配置写入仍由 core 管理。
- `go/agenthub-adapterd` 已有隔离运行切片：`Handshake`、`Start`、`Status`、`Stop`、`AcquireOrRenewOwner`、探测专用 `ActivateProbeListen`，以及合成 Key 的 Messages JSON/SSE。
- 隔离连接池已接上：`priority_failover` / `round_robin`、成员健康与冷却、模型并集、客户端取消；已开始输出后不换成员。探测见 `scripts/route-runtime-probe/pool-isolated.sh`。
- 隔离转发已接上 `POST /v1/responses`、`POST /v1/chat/completions` 和别名 `POST /chat/completions`（JSON、SSE、工具调用、取消、上游错误），并覆盖 Responses 入口到 OpenAI 兼容 Chat 上游的协议转换。入口格式由连接池明确指定，不根据请求正文猜测 Codex 或 Grok。探测见 `scripts/route-runtime-probe/protocols-isolated.sh`。
- core 已能从保存结果中筛选可用的 loopback 路由和官方 Anthropic API Key 的 Messages 路由，生成只在内存中使用的 Go 配置；应用侧隔离监督器通过子进程 stdin 交付配置，提供真实状态、退出排空和有限次数崩溃恢复。外网上游只放行该 Messages 路由的 Anthropic 官方 HTTPS 地址且不跟随重定向；它仍使用临时目录和非默认端口，不是默认本机转发。
- 四个 Go `scripts/route-runtime-probe/*-isolated.sh` 只使用临时目录、临时端口、合成 Key 和受控 loopback 上游；`existing-flow-isolated.sh` 另行检查多入口隔离、两种上游认证、排空和敏感信息扫描，不会访问真实 Anthropic 服务。`ticket-bind-saga-isolated.sh` 只在 scratch Claude 目录用合成登录运行 core bind/unbind 与持久化失败补偿，覆盖真实文件写入，不覆盖 Tauri 监督器、Go 进程、桌面端到端或真实上游。

现行功能仍以[本机路由 API](../reference/local-route-api.md)、[路由兼容性](../reference/route-compatibility.md)和 [STATUS](../STATUS.md)为准。

## 目标

Go 接管本机监听、协议转换和连接池运行。迁移期间避免为同一目标同时扩展两套实现；当前产品确有需要时，仍按实际问题处理。Go 覆盖当前实际使用范围后，生产代码再停止加载 Rust 路由运行模块。

迁移不改变登录、连接和路由的使用方式，不迁移数据库，也不改变 Agent 配置格式。

```text
页面 / CLI
    │
    ▼
core：路线选择、登录、连接、保存与恢复
    │  启动、配置、状态、停止
    ▼
agenthub-adapterd：监听、转发、调度、取消与运行状态
    │
    ▼
上游服务
```

## 职责

| 部分 | 负责 | 不负责 |
|---|---|---|
| core | `plan` / `bind` / `unbind`、登录刷新、连接池配置、数据库、Agent 配置写入和失败恢复 | HTTP 转发、协议转换和池内请求调度 |
| Go 路由程序 | loopback 监听、入口校验、协议转换、池内调度、流式响应、取消、健康和运行状态 | 读取领域数据库、修改 Agent 配置、决定产品支持范围 |
| Tauri | 组合 core，启动和停止 Go，把状态交给现有界面 | 复制路线选择或登录规则 |
| 页面 | 沿用现有 backend contract | 直接连接 Go 控制通道 |

所有 API Key 继续遵守现有共享规则；国产官方登录仍不能分享或转换成 API。Go 改造不扩大现有产品支持范围，也不新增登录信息落盘加密。

## 最小接线

产品接入首期只需要四个动作：

| 动作 | 作用 |
|---|---|
| `Handshake` | 确认程序版本和基本能力 |
| `Start` | 使用 core 给出的完整运行配置开始监听 |
| `Status` | 返回进程、监听、连接池和脱敏错误状态 |
| `Stop` | 停止接收新请求，排空后释放端口并退出 |

配置变化先采用停止后重新启动。首期不引入 prepare/commit、持久化租约、多控制方抢占或热更新。只有真实运行证明简单重启不能满足需要时，再为已经复现的问题补最小机制。

Go 由应用启动并随应用退出，首期只有一个控制方。Go 不获得数据库写入能力；登录信息只通过受保护的本机通道进入内存，不放进命令行、状态或日志。接入时选择满足目标平台的最小通信方式，不预建通用 RPC 框架。

现有 `ActivateProbeListen` 继续只服务隔离探测，不作为产品 `Start`。

## 交付切片

### 1. 应用控制 Go Messages

- 复用现有 Go 隔离切片。
- 接通应用对 Go 的启动、状态和停止。
- 使用测试连接池、临时目录和非默认端口。
- 完成一个 API Key 连接的 Messages JSON/SSE 请求。

完成标准：应用能明确显示启动、ready、停止和失败；Go 崩溃不修改登录、连接或 Agent 配置；停止后端口释放。

### 2. 补齐连接池运行

- 实现现行需要的优先级和轮询调度。
- 接入成员健康、冷却、配额提示、模型列表和客户端取消。
- 已开始输出的请求不自动换成员或重放。

完成标准：Messages 的主要连接池场景通过相同合成请求验证；core 或 Go 退出后不继续接受新请求。

### 3. 按实际使用迁移协议

依次实现当前路线需要的 Responses 和 Chat Completions。每个协议单独合入，分别检查状态码、必要响应头、流式事件顺序、工具调用结构、取消和上游错误。

Responses 到 OpenAI 兼容 Chat 的隔离转换已完成受控 loopback 验证；真实 Kimi / OpenAI 上游仍需单独收窄 HTTPS 地址范围，不能借此放开任意外网地址。

协议差异按现行接口裁决，不照搬旧实现中的已知错误，也不为了 Go 改造新增路线。

### 4. 接入现有路由流程

- core 用现有保存结果生成 Go 运行配置。
- 接通自动启动、应用退出排空、崩溃后的恢复和现有状态展示。
- 先用测试 Agent 与非默认端口验证真实写入和恢复。
- 官方登录按现有路线逐项接入；刷新逻辑仍留在 core。

完成标准：现有界面和 Agent 配置不需要新增操作；失败时显示不可用，不静默回退到 mock 或其他路线。

### 5. 切换默认网关

正式切换以整个默认网关为单位：

```text
暂停新的路由写入
→ 排空并停止 Rust 监听
→ 确认默认端口已经释放
→ 用当前配置启动 Go
→ 检查状态和一条合成请求
→ 恢复路由写入
```

切换失败时停止 Go、确认端口释放，再使用切换前配置恢复 Rust，并显示明确错误。Rust 与 Go 不同时监听默认端口，也不长期按连接池拆成两套路由。

Go 覆盖当前开放协议并完成目标平台的打包、启动、退出和恢复验证后：

- 断开 `BridgeRuntimeHost` 的生产调用。
- 保留 core 的路线选择、登录、连接、保存和补偿逻辑。

删除 Rust 路由代码和迁移入口不作为前几个切片的验收条件。默认切换稳定后再单独清理，并为那次清理保留与风险匹配的回退方式；本方案现在不预设具体期限或机制。

## 每个切片如何检查

- Go 单元测试和协议契约检查。
- 使用合成 Key、合成请求和受控 loopback 上游的黑盒脚本。
- 接入保存结果时，用 `existing-flow-isolated.sh` 检查 stdin 配置、多入口隔离、两种上游认证和退出排空。
- 用 `ticket-bind-saga-isolated.sh` 在 scratch Claude 目录用合成登录检查 core bind/unbind、真实文件写入和持久化失败补偿；该证据不覆盖 Tauri 监督器、Go 进程、桌面端到端或真实上游。
- 真实进程的启动、停止、端口占用、取消和崩溃日志。
- 扫描状态、错误和日志，确认不包含 Key、登录信息或请求正文。
- backend contract 或界面 DTO 变化时运行相关 Vitest、contract test 和 typecheck。
- Rust 接线使用真实应用或 CLI 运行日志验证；按项目规则不编写或执行 Rust 测试。

跨层切片合入前只审查本次差异。检查失败只阻止当前切片，不要求提前补完后续协议或长期能力。

## 不在本方案内

- 插件商店、动态 ABI、通用 SDK、事件总线或第二套领域数据库。
- 把其他业务模块拆成独立进程。
- 只为迁移本身重构 Rust 路由运行模块。
- 首期不停机热更新、多控制方、复杂租约或两阶段提交。
- Go 直接写数据库、登录文件或 Agent 配置。
- 公网或多人网关。
- 新增国产官方登录路由或把 OAuth 转成 API Key。

## 下一步

切片 1「应用控制 Go Messages」、切片 2「补齐连接池运行」和切片 3 的同协议转发、Responses 到 OpenAI 兼容 Chat 转换已在隔离目录接通。切片 4 已完成一部分：保存结果生成运行配置、多入口 stdin 交付、现有状态展示、退出排空和有限次数崩溃恢复均已接通；官方 Anthropic API Key 的 Messages 上游已有严格白名单和不跟随重定向的请求策略，但未访问真实 Anthropic 服务；core 的 Claude 文件写入、解绑恢复和失败补偿已有 scratch 隔离进程证据。

切片 4 仍需把桌面监督器、Go 进程和测试 Agent 写入串成一条端到端验证，并按已有路线逐项接入官方登录刷新；Codex/Grok 外网上游、真实 Anthropic 服务调用和 Windows 控制通道也尚未验证。完成这些检查前不进入切片 5，默认本机转发仍是进程内 `BridgeRuntimeHost`。

## 相关页面

- [架构总览](../architecture/overview.md)
- [Core 与 Runtime](../architecture/core-runtime.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [产品边界](../decisions/product-boundaries.md)
- [本机路由 API](../reference/local-route-api.md)
- [测试与验证](../guides/testing-and-validation.md)
