# AgentHub 日志规范

> 正式契约：CLI / GUI 共用 `agenthub-core` 统一日志。  
> 实现真源：`crates/agenthub-core/src/logging/`、`utils/redact.rs`。  
> 配置契约交叉引用：[cli-and-config.md](cli-and-config.md) · [architecture.md](architecture.md)

---

## 1. 目标与原则

| # | 原则 |
|---|---|
| 1 | **双端一致**：CLI 与 GUI 只初始化一次 subscriber，业务埋点在 core |
| 2 | **默认可排障**：默认 `info` 足够定位关键失败；`-v` 拉高到 `debug` |
| 3 | **文件为主、控制台为辅**：持久化到 `{data_dir}/logs/`；stderr 按壳层策略 |
| 4 | **字段可检索**：稳定 `module` / `code` / `op`，便于对照错误码搜当日日志 |
| 5 | **禁止泄密**：出 core 前脱敏；日志消息走 `redact_text`，JSON DTO 走 `redact_json` |
| 6 | **可配置、可保留**：`log_level`、`log_retention_days` 落 L1 settings，改后下次启动生效 |

---

## 2. 架构

```text
CLI / GUI 薄壳
    │  init_for_app(data_dir?, shell, verbose, version)
    ▼
agenthub-core::logging
    ├── 解析 data_dir、读 settings（log_level / log_retention_days）
    ├── 按日文件 sink：tracing-appender RollingFileAppender
    ├── 可选 stderr sink（CLI 默认 warn；-v → debug；GUI 默认无控制台）
    ├── 启动时 purge 超期日志
    └── targets + log_* 辅助（error/info/warn/debug + 脱敏）
```

| 层 | 职责 |
|---|---|
| **core** | 唯一 subscriber 初始化、文件路径、保留策略、模块常量、埋点 helper |
| **文件** | `{data_dir}/logs/agenthub.YYYY-MM-DD`（daily rotation） |
| **stderr** | CLI：默认 `warn` 及以上；`-v` → `debug`。GUI：默认不挂控制台层 |
| **CLI/GUI** | 启动时调 `logging::init_for_app`；业务错误尽量走 `log_app_error` |

初始化幂等：同进程重复调用为 no-op。

---

## 3. 级别语义

| 级别 | 何时用 | 示例 |
|---|---|---|
| **error** | 操作失败、需用户或支持介入 | `provider.switch` 写 live 失败；`AppError` |
| **warn** | 可继续但异常/降级 | 配置未知键忽略、锁等待告警、部分 skip |
| **info** | 里程碑与审计 | 启动 logging initialized、settings 变更、切换成功 |
| **debug** | 路径、锁、操作步骤（`-v` 常用） | data_dir 覆盖、备份路径、关键 op 边界 |
| **trace** | 极细内部（默认不写；开发排查） | 循环/字段级细节 |

合法字符串：`error` \| `warn` \| `info` \| `debug` \| `trace`（大小写不敏感；`warning` 视为 `warn`）。

---

## 4. 日志文件

| 项 | 约定 |
|---|---|
| 目录 | `{data_dir}/logs/`（`config path` 可打印 `logs_dir`） |
| 命名 | `agenthub.YYYY-MM-DD`（`tracing-appender` daily，prefix=`agenthub`） |
| 兼容识别 | 保留清理也认 `agenthub.YYYY-MM-DD.log`、`agenthub-YYYY-MM-DD.log` |
| 轮转 | 按本地日历日 |
| 保留 | `log_retention_days`（默认 **14**，允许 **1..=365**）；启动时 purge 过期文件 |
| 编码 | 文本、无 ANSI；含 level、target、字段与消息 |

示例路径（Windows）：`%USERPROFILE%\.agenthub\logs\agenthub.2026-08-02`

---

## 5. 配置与优先级

### 5.1 键

| key | 类型 | 默认 | 说明 |
|---|---|---|---|
| `log_level` | 级别字符串 | `info` | 文件侧过滤级别 |
| `log_retention_days` | u32 | `14` | 日志保留天数；范围 1–365 |

存储：L1 SQLite `settings` 表。`config set` 后**下次进程启动**生效（当前进程已 init 的 subscriber 不热更新）。

### 5.2 解析优先级（高 → 低）

1. **`-v` / `--verbose`**：将生效级别至少抬到 `debug`；CLI 控制台同为 `debug`
2. **环境变量（可选）**：预留 / 与 subscriber 过滤器扩展兼容；未配置时跳过
3. **settings**（`log_level` / `log_retention_days`）
4. **内置默认**：`info` + 保留 14 天

CLI 无 `-v` 时：文件用 settings 级别；控制台默认只出 `warn+`，减少表格输出噪声。

可选 L0 `agenthub.toml` 的 `log_level` 为**契约预留 / 当前未实现**（全仓无读取代码）；L0 现行只有 `--data-dir` 与 `AGENTHUB_HOME`。**业务真源以 L1 settings 为准**（见配置文档 §7）。

---

## 6. 模块（`logging::targets`）

日志中通过 **target** 或字段 **`module=`** 标识来源。规范常量：

