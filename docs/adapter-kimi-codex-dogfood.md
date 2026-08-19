# Kimi Membership → Codex `local_bridge` 发布前实机 Dogfood

> 关联：[adapter-design.md](adapter-design.md) Phase 1 / §11.4、[product-decisions.md](product-decisions.md)。  
> 自动验收（bridge / restore / 退出协调器）已在工作区通过；**本清单只覆盖必须用桌面应用 + 真实连接完成的项**。  
> 创建/应用的日常入口可以是 Dashboard 卡片「连接/切换」或 Connections「接到…」（ConnectFlow）；本机路由页（`/routes`，侧栏 Routes）只用于 ③ 的桥控件。  
> 入口：Dashboard「连接/切换」与 Connections「接到…」（真登录常驻；不可行目标在对话框置灰 + 原因）。生成投影不进登录列表。见 [connection-binding-model.md](connection-binding-model.md)。  
> 本清单同时覆盖 ①（Kimi→Claude、Anthropic Key→Pi）和 ③（Kimi→Codex）。同一把 Kimi Key 对不同目标走不同路，不是「双协议 = 万能」。  
> **禁止**把密钥、Authorization、prompt、工具参数或响应正文写入本文件或任何报告。只记 `profile_id`、端口、错误码、耗时、是否完成。

## Hub ConnectFlow 真机验收（最短）

mock 下 apply 正向链路不可达，必须 `pnpm tauri:dev` + 真实凭据。下列勾选框保持未勾。

入口：Dashboard「连接/切换」与 Connections「接到…」（每份真登录常驻该按钮）。

反例：生成投影不出现在登录列表，故无「接到…」。OAuth / 未识别 / 无边等真登录仍显示「接到…」，对话框内不可行目标置灰 + 原因原文。

### 1. Kimi 会员 Provider → Claude（① API 直连）

步骤：

1. 从 Dashboard Claude 卡片「连接/切换」，或从 Connections 的 Kimi 会员行「接到…」进入。
2. 选该 Kimi 会员为来源、Claude 为目标，预览后 apply。
3. 核对 Claude 当前连接按 ① 直连生效（只记 profile / provider id 后缀）。

预期：直连可 apply；Claude 卡片显示直连 / 改配置，**不**显示桥；该 Kimi 行「正用于」含 Claude。

- [ ]

### 2. Kimi 会员 Provider → Codex（③ 本机路由；Chat Completions ≠ Responses）

步骤：

1. 从 Dashboard Codex 卡片「连接/切换」，或从 Connections 的 Kimi 会员行「接到…」进入。
2. 选该 Kimi 会员为来源、Codex 为目标，预览后 apply（本地桥）。
3. 核对桥已创建/可启动，Codex 当前连接指向生成 Provider（只记 `profile_id`、端口）。

预期：③ 本机路由可 apply；Codex 卡片显示桥状态；该 Kimi 行「正用于」含 Codex。

- [ ]

### 3. Claude Anthropic Provider → Pi（① API 直连 / 写槽；不是订阅 OAuth）

步骤：

1. 从 Dashboard Pi 卡片「连接/切换」，或从 Connections 的 Claude Anthropic 行「接到…」进入。
2. 选该 Anthropic Provider 为来源、Pi 为目标，预览后 apply。
3. 核对 Pi 当前连接已同步（只记 profile / provider id 后缀）。

预期：① 写槽可 apply；Pi 卡片显示直连 / 改配置，**不**显示桥；该 Anthropic 行「正用于」含 Pi。

- [ ]

## 自动覆盖对照（半 e2e ≠ 本清单放行）

下列自动化已覆盖**可机器化子集**；勾选本 dogfood 清单时，自动绿只能作为前置，不能代替真机项。

| Dogfood 项 | 自动化覆盖 | 测试入口（过滤） | 仍须真机 |
|---|---|---|---|
| 1. 密钥轮转 | 部分：上游 secret 变更 → listener 替换 + local bearer 不变；restore material 读新 key | `cargo test -p agenthub-gui ensure_listener_replaces_upstream_auth_while_keeping_local_bearer`；`cargo test -p agenthub-core restore_uses_a_rotated_source_key_without_changing_the_local_bearer` | 真 Kimi 上游是否吃新 key；Codex 长连接行为 |
| 2. 端口占用重绑 | 部分：preferred 被占 → rebind；再 realign profile/provider 端口一致；失败回滚 | `ensure_listener_rebinds_when_preferred_port_is_busy`；`busy_preferred_port_rebind_then_realign_updates_projection`；`realign_restored_bridge_port_*` | 真 Codex 读到新 `base_url` |
| 3. 长时间 SSE | 部分：mock 上游分片 / 空闲超时（core bridge） | `cargo test -p agenthub-core -- bridge::` | 真模型数分钟流 |
| 4. 文本+工具闭环 | 部分：协议 fixtures（Responses↔Chat） | `cargo test -p agenthub-core -- bridge::protocol` | 真 Codex 工具执行 |
| 5. 上游失败与取消 | 部分：401 health 拒绝、脱敏 Debug、degraded 状态 | `bound_health_rejects_upstream_auth_before_a_provider_switch`；bridge host health/status 测 | 真 401/429/5xx 正文不泄漏；客户端取消 |
| 6. 托盘退出 drain | 部分：ExitCoordinator / stop 幂等 | `exit_coordinator`；`stop_is_idempotent_*` | 真托盘三选一 UI |
| 7. auto_start 恢复 | 部分：restore 过滤、retryable 标记、失败回滚、端口 realign | `restore_filter_*`；`retryable_restore_*`；`realign_restored_bridge_port_*` | 冷启动完整 GUI + 真端口竞争 |

