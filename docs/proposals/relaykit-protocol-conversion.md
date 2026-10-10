---
title: RelayKit 协议转换参考
type: proposal
status: proposed
owner: maintainers
updated: 2026-10-09
---

# RelayKit 协议转换参考

本页只记录 [RelayKit `relaykit/v0.3.0`](https://github.com/QuantumNous/new-api/tree/relaykit/v0.3.0/relaykit) 是什么、能转哪些协议，以及它**不是**本仓库已经接入的能力。核对日为 2026-10-09。细节以该标签的 README 为准，不把本页当成第二份 API 手册，也不要改去看会变动的 `main`。

## 它是什么

RelayKit 是从 [QuantumNous/new-api](https://github.com/QuantumNous/new-api/tree/relaykit/v0.3.0) 抽出的独立 Go 模块，只做四种文本协议之间的请求、完整响应和流式事件转换：

- OpenAI Chat Completions
- OpenAI Responses
- Anthropic Messages
- Gemini `generateContent`

模块路径是 `github.com/QuantumNous/new-api/relaykit`，要求 Go 1.25.1 或更新。当前是 `v0.x`，小版本可以改公开 API 和数据结构。

它明确不做 HTTP 收发、SSE 读写、渠道选择、鉴权和计费。需要把 URL 或 data URL 变成字节时，必须由宿主注册媒体解析函数，否则转换报错。

## 和本仓库的关系

本仓库还没有依赖、调用或嵌入 RelayKit。本机转发的协议转换仍是现有实现；现行事实以 [本机路由 API](../reference/local-route-api.md)、[路由兼容性](../reference/route-compatibility.md) 和 [STATUS](../STATUS.md) 为准。Go 路由替换的范围见 [Go 路由替换方案](adapter-sidecar.md)，那份方案不因本页而改。

记录它只是为了以后讨论「要不要复用外部协议转换」时有一份共同基线。记录不等于批准接入，也不扩大现有路由支持范围。

不能把这个库直接嵌进本仓库来省掉自己的协议转换。它要求 Go 1.25.1，并直接依赖 `github.com/google/uuid`、`github.com/samber/lo`、`github.com/tidwall/gjson`、`github.com/tidwall/sjson`（测试还依赖 `testify`）。`go/agenthub-adapterd` 现在是 Go 1.22、没有第三方依赖。升语言版本和引入这些包都不是主要障碍；主要障碍是许可证，以及它覆盖的协议和本仓库已经在跑的路由并不重合。

本仓库已经自己实现、并且正在使用的转换，不该被这个库替换：

- Rust 本机转发：Responses、Messages、Chat Completions 三者互转；Codex Responses 与 Grok Responses 按独立开关互转。Kiro 只作为 Claude（Messages）和 Codex/Grok（Responses）的上游，把文本收成一轮提示再包回 JSON/SSE，不是完整的工具转换；没有 Chat Completions 到 Kiro 的路由。
- Go 路由程序目前更窄：Responses 到 Chat Completions（含流和用量），以及 Codex 与 Grok 的 Responses 互转。Messages 只对接 Messages 上游，Chat 只对接 Chat 上游。

两边都没有 Gemini `generateContent`。模型名或计价数据里出现 Gemini，也不等于有这条协议。RelayKit 多出来的主要是 Gemini，以及 Claude 与 Gemini 这条它自己标成不建议的路径。那不是当前路由缺的轮子。

可以借鉴的只有公开行为，并且必须用自己的代码重写：

- 协议对和转换质量（上面的矩阵），用来对照我们还缺哪几条，而不是照单全接。
- 库与宿主的分界：转换只管数据结构；监听、SSE 读写、鉴权、选路、计费留在宿主。这和 [Go 路由替换方案](adapter-sidecar.md) 里「Go 不做登录和路线选择」一致。
- 转换结果带实际步骤、质量等级和诊断；工具或字段对不上时显式报告，而不是悄悄丢。
- 每条流单独保持状态，结束时再补终止事件和用量。

不能做：

- `go get`、vendor，或把 `relaykit` 源码、测试、注释、golden JSON 复制或逐文件翻译进 `go/agenthub-adapterd`。
- 用它替换已经在跑的 Responses、Messages、Chat Completions 转换。
- 因为上游有 Gemini，就在产品里增加 Gemini 路由。产品支持范围仍以 [路由兼容性](../reference/route-compatibility.md) 为准。

## 许可

上游根许可证文件是 GNU AGPL-3.0 正文，文件本身没有附加条款。署名（“Frontend design and development by New API contributors.”）和「有界面的修改版须显示上游仓库链接」写在父仓库 README 的第 7 条附加条款里。RelayKit 的 `go.mod` 没有单独许可证，沿用这份 AGPL。本仓库的 `LICENSE`、`Cargo.toml`、`package.json` 都是 MIT。

`go get` 这个动作本身不是障碍。把该模块链进 `go/agenthub-adapterd` 再随桌面程序分发，组合结果就不能只按 MIT 交付：MIT 代码可以放进 AGPL 组合，但这不会把 RelayKit 或组合程序变成 MIT。只监听本机也不取消分发二进制时的源代码义务。AGPL 第 13 条针对的是修改版的远程网络用户，不能据此说本机调用自动触发它，也不能据此说本机程序就可以嵌进去。独立进程算不算另一部作品要看实际边界；把库链进现有路由程序不是许可隔离。本仓库没有批准 AGPL 组合，也没有批准另起一个 AGPL 进程。

读公开说明、按行为自己实现，不构成复制该库。

## 转换质量

同一张表同时覆盖请求、非流式响应和流。行是来源，列是目标。同协议格上游留空。

| 来源 → 目标 | OpenAI Chat | OpenAI Responses | Claude Messages | Gemini |
|---|---|---|---|---|
| OpenAI Chat | — | good | fair | fair |
| OpenAI Responses | good | — | fair | fair |
| Claude Messages | fair | fair | — | discouraged |
| Gemini | fair | fair | discouraged | — |

- `good`：结构接近，字段大体对应。
- `fair`：主要内容能转，协议特有限制以诊断形式返回。
- `discouraged`：Claude 与 Gemini 互相转换要先经过 OpenAI Chat，两步转换更容易丢字段。

结果会带上实际走过的步骤（`Steps`）和这次转换的质量（`Quality`）。若干其他路径也会经 OpenAI Chat 中转，例如 Gemini 到 Responses 的请求和响应。

## 包

| 包 | 作用 |
|---|---|
| `dto` | 四种协议的请求、响应、流和用量结构，以及与 new-api 共用的渠道、定价和用户设置结构 |
| `types` | 协议标识、错误、诊断和媒体来源 |
| `relayconvert` | 请求、响应和流的转换入口 |
| `relayconvert/convmeta` | 转换上下文和按次选项 |
| `relayconvert/reasoning` | 推理设置、模型名后缀和跨步思考状态 |
| `relayconvert/kitutil` | JSON 编解码和日志钩子 |
| `reasonmap` | Claude 与 OpenAI 结束原因对照 |

## 宿主必须自己做的事

- 解析上游 SSE，把事件变成对应 DTO；再把转换结果序列化回下游。
- 每条流单独持有一个 `ResponseStreamState`。流结束后必须调用 `FinalizeStreamResponse`，有的转换器到这一步才补终止事件和最终用量。
- 转换前把请求里的模型名改成上游真正要调用的模型。Claude 若请求和选项都没给出输出上限，转换失败。
- 默认 `ToolLossPolicy` 是 `allow`：转换照常完成，损失写进诊断。`safe` 在错误级诊断时拒绝请求；`strict` 有任何诊断就拒绝请求。响应和流在三种策略下都完成转换，只附带诊断。
- 函数定义、调用和结果四种协议都能转。Responses 的自定义工具和带命名空间的工具，转到 Chat、Claude 或 Gemini 时会变成普通函数，并用状态记住编码以便还原。无法表示的托管工具只产生诊断，不会静默变成等价能力。
- 推理额度、思考内容和模型名后缀（如 `-thinking`）要由调用方在转换前解析并写入上下文；关闭适配时，后缀留在模型名上，推理只来自请求字段。

## 来源

- 目录：<https://github.com/QuantumNous/new-api/tree/relaykit/v0.3.0/relaykit>
- 说明：<https://github.com/QuantumNous/new-api/blob/relaykit/v0.3.0/relaykit/README.md>
- 模块声明：<https://github.com/QuantumNous/new-api/blob/relaykit/v0.3.0/relaykit/go.mod>
- 标签 `relaykit/v0.3.0`（2026-10-09）：<https://github.com/QuantumNous/new-api/releases/tag/relaykit/v0.3.0>
