# Kimi Membership → Codex `local_bridge` 发布前实机 Dogfood

> 关联：[adapter-design.md](adapter-design.md) Phase 1 / §11.4。  
> 自动验收（bridge / restore / 退出协调器）已在工作区通过；**本清单只覆盖必须用桌面应用 + 真实连接完成的项**。  
> **禁止**把密钥、Authorization、prompt、工具参数或响应正文写入本文件或任何报告。只记 `profile_id`、端口、错误码、耗时、是否完成。

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
3. 在 Adapter / Connections 核对写回的端口和 Codex `base_url`。

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