| 常量 | 字符串 | 用途 |
|---|---|---|
| `BOOT` | `core.boot` | 启动、logging 初始化 |
| `STORAGE` | `core.storage` | DB / 迁移 |
| `LOCK` | `core.lock` | per-agent 写锁 |
| `PROVIDER` | `core.provider` | 供应商池与切换 |
| `ACCOUNT` | `core.account` | 账号池与切换 |
| `BACKUP` | `core.backup` | 快照与恢复 |
| `INSTALL` | `core.install` | Agent/渠道安装、升级、卸载与 exec 诊断 |
| `DETECT` | `core.detect` | Agent/Runtime 探测（doctor、Agents 页、安装后 redetect） |
| `SKILL` | `core.skill` | 技能投影 |
| `CHAT` | `core.chat` | 会话 send/cancel；`agent_started`（命令脱敏）、`agent_process`(trace) |
| `PROJECT` | `core.project` | Agent project / session 扫描与删除 |
| `RUN` | `core.run` | headless multi-run；structured stream session open/close、`stream_step`(trace) |
| `CAPABILITY` | `core.capability` | 能力矩阵闸门（`require` blocked/partial；doctor 下发矩阵） |
| `SETTINGS` | `core.settings` | 应用设置；亦覆盖 ConfigurationService apply/materialize |
| `USAGE` | `core.usage` | 会话用量采集与成本重算 |
| `OAUTH` | `core.oauth` | OAuth PKCE 启动 / 回调 / 浏览器 |
| `CLI` | `cli` | CLI 壳 |
| `GUI` | `gui` | GUI 壳 |

检索示例：在当日日志中搜 `module=core.provider` 或 target `core.provider`。

---

## 7. 字段约定

| 字段 | 含义 |
|---|---|
| `module` | 逻辑模块 id（见上表）；`log_app_error` 等 helper 写入字段而非仅依赖 target |
| `code` | 稳定错误码，与 `AppError::code()` 对齐（如 `env.not_ready`、`io`、`invalid_arg`） |
| `op` | 操作名短标识（如 `switch`、`open`、`set`、`start`） |
| `agent` | 相关 Agent id（可选） |
| `elapsed_ms` | 耗时毫秒（可选，性能/锁等待） |
| `msg` | 人类可读消息（实现上多为 format 正文；写入前 `redact_text`） |

错误推荐形态：

```text
ERROR ... module=core.provider code=io op=switch agent=codex <redacted message>
```

Helper（core）：

- `log_app_error(module, op, &err)`
- `log_app_error_agent(module, op, agent, &err)`
- `log_info` / `log_warn` / `log_debug`

---

## 8. 脱敏

| API | 用途 |
|---|---|
| `redact_json` | 序列化给 CLI/GUI 的 JSON；按密钥 key 名递归替换为 `***`；opaque TOML `content` 整段掩码 |
| `redact_text` | 日志、安装输出、错误串：Bearer、`api_key=` / `token:` 赋值、常见前缀 `sk-` / `xai-` / `ghp_` 等 |

**禁止写入日志（及普通用户输出）的内容：**

- 完整 API Key、OAuth access/refresh token、session token、client_secret、password、private_key
- 未脱敏的 provider/account credentials JSON
- 用户主密码类材料（当前产品范围外也不应引入明文落盘日志）

原则：宁可过度掩码，不可漏打密钥。DTO 出 core 前集中脱敏；埋点消息统一 `redact_text`。

---

## 9. 排查流程

1. 复现失败，记下 CLI/GUI 展示的 **`code`**（json 错误的 `code` 字段，或文案中的错误码）
2. `agenthub config path` 确认 `data_dir` / `logs_dir`
3. 打开**当日**文件：`{logs_dir}/agenthub.YYYY-MM-DD`
4. 按 `code=` 或 `module=core.<域>` / `op=` 过滤
5. 信息不足时：  
   `agenthub config set log_level debug` → **重启** → 或本次加 `-v` 再跑
6. 对照操作顺序：backfill → backup → 写 live → 锁；锁/IO 类优先看 `core.lock` / `core.backup` / 对应业务 module
7. 仍不足：保留该日日志 + `doctor` 输出（脱敏后）再升级支持

---

## 10. 设置与 CLI

```text
agenthub config get
agenthub config get log_level
agenthub config get log_retention_days
agenthub config set log_level debug
agenthub config set log_retention_days 30
agenthub config path          # 含 logs_dir
agenthub -v doctor            # 本次进程 debug（文件 + stderr）
```

| 命令 | 行为 |
|---|---|
| `config get [key]` | 白名单内可读；无 key 列出含 `log_level`、`log_retention_days` |
| GUI 设置 → 数据 | 日志级别 / 保留天数 / 日志目录「打开」；级别下次启动生效 |
| `config set <key> <value>` | 校验级别/天数范围后写入 settings |
| `-v` | 本次抬高到至少 debug，不修改 settings |

白名单真源见 [cli-and-config.md §7.3](cli-and-config.md)；日志细节以本文为准。

---

## 11. 实现状态

| 项 | 状态 |
|---|---|
| core 统一 init（文件 + 可选 stderr） | **P0 已落地** |
| 按日文件 + `log_retention_days` 启动清理 | **已落地** |
| settings / `config get|set`：`log_level`、`log_retention_days` | **已落地**（下次启动生效） |
| `targets` + `log_app_error*` / `log_*` | **已落地** |
| core 单测：parse/purge/load_log_prefs/settings 校验 | 测试仍内嵌于 `logging/mod.rs`（历史遗留，与 [testing.md](testing.md) 分文件约定不一致） |
| GUI settings commands + Settings 数据页日志项 | **已落地** |
| 关键路径埋点（boot、settings、部分 service/CLI/GUI） | **P0 已落地** |
| detect 消防日志（`core.detect`：PATH / well-known / version probe / PS 双版本 notes） | **已落地** |
| install 失败附加 diag 行（redetect / PATH / 解释器） | **已落地** |
| 全 service 穷尽埋点、elapsed_ms 全面化、GUI 日志页 | 后续按需 |
| 环境变量覆盖级别 | 可选扩展 |

---

## 12. 修订记录

| 版本 | 日期 | 说明 |
|---|---|---|
| v1.0 | 2026-08-02 | 初版：架构、级别、文件、配置优先级、模块/字段、脱敏、排查与 CLI |
