---
title: Claude Chat B3 探测结论
type: status
status: current
owner: maintainers
audience: chat implementers
updated: 2026-09-06
---

# Claude Chat B3 探测结论

承接 [统一体验方案](../proposals/chat-unified-experience.md)、[S0](chat-codex-s0.md) 与 [B2](chat-codex-b2.md)。本记录只回答：**当前 Claude 是否能以不大的改动复用 `ChatRuntime`（与 Codex app-server 同级的会话控制）**。结论是否定的；本批 **未** 把 Claude 空会话接到 runtime，也 **未** 伪造确认/补充按钮。

## 证据（仓库现状）

| 路径 | 事实 |
| --- | --- |
| `crates/agenthub-core/src/services/chat_runtime/mod.rs` | `ChatRuntime` 明确为 **Codex app-server** 运行时；传输与 `model/list`、`turn/*`、approval 均绑定 Codex JSON-RPC |
| `crates/agenthub-core/src/services/chat_runtime/store.rs` `enable_if_new` | 非 Codex 直接 `Unsupported("持续聊天目前只支持 Codex")`；有历史消息的会话保持 legacy |
| `crates/agenthub-core/src/adapters/claude.rs` `build_run_spec` | Claude Chat 路径是 **`claude -p` + `--output-format stream-json`**（可选 `--resume`）；危险模式用 `--dangerously-skip-permissions`，**不是**交互式批准通道 |
| 同文件 `capability(SessionResume)` | 标明「Chat 后续轮次走 print+resume」 |
| `crates/agenthub-core/src/adapters/session_resume.rs` | Claude/Codex 均支持 print-resume；**不等于** app-server 式 reply/steer/cancel |
| `docs/status/chat-codex-s0.md` T03 | Claude Agent SDK 仅为候选；第三方 claude.ai 登录/额度需官方批准；**未做** SDK 登录或双向交互实验 |
| 方案文 | 「无真实确认通道不能展示可点击的假确认」；Claude 需「经验证的交互接口」 |

## 阻塞项

1. **没有** 与 Codex `turn/start`、`turn/steer`、`turn/interrupt`、command/file approval、`respond*` 对等的、已在本仓库验证过的 Claude 控制协议。
2. 现有 Claude 适配器是一次性 **print/exec + stream-json 解析**；stdin 管道模型不能承载 B1 那种串行确认队列。
3. Agent SDK 路线仍卡在 **登录/计费边界未实验**；不能默认把现有订阅登录接到 SDK。
4. 在协议未清时把空 Claude 会话标成 `ChatRuntime`，只会得到无确认/无 steer 的假 runtime，违反方案红线。

## 本批决定

- **不实现** Claude → `ChatRuntime` 接线。
- **不增加** 假的允许/拒绝或“已补充”UI。
- Claude 继续走现有 legacy `chat_send` / print+resume，直到另开批次完成：可选接入方式（SDK vs CLI）、真实确认/停止实验、再定最小 driver。

## 若后续要做（非本 PR）

1. 先固定一种获许可的登录方式并做真实双向实验（确认、停止、恢复）。
2. 实验通过后再增加 Claude driver；空新会话进 runtime，**legacy 历史保持 legacy**（与 Codex B1 相同规则）。
3. UI 能力矩阵按 `supported / unsupported / unknown` 逐项开放，禁止从 Codex 成功类推。
