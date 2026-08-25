---
title: CLI 与配置参考
description: agenthub-cli 的当前命令树、全局参数、退出码和数据目录契约。
type: reference
audience: user-and-contributor
status: current
updated: 2026-08-25
---

# CLI 与配置参考

CLI 二进制名称为 `agenthub`，实现位于 `crates/agenthub-cli`，业务委托给 `agenthub-core`。开发时可以用 `cargo run -p agenthub-cli -- ...`。

## 全局参数

| 参数 | 说明 |
|---|---|
| `-h`, `--help` | 显示帮助 |
| `-V`, `--version` | 显示版本 |
| `--data-dir <path>` | 覆盖 AgentHub 数据目录 |
| `-a`, `--agent <id>` | 默认 Agent 过滤器 |
| `-o`, `--output <table\|json\|quiet>` | 输出格式，默认 `table` |
| `-y`, `--yes` | 跳过危险写操作确认 |
| `-v`, `--verbose` | 本次进程文件/控制台至少使用 debug |
| `-q`, `--quiet` | 等同 `--output quiet` |

当前 Agent id 由 `AgentId::ALL` 提供，通常包含 `claude`、`codex`、`kimi`、`grok`、`pi`、`workbuddy`、`cursor`、`dsh`。以 `agenthub agent list` 和 CLI help 为准。

## 命令树

| 命令 | 子命令 |
|---|---|
| `doctor` | runtimes、Agent、路径、数据库和 locks 的健康总览 |
| `env` | `list`、`install <runtime> [--channel]` |
| `agent` | `list`、`capabilities [--markdown]`、`install`、`upgrade`、`outdated`、`uninstall [--purge-config]` |
| `run` | 以 headless 模式运行 prompt；支持 `--agents`、`--all`、`--mode`、`--timeout`、`--cwd`、`--dry-run` |
| `provider` | `list`、`show`、`presets`、`import-live`、`switch`、`undo`、`test-latency` |
| `account` | `list`、`import`、`add-apikey`、`switch`、`delete`、`oauth-url`、`refresh`、`undo` |
| `skill` | `list`、`list-installed`、`sync`、`enable`、`disable`、`install`、`import-private`、`uninstall`、`update`、`project`、`market` |
| `usage` | `collect`、`stats`、`models`、`health` |
| `backup` | `list`、`create`、`restore`、`delete` |
| `config` | `path`、`get [key]`、`set <key> <value>` |

危险写操作默认需要确认。非 TTY 且未提供 `-y` 时不会写盘。

## 稳定退出码

| 码 | 含义 |
|---|---|
| `0` | 成功 |
| `1` | 运行期失败、IO、解析或 core 错误 |
| `2` | 用法错误、未知命令、非法参数或 Agent id |
| `3` | 业务拒绝或能力不支持，例如 Runtime 未就绪 |
| `4` | 需要确认但未提供 `-y`，或用户取消 |
| `5` | 部分成功；JSON 中包含失败项 |

`--output json` 时 stdout 是单一 JSON 值；错误写 stderr，敏感字段脱敏。

## 数据目录解析

优先级从高到低：

1. `--data-dir <path>`；
2. `AGENTHUB_HOME`（绝对路径或以 `~` 开头）；
3. `dirs::home_dir()/.agenthub`。

项目当前没有读取 `agenthub.toml` 的实现；不要把它当成有效配置入口，也不要把 API key、OAuth token、provider 列表写入未实现文件。

典型布局：

```text
{data_dir}/
├── agenthub.db
├── backups/
│   ├── db/
│   └── live/<agent>/<timestamp>/
├── exports/
├── logs/
└── cache/
```

SQLite 是 AgentHub 业务真源，live 文件由各 Agent adapter 管理。写 live 前应 backfill 当前状态并创建 backup；备份、日志和路径信息由 core service 统一处理。

## `config` 白名单

可写 key：`theme`、`language`、`log_level`、`log_retention_days`、`skill_market_source`、`close_to_tray`、`usage_collect_interval_min`。只读 key：`app_version`。

- `log_level`：`error|warn|info|debug|trace`，下次进程启动生效。
- `log_retention_days`：`1..=365`，默认 14，下次启动清理。
- `skill_market_source`：`auto|skills.sh|skillhub.cn`。
- `close_to_tray`：布尔值。
- `usage_collect_interval_min`：`0..=1440`；0 表示仅手动。
- `data_dir` 只能用 `--data-dir` 或 `AGENTHUB_HOME` 定位，不能用 `config set` 修改。

## 常见例子

```text
agenthub --output json doctor
agenthub --data-dir D:\agenthub-test config path
agenthub agent capabilities --markdown
agenthub account add-apikey --agent codex --label work --key -
agenthub provider switch --agent claude my-provider --yes
agenthub usage stats --days 30 --agent codex
```

命令的详细参数以 `agenthub <command> --help` 和 `crates/agenthub-cli/src/main.rs` 为准。