本地快捷（与 PR CI 互补）：

```bash
cargo test -p agenthub-core --locked bridge
cargo test -p agenthub-gui --locked adapter_bridge_controller
```

## 环境（填写，勿贴密钥）

| 项 | 记录 |
|---|---|
| 日期 / 操作者 | |
| 构建 | 本地 `dev` 提交：`f323e90` 或更新 |
| OS | |
| AgentHub 数据目录 | `%USERPROFILE%/.agenthub`（或 `AGENTHUB_HOME`） |
| Kimi 来源 | Connections 中 `kimi-code-membership` Provider，只记 id 后缀 |
| Codex 是否已安装 | |
| 桥 `profile_id` | |
| 首选端口 / 实际端口 | |
| `auto_start` | 默认 false；第 7 项再打开 |

## 记录规则

每项只写：

- 环境（构建、是否有活跃桥）
- 步骤（做了什么）
- 结果（通过 / 失败 / 跳过）
- 日志证据：`profile_id` + `request_id` + `code` + `elapsed_ms` + `op`
- 问题（无则写「无」）

不要复制日志里的 URL query、token、消息正文。

## 1. 密钥轮转

步骤：

1. 用当前 Kimi membership 创建并启动 Codex 本地桥。记下 `profile_id`、端口。
2. 在 Connections 轮转该 Kimi Provider 的 API Key（不要把新旧 key 写入记录）。
3. 对已运行的桥再发一次最小 Codex 请求，或停止后启动。
4. 确认 listener 因 spec 漂移被替换；**local bearer 不变**（只记「未变 / 已变」）。

预期：上游改用新 key；loopback token 不变；DTO / 日志无 bearer。

结果：

- [ ] 通过
- 证据：
- 问题：

## 2. 端口冲突与重绑定

步骤：

1. 用 `TcpListener` 或其他进程占住桥的 preferred 端口。
2. 启动或重启该桥（或重启 AgentHub 且 `auto_start=true`）。
3. 在本机路由页（`/routes`）、Connections 或 Dashboard 徽标均可核对写回的端口和 Codex `base_url`（不要只看本机路由页）。

预期：绑到新端口；profile / generated provider 对齐新端口；不留下指向已停 listener 的旧端口。

结果：

- [ ] 通过
- 旧端口 / 新端口：
- 证据：
- 问题：

## 3. 长时间 SSE

步骤：

1. 经 Codex 走该桥发一次持续数分钟的流式请求（不要记录 prompt / 正文）。
2. 观察是否完成、是否空闲超时、日志是否只有 `request_id` / `elapsed_ms` / `code`。

预期：正常结束或给出通用错误；无 payload 泄漏。

结果：

- [ ] 通过
- 时长：
- 证据：
- 问题：

## 4. 文本及工具调用闭环

步骤：

1. 一次短文本流。
2. 一次工具调用闭环（模型调工具 → 回填 → 终答）。只记是否完成，不记参数。

预期：Responses 事件完整；工具 `call_id` / name 可在诊断里看到结构字段，看不到参数值。

结果：

- [ ] 文本通过
- [ ] 工具闭环通过
- 证据：
- 问题：

## 5. 上游失败和中途取消

步骤：

1. 临时弄坏上游（错误 key 或断开），确认通用错误、无凭据泄漏。
2. 从 Codex 客户端取消进行中的流。
3. 可选：触发 429 / 5xx，确认本地映射为通用错误且 `upstreamStatus=degraded`。

预期：401 / 取消 / 429 / 5xx 都不回传上游正文；listener 在仍运行时可标 `degraded`。

结果：

- [ ] 上游失败通过
- [ ] 客户端取消通过
- 错误码：
- 证据：
- 问题：

## 6. 托盘退出 drain

步骤：

1. 保持至少 1 个桥在跑。
2. 托盘退出：确认「隐藏到托盘 / 停止桥接并退出 / 取消」。
3. 选「隐藏到托盘」后，桥端口仍可请求。
4. 再选「停止桥接并退出」，确认端口释放、`active_bridge_count` 写入日志。

预期：有桥时必须确认；退出走 `ExitCoordinator` drain；0 桥日志不能代替本项。

结果：

- [ ] 三选一出现
- [ ] 隐藏到托盘后端点仍可用
- [ ] 停止并退出后端口释放
- 证据（可引用 `op=exit active_bridge_count=Some(N)`）：
- 问题：

## 7. 自动恢复和恢复失败回滚

步骤：

1. 打开该桥 `auto_start`，退出后再启动 AgentHub。
2. 确认只恢复 `Active + auto_start + local_bridge`。
3. 若 preferred 端口被占：恢复后写回新端口；失败则旧 `local_port` / provider `base_url` 仍指向旧端口，且新 listener 被停掉。

预期：恢复失败标 retryable，不污染其它 profile；`NeedsAttention` 不会被成功 restore 清掉。

结果：

- [ ] 自动拉起通过
- [ ] 重绑或失败回滚通过
- 证据：
- 问题：

## 发布门槛

七项全部勾选且无未关闭的密钥泄漏问题后，才算 Phase 1 实机 dogfood 完成。  
工作区自动测试通过**不能**单独作为放行依据。
