# AgentHub CLI 命令规范与配置契约 v1.4

> **现行状态（2026-08-19）**：sidecar 未迁；官方船经 `release` 三文件 bump。Chat 没有模型选择器；Claude/Codex Chat 后续轮次可走 print+resume；Projects 列表不直接调用 `--resume`。MCP 只读 inventory（注入未做）。
> 对应《项目方案》GUI + CLI 双端与《架构拆分》`agenthub-cli` / 数据目录。  
> 本文是**可验收契约**：实现 CLI 与配置读写时以本文为准；与 GUI 冲突时以 **core service 行为一致** 为最高原则。  
> 状态（2026-08-15，以代码为准）：CLI 已覆盖 doctor（含 ⑤ Locks）/ run / env / agent（含 `capabilities`、`outdated`）/ provider（含 `undo`、`test-latency`）/ account（含 `oauth-url`、`refresh`、`delete`、`undo`）/ skill 全树 / usage / backup（含 `delete`）/ config（白名单 + 只读 `app_version`）。GUI 已接线 doctor、安装、Provider、Account、OAuth PKCE（Claude/Codex/Grok）、Skill、Usage、Backup、Chat、Projects、Settings。Chat 无模型选择器；Projects 不接 `--resume`；MCP 只读 inventory（注入未做）；全站 i18n 未做。Provider/Account **测速与切换撤销** CLI/GUI 均已接线。**备份导出**仍未实现。凭据落盘加密为当前范围外。跨 Agent 复用三路见 [product-decisions.md](product-decisions.md)；本文「代理模式」≠ ③ 本机路由。  
> **2026-08-16 文档回写**（仍为 v1.4）：对齐 `DoctorReport`、`--days`、`add-apikey [--label]`、L0 仅 `--data-dir` / `AGENTHUB_HOME`；删除 `--show-secrets` 二期主密码表述。
> v1.1：`doctor` 含 runtimes；新增 `env` 资源；`agent install` 两阶段与 `--install-deps`。  
> v1.2：平台环境差异——`doctor`/`env list` 仅返回宿主相关 Runtime（macOS 不含 PowerShell）；`env install` 默认 channel 与 `agent install|upgrade` native 底层命令按 Windows/macOS/Linux 分流。

系列文档：[项目方案](agenthub-plan.md) · [架构拆分](architecture.md) · [UI 设计](ui-design.md) · [日志规范](logging.md)

---

## 1. 总原则

| # | 原则 |
|---|---|
| 1 | CLI 与 GUI **只做薄壳**，业务一律进 `agenthub-core` 的 service |
| 2 | 与 GUI **共用** `~/.agenthub/`（或 `AGENTHUB_HOME`）与 **per-agent 写锁** |
| 3 | 命令采用 **资源型子命令**（`agenthub <resource> <verb>`），禁止再发明平行的扁平全局动词树 |
| 4 | 危险写操作：默认交互确认；非 TTY 或未传 `-y` 时拒绝执行（退出码见 §5） |
| 5 | 脚本优先：稳定 **退出码**、可选 **`--json`**、敏感字段默认脱敏 |
| 6 | OAuth 浏览器流以 GUI 为主；CLI 侧重 list/switch/import/apikey 与自动化 |

---

## 2. 全局参数与运行环境

### 2.1 全局 flags（所有子命令可用）

| Flag | 说明 |
|---|---|
| `-h` / `--help` | 帮助 |
| `-V` / `--version` | 版本（与 core/appVersion 对齐） |
| `--data-dir <path>` | 覆盖数据目录（优先级见 §7.1） |
| `-a` / `--agent <id>` | 默认 agent：`claude` \| `codex` \| `kimi` \| `grok` \| `pi` \| `workbuddy` \| `cursor` \| `dsh`（真源 `AgentId::ALL`） |
| `-o` / `--output <fmt>` | `table`（默认，TTY）\| `json` \| `quiet`（仅退出码/最少输出） |
| `-y` / `--yes` | 跳过交互确认（脚本/CI） |
| `-v` / `--verbose` | 已接入 core **tracing**：本次进程级别至少 `debug`（文件 + stderr），用于路径/操作等诊断；详见 [logging.md](logging.md) |
| `-q` / `--quiet` | 等同 `-o quiet` |

### 2.2 Agent id 规范

- 一律小写：`claude` / `codex` / `kimi` / `grok` / `pi` / `workbuddy` / `cursor` / `dsh`（真源 `AgentId::ALL`；`dsh` 约束见 [deepseek-harness-integration.md](deepseek-harness-integration.md)）
- 非法 id → 退出码 `2`，stderr 提示合法值（`AgentId::expected_list()`）

### 2.3 输出

- **table**：`comfy-table`，适合人类；列名稳定（改名视为破坏性变更）
- **json**：stdout 单一 JSON 值（对象或数组）；错误时 stdout 可空，stderr 为 JSON 或纯文本错误（见 §5.2）
- **凭据**：任何输出默认脱敏（`sk-••••3f2a`）；**不提供** `--show-secrets`。主密码 / 落盘加密为项目范围外。

### 2.4 二进制与安装名

- crate：`agenthub-cli`
- 用户命令名：`agenthub`（安装/打包时映射）
- Windows：同名 `agenthub.exe`，子进程启动第三方安装脚本时 `CREATE_NO_WINDOW` 由 core 处理

---

## 3. 命令树（Freeze）

```text
agenthub
├── doctor                          # 排障总览（含 runtimes）
├── run        <prompt>             # P0.5：多 Agent 并行/串行 headless 执行
│              [--agents a,b] [--all] [--mode parallel|sequential]
│              [--timeout secs] [--cwd path] [--dry-run] [--allow-dangerous]
├── env
│   ├── list                        # 共享运行时检测
│   └── install    <runtime> [--channel <name>]   # P2：引导装 Node 等
├── agent
│   ├── list
│   ├── capabilities [--markdown]   # 静态能力矩阵
│   ├── install    <agent> [--channel <name>] [--install-deps]
│   ├── upgrade    <agent>
│   ├── outdated   [agent] [--force]        # npm dist-tag 探测
│   └── uninstall  <agent> [--purge-config]
├── provider
│   ├── list       [--agent <id>]
│   ├── show       <name> [--agent <id>]
│   ├── switch     <name> --agent <id>
│   ├── import-live [--agent <id>] [--name <name>]
│   ├── presets    [--agent <id>]          # 列出内置预设 id
│   ├── undo       --agent <id>            # 撤销最近一次 switch
│   └── test-latency <name> --agent <id>   # Base URL RTT（毫秒）
├── account
│   ├── list       [--agent <id>]
│   ├── switch     <id-or-label> --agent <id>
│   ├── import     [--agent <id>] [--name <name>]
│   ├── add-apikey --agent <id> [--label <s>] --key <s>   # key 也可 stdin
│   ├── delete     <id-or-label> --agent <id>
│   ├── oauth-url                       # 打印 PKCE authorize URL（不完成流程）
│   ├── refresh    <id-or-label> --agent <id>
│   └── undo       --agent <id>            # 撤销最近一次 switch
├── skill
│   ├── list            [--agent <id>]
│   ├── list-installed              # 全技能根（共享 + Agent 私有）
│   ├── sync            [--agent <id>] [--all] [--force]   # copy 投影
│   ├── enable          <skill> --agent <id> [--force]
│   ├── disable         <skill> --agent <id>
│   ├── install         <path|zip|git> [--overwrite]
│   ├── import-private  <skill> [--overwrite]
│   ├── uninstall       <skill> [--private --agent <id>]
│   ├── update          <skill>
│   ├── project         <skill> --agent <id> [--mode link|copy]
│   └── market          [--query <q>]
├── usage
│   ├── collect
│   ├── stats      [--days <n>] [--agent <id>] [--model <name>]   # n >= 1，默认 7
│   ├── models     [--agent <id>]
│   └── health
├── backup
│   ├── list       [--agent <id>]
│   ├── create     --agent <id> [--note <s>]
│   ├── restore    <backup-id>
│   └── delete     <backup-id>
└── config                           # AgentHub 自身设置（非各 agent live）
    ├── path                         # 打印 data dir / db 路径
    ├── get        [key]
    └── set        <key> <value>
```

**明确不做（CLI v1）：**

- `agenthub oauth ...` 完整浏览器 PKCE 主路径归 GUI；CLI 提供 `account oauth-url` / `account refresh`
- 官方「模型商店」浏览/切换
- 代理模式（热切换不改 live 文件，P4）、WebDAV。这不是三路里的 ③ 本机路由；③ 走现有 `local_bridge`，不在本条「明确不做」里

---

## 4. 子命令契约

下列「core」列表示应调用的 service；实现时禁止在 CLI 内直接改 live 文件。

### 4.1 `doctor`

| 项 | 约定 |
|---|---|
| 作用 | 一页看健康：**宿主相关共享 runtimes**、数据目录、db 可写、**全部已注册 Agent** detect、当前 provider/账号摘要（脱敏）、parser health、锁是否异常 |
| core | `env_service.detect_all`（= `runtime::host_runtimes`）+ `agent_service.detect_all` + `usage_service.parser_health` + settings/paths |
| 输出 | table 分区或 json（`DoctorReport`，camelCase）：`{ dataDir, runtimes[], agents[], capabilities, usageHealth[], paths, dbOk, ok, warnings[], version, locks[] }`。`paths` 为 `PathInfo`（`dataDir` / `dbPath` / `backupsDir` / `logsDir`）。`locks` 为 `{data_dir}/locks` 下 live-write 锁检查（`held` / `stale` / `malformed`） |
| 分区顺序 | ① Runtimes ② Agents ③ Paths/DB ④ Usage parsers ⑤ Locks |
| 退出码 | 全部关键检查通过 `0`；runtime 缺失/过旧视为 **警告**（`0` + warnings，不挡 doctor）；db 不可用等硬错误 `1` |

`runtimes[]` 元素建议字段：`id`（`nodejs`/`npm`/`git`，Windows 另可含 `powershell`）、`status`（`ok`/`missing`/`outdated`/`broken_path`）、`version?`、`path?`、`minRequired?`、`remediation?`、`notes?`。

**平台约束**：macOS/Linux 的 `runtimes[]` **不得**包含 `powershell`；不得把「未安装 pwsh」计为环境故障。完整约定见 [agenthub-plan.md §5.7.5](agenthub-plan.md)。

### 4.1b `run`（P0.5 multi-agent execution）

| 项 | 约定 |
|---|---|
| 作用 | 将同一 prompt **并行或串行**投递给一个或多个已安装 Agent CLI（headless），汇总 stdout/stderr/退出码 |
| core | `run_service.run` → 各 `AgentAdapter::build_run_spec` + 带超时的 `ProcessRunner` |
| 默认 agents | 未传 `--agents`/`--all`/`-a` 时：当前 **已安装** 的全部 agent |
| 未安装 | 默认 **skip**（`status=skipped`），不失败；若全部 skipped → 退出码 `3` |
| `--dry-run` | 只打印将执行的命令，不 spawn |
| `--allow-dangerous` | 显式注入各家危险 auto-approve flag；stderr 警告；**默认关闭** |
| 超时 | `--timeout` 秒（默认 300）；超时 kill 子进程，`status=timeout` |
| 输出 | table 摘要 或 json 完整 `MultiRunReport` |
| 退出码 | 全成功（含 dry_run/skipped 混合但至少有一个 ok/dry_run）`0`；参数错 `2`；有 failed/timeout 或全 skipped `3` |
| 边界 | CLI 侧仍是一次性 headless 投递（非多轮对话）。**GUI Chat 页**支持多 Agent 并排多轮对话：上下文由 AgentHub 侧拼接且各 Agent 隔离；共享 cwd 并行可能有副作用（后续 worktree） |

各家 headless 入口由 `AgentAdapter::build_run_spec` 生成（随上游 CLI 版本变化，**不在本文抄写完整 argv**）。CLI `run` 默认 text 模式；GUI Chat 对支持结构化流的 Agent 使用 `ProcessMode::Auto`。

> Chat 过程流式与 `ProcessMode` 见 [chat-process-streaming.md](chat-process-streaming.md)。StructuredStream 能力以矩阵与 parser 源码为准。

### 4.2 `env`

| 命令 | 参数 | core | 危险 | 说明 |
|---|---|---|---|---|
| `list` | | `env_service.detect_all` | 否 | 与 doctor 的 runtimes 段同源；**仅宿主相关 Runtime**（macOS/Linux 不含 `powershell`） |
| `install` | `<runtime> [--channel]` | `env_service.install_runtime` | 中 | P2；`runtime`：`nodejs`/`git` 等；channel 默认 **Windows=`winget`、macOS=`brew`、Linux=`manual`**；`powershell` **永不**一键安装；Linux 的 `manual`/`apt`/`dnf`/`pacman`/`zypper`/`apk` 只打印 remediations，不自动 sudo；未知发行版不猜测 apt-get；流式日志 stderr；成功后 invalidate 缓存 |

- `install` 在无自动渠道时打印 remediations（命令 + URL）并以退出码 `3`（业务失败）结束，**不**假装成功。无包管理器（brew/winget 未找到）或不支持的安装渠道 → 退出码 `3`（`env.not_ready` / `unsupported`），`--output json` 的 `details` 含 `remediations`（已按宿主平台过滤）。命令已执行但重新检测未就绪仍为 `install.failed`（退出码 `1`）。
- **不提供** `env uninstall`（避免误伤系统 Node）。
- 平台环境差异硬约束见 [agenthub-plan.md §5.7.5](agenthub-plan.md)。

### 4.3 `agent`

| 命令 | 参数 | core | 危险 | 说明 |
|---|---|---|---|---|
| `list` | | `detect_all` | 否 | 安装态、版本、渠道、bin、认证摘要；建议附带所选默认渠道的 `envReady` |
| `install` | `<agent> [--channel] [--install-deps]` | `install`（两阶段） | 中 | **先 ensure_env**；缺环境且无 `--install-deps` → 退出码 `3`，json/文本含 `EnvNotReady`；有 `--install-deps` 则先引导装 Runtime 再装 Agent；channel 默认取元数据第一个；**底层 native：Windows ps1 / Unix sh**；流式日志 stderr |
| `upgrade` | `<agent>` | `upgrade` | 中 | 复用安装渠道：npm → 重装 latest；native → 平台对应官方脚本；升级前同样校验渠道 Runtime |
| `uninstall` | `<agent> [--purge-config]` | `uninstall` | **高** | `--purge-config` 必确认或 `-y`；先 `pre-uninstall` 备份；**不卸载**共享 Runtime |

`EnvNotReady`（`--output json` 时 `details`）建议：

```json
{
  "error": "environment not ready",
  "code": "env.not_ready",
  "details": {
    "agent": "codex",
    "channel": "npm",
    "missing": ["nodejs"],
    "remediations": [
      { "kind": "winget", "command": "winget install OpenJS.NodeJS.LTS" },
      { "kind": "brew", "command": "brew install node" },
      { "kind": "url", "url": "https://nodejs.org/" },
      { "kind": "hint", "text": "Install Node, restart shell/AgentHub, then re-run" }
    ],
    "hint": "re-run with --install-deps to bootstrap supported runtimes"
  }
}
```

GUI/CLI 展示 remediations 时必须按宿主平台过滤（Windows 不展示 `brew`，macOS 不展示 `winget`，Linux 不展示 `winget`/`brew`）。

### 4.4 `provider`

| 命令 | 参数 | core | 危险 | 说明 |
|---|---|---|---|---|
| `list` | `[--agent]` | `list` | 否 | 标记 `is_current` |
| `show` | `<name> [--agent]` | `get` | 否 | 配置正文脱敏 |
| `switch` | `<name> --agent`（agent 可全局 `-a`） | `switch` | **高** | 流程：校验→backfill→backup→原子写→锁；与 GUI 完全一致 |
| `import-live` | `[--agent] [--name]` | `import from live` | 中 | 把当前 live 收成一条 provider |
| `presets` | `[--agent]` | presets 注册表 | 否 | P1 起 presets 应在 **core**，CLI/GUI 共用 |
| `undo` | `--agent` | `undo_switch` | **高** | 一发撤销最近一次 switch；无槽位时不报错 |
| `test-latency` | `<name> --agent` | `test_latency` | 否 | 探测已存 provider 的 Base URL RTT（毫秒） |

`switch` 在 TTY 且无 `-y` 时展示三要素摘要（backfill / backup 路径 / 进程警告）。GUI 侧已无 `SwitchConfirmDialog`，危险确认走各页 Dialog + `busy-confirmation`。

### 4.5 `account`

| 命令 | 参数 | core | 危险 | 说明 |
|---|---|---|---|---|
| `list` | `[--agent]` | `list` | 否 | 脱敏 label、is_current、status |
| `switch` | `<id-or-label> --agent` | `switch` | **高** | 同 provider：lock → backfill → backup → apply → verify → DB |
| `import` | `[--agent] [--name]` | `import_live` | 中 | 导入 live 文件型凭据 |
| `add-apikey` | `--agent [--label] --key` | `add_api_key` | 中 | `--key -` 表示从 stdin 读，避免进 shell 历史 |
| `delete` | `<id-or-label> --agent` | `delete` | 中 | 仅删池内记录，不改 live |
| `oauth-url` | `--agent` | `start_oauth` | 否 | 只打印 authorize URL，不完成浏览器流 |
| `refresh` | `<id-or-label> --agent` | `refresh_token` | 中 | 用 refresh 换新 |
| `undo` | `--agent` | `undo_switch` | **高** | 一发撤销最近一次 switch |

文件型账号池：仅导入 adapter 声明支持的 live 凭据形态。无法在公开配置中可靠定位的官方登录态，import 返回 `unsupported`（退出码 `3`），不猜测路径。可入池但写回契约未锁定的 API Key，apply 到 live 仍 `unsupported`。

**OAuth**：GUI 完成已配置平台的 loopback PKCE；CLI 提供 `account oauth-url`（只出 URL）与 `account refresh`（用 refresh 换新），**不**把完整浏览器 PKCE 作为 CLI 主路径。CLI `oauth-url` 只打印 URL，进程退出后 loopback 失效。

### 4.6 `skill`

| 命令 | 参数 | core | 危险 | 说明 |
|---|---|---|---|---|
| `list` | 过滤 | `list` | 否 | 可 json 输出矩阵 |
| `list-installed` | | `list_installed` | 否 | 共享真源 + 各 Agent 私有技能根 |
| `sync` | `[--agent] [--all] [--force]` | `sync` | 中 | `--force` 冲突时覆盖；默认跳过冲突并 stderr 报告 |
| `enable` | `<skill> --agent [--force]` | 投影 copy | 中 | skill 名为真源目录名 |
| `disable` | `<skill> --agent` | `disable` | 中 | 只移除投影，默认不动真源 |
| `install` | `<path\|zip\|git> [--overwrite]` | `install_skill` | 中 | 装入共享真源 |
| `import-private` | `<skill> [--overwrite]` | `import_private_to_shared` | 中 | 私有 → 真源 |
| `uninstall` | `<skill> [--private --agent]` | `uninstall_skill` | **高** | 可删真源；需确认/`-y` |
| `update` | `<skill>` | `update_skill` | 中 | 按记录来源更新 |
| `project` | `<skill> --agent [--mode]` | `project_skill` | 中 | link \| copy |
| `market` | `[--query]` | `SkillMarketRegistry::with_defaults()`（skills.sh） | 否 | 远程搜索；安装走 GUI/Tauri `install_market_skill` 或本地 `skill install`；依赖网络与本机 Git |

删除真源已通过 `skill uninstall` 提供（危险操作需确认）；勿与 `disable`（仅去投影）混淆。

### 4.7 `usage`

| 命令 | 参数 | core | 说明 |
|---|---|---|---|
| `collect` | | `collect` | 增量采集；`-v` 打每 agent 进度 |
| `stats` | `--days` `--agent` `--model` | `summary` | `--days` 任意 ≥ 1，默认 7；输入/输出/缓存/成本估算 |
| `models` | `[--agent]` | `list_models` | **用量去重模型名**，非官方目录 |
| `health` | | `parser_health` | 与 Dashboard 用量段 ParserHealthBar 同数据 |

### 4.8 `backup`

| 命令 | 参数 | core | 危险 | 说明 |
|---|---|---|---|---|
| `list` | `[--agent]` | `list` | 否 | kind / files / size / note |
| `create` | `--agent [--note]` | `create` manual | 否 | |
| `restore` | `<backup-id>` | `restore` | **高** | 恢复前自动再备份当前 live |
| `delete` | `<backup-id>` | `delete` | **高** | 删快照与索引行；需确认/`-y` |

### 4.9 `config`（AgentHub 自身，不是 live）

| 命令 | 说明 |
|---|---|
| `path` | 打印 `data_dir`、`db_path`、`backups_dir`、`logs_dir` |
| `get [key]` | 无 key 则列出全部非敏感 settings（白名单见 §7.3）；有 key 则单值 |
| `set <key> <value>` | 仅允许白名单 key（§7.3）；`log_level` / `log_retention_days` 于**下次进程启动**应用 |

---

## 5. 退出码与错误

### 5.1 退出码（稳定契约）

| 码 | 含义 |
|---|---|
| `0` | 成功（含「有警告但可继续」，警告在 stderr 或 json.warnings） |
| `1` | 运行期失败（IO、解析、core `AppError`、锁超时等） |
| `2` | 用法错误（未知命令、缺参、非法 agent id、非法 flag） |
| `3` | 业务拒绝 / 能力不支持（如 **EnvNotReady**、Kimi account switch、无 skills 的 agent 上 enable） |
| `4` | 需要确认但未提供 `-y` 且非 TTY / 用户取消 |
| `5` | 部分成功（如 `skill sync --all` 部分 agent 失败；json 含 failed[]） |

### 5.2 错误输出

- 默认：stderr 人类可读；文案可带 i18n key 后缀便于搜日志，如 `[provider.switch.locked]`
- `--output json`：stderr 一行 JSON：`{"error":"...","code":"...","details":{}}`（details 无密钥）

---

## 6. GUI ↔ CLI 能力矩阵

| 能力 | GUI | CLI v1 | 备注 |
|---|---|---|---|
| Runtime 检测 / 引导安装 | ✅ Agents 页 / Env 条 | ✅ `env list` / `env install` | 两阶段 ensure_env |
| Agent 检测/安装/升级/卸载 | ✅ Agents 页 | ✅ list/install/upgrade/uninstall/outdated + `capabilities` | |
| Provider 列表/导入/切换/upsert | ✅ Connections | ✅ list/show/import/switch/presets/undo/test-latency | CLI 无 create/update/delete；GUI 有 upsert/delete |
| Account 池与切换 | ✅ Connections | ✅ list/import/apikey/switch/delete/undo | GUI/CLI 切换撤销均已接线 |
| OAuth 添加账号 | ✅ Claude/Codex/Grok | 🟡 oauth-url + refresh | 完整浏览器流以 GUI 为主 |
| Skills 矩阵 / 同步 / 安装 | ✅ Skills 页 | ✅ 全树（含 install/market/project） | market 默认 skills.sh；依赖网络与 Git |
| Usage 采集/图表 | ✅ Dashboard 用量段 | ✅ collect/stats/models/health | Cursor Unsupported |
| Backup 列表/创建/恢复/删除 | ✅ /settings?tab=backups | ✅ list/create/restore/delete | **导出包**未实现 |
| Chat 多 Agent | ✅ /chat | ❌（用 `run` 一次性 headless） | 过程面板 Phase 0–2 现行契约；Phase 3 展示层已落地。**无 Chat 模型选择器** |
| Projects | ✅ /projects | ❌ | 列表/删除/摘录；**不接**各家原生 `--resume` |
| Settings 主题/日志等 | ✅ L1 白名单 + OS 自启 | ✅ config get/set 白名单 | 主题/用量间隔/托盘/语言落 SQLite；`autoStart` 为 OS 登录项；GUI Settings chrome 可切换中/英。**全站 i18n 未做** |
| Doctor / 排障 | ✅ doctor report | ✅ doctor（含 runtimes + locks） | |
| 官方模型目录 | ❌ | ❌ | 非目标 |
| 备份导出 / DB 备份 | ❌ | ❌ | 预留目录 |

---

## 7. 配置契约（三层）

### 7.1 层 L0 — 启动定位（极薄，可文件可环境变量）

**目的**：只解决「数据目录在哪」。**不**承载 providers/accounts 业务。

#### 查找顺序（高 → 低）——现行只实现 1 / 2 / 4

1. CLI/GUI 显式 `--data-dir` / 开发参数  
2. 环境变量 `AGENTHUB_HOME`（绝对路径或可展开 `~`）  
3. 可选用户文件 `agenthub.toml`：**契约预留 / 当前未实现**（全仓无读取代码）  
4. 默认：`dirs::home_dir()/.agenthub`（**禁止**用 Git Bash 注入的错误 `HOME` 作为唯一依据；Windows 用 `dirs`）

**L0 现行只有** `--data-dir` 与 `AGENTHUB_HOME`。

#### 可选文件：`agenthub.toml`（契约预留 / 当前未实现）

文档曾写 `%USERPROFILE%\.agenthub\agenthub.toml`（或 `$AGENTHUB_HOME/agenthub.toml`）可带 `data_dir` / `log_level`。这是契约预留，**代码未读该文件**。若日后实现，仅允许：

```toml
# AgentHub 启动配置（可选）。业务数据在 agenthub.db，勿把 provider 写这里。
data_dir = "D:/data/agenthub"   # 可选；与 AGENTHUB_HOME 二选一习惯上 env 优先
log_level = "info"              # error | warn | info | debug | trace
```

**禁止**写入：api_key、oauth token、provider 列表、账号池。  
若日后实现且文件出现未知键：警告并忽略（前向兼容），不失败启动。

### 7.2 层 L1 — AgentHub 业务真源（SQLite）

路径：`{data_dir}/agenthub.db`（WAL）。

| 表 | 内容 |
|---|---|
| `providers` | 各 agent 的 API 配置池；`settings_config` 为 JSON（不建模死字段） |
| `accounts` | 账号池；`credentials` 按当前版本现有存储方案保存，输出统一脱敏 |
| `skills` | 技能记录 / 同步状态缓存（真源仍在 `~/.agents/skills` 文件系统） |
| `usage_records` | Token 明细 |
| `backups` | 备份索引 |
| `settings` | 应用设置键值（见 §7.3） |

另：

```text
{data_dir}/
├── agenthub.db
├── backups/db/
├── backups/live/<agent>/<ts>/
├── exports/
└── logs/                      # 按日文件 agenthub.YYYY-MM-DD；保留见 log_retention_days
    └── agenthub.YYYY-MM-DD    # tracing-appender daily；规范见 logging.md
```

`logs/`：CLI/GUI 共用；启动时按 `log_retention_days`（默认 14）清理过期按日文件。完整约定 → [logging.md](logging.md)。

### 7.3 `settings` 键白名单（`config get/set` 与 GUI 共用）

| key | 类型 | 说明 |
|---|---|---|
| `theme` | `dark` \| `light` \| `system` | core 权威；Settings Select 预览并立即落盘；启动 `getSettings` 对账；localStorage 仅首屏缓存 |
| `language` | `zh` / `zh-CN` \| `en` | 可落盘；GUI Settings 可切换中/英，core 为真源，localStorage 仅首屏缓存 |
| `log_level` | `error` \| `warn` \| `info` \| `debug` \| `trace` | 文件日志级别；**下次启动生效**；默认 `info` |
| `log_retention_days` | u32（1..=365） | 日志保留天数；默认 **14**；**下次启动** purge 时生效 |
| `skill_market_source` | `auto` \| `skills.sh` \| `skillhub.cn` | 远程技能市场源 |
| `close_to_tray` | bool | 关窗隐藏到托盘；写 L1 并同步 Tauri AppState；默认 true |
| `usage_collect_interval_min` | `Option<u32>` | `None`=从未写入（不序列化；GUI 默认 30）；`0`=仅手动；`1..=1440`=前台间隔分钟（非 OS 守护） |

白名单与 `settings_service::SETTINGS_WHITELIST` 一致。下列**不是** L1 `config set` 键：

- **开机自启**（GUI `autoStart`）：OS 登录项，不写 `settings` 表
- **`auto_backup`**：旧客户端残留字段已忽略；live 快照由核心服务固定触发，无关闭开关
- **`data_dir`**：L0 启动定位（`--data-dir` / `AGENTHUB_HOME`）；`config path` 只读展示

只读展示（get 可给，set 拒绝）：`app_version`。当前版本不提供主密码或凭据存储方式切换。

日志优先级摘要：`-v` > settings > 默认。环境变量 / `agenthub.toml` 的 `log_level` 为契约预留、**未实现**。详见 [logging.md](logging.md) §5。

### 7.4 层 L2 — 各 Agent live 文件（第三方，Adapter 读写）

各 Agent 的主配置、认证与技能目录由对应 Adapter 声明（`agent_home` / `live_backup_paths` / `skills_dir`）。文档不维护完整凭据落点清单，避免与上游版本漂移。

规则：

- AgentHub **改 live 前**必须：`live_backup_paths` → `backup_service.snapshot`
- TOML 使用 `toml_edit` 合并受管字段，保留未管理区段和 MCP；不承诺所有格式化细节完全不变
- 路径必须经 `is_safe_path`；拒绝危险字符
- **共享技能真源**：`~/.agents/skills/`（及 lock 清单）；投影到各 Agent 技能目录

### 7.5 层 L3 — 内置预设与元数据（只读产品资源）

| 资源 | 位置（实现目标） | 说明 |
|---|---|---|
| Agent 元数据 | core + 前端 `config/agents.ts` 展示同步 | id、能力位、安装渠道、**渠道 requires RuntimeId** |
| Runtime 元数据 | core `runtime/` | id、检测方式、min_version、bootstrap 渠道（winget/url/命令） |
| Provider 预设 | **P1 起以 core 为准**；前端 presets 为镜像 | `anthropic` / `openai-compatible` 等模板 |
| 定价表 | `usage/pricing.rs` + `embedded-pricing.json` | 成本估算（USD；离线快照；`pnpm pricing:update` 同步 LiteLLM） |
| Logo/文案 | `src-tauri/icons/`、UI 文案；前端无顶层 `assets/`；改图标：`pnpm icons`（桌面专用，勿裸跑 `tauri icon`） | |

用户「另存为供应商」写入 L1 `providers`，不改 L3 仓库文件。

### 7.6 三层关系（不得双真源）

```text
L0 启动    →  只定位 data_dir（现行：`--data-dir` / `AGENTHUB_HOME`；`agenthub.toml` 契约预留未实现）
L1 SQLite  →  AgentHub 业务唯一真源（池、用量、备份索引、settings）
L2 live    →  各 Agent 运行时读的文件；由 switch/sync 从 L1 投影或写回
L3 内置    →  只读模板；不是用户状态

错误示范：在 agenthub.toml 里存 api_key 或 provider 列表
错误示范：以 live 为唯一真源却又在 DB 存一份长期不 backfill
```

**backfill**：live → L1 当前 provider/account（防手改丢失）  
**backup**：live → `{data_dir}/backups/live/...`（防写坏回滚）  
二者不可互相替代。

### 7.7 导出包（换机）

- 路径：`{data_dir}/exports/`
- 内容：L1 逻辑备份（providers/accounts 等）；导出能力尚未实现，按现有存储方案处理，不规划额外加密层
- CLI 二期：`agenthub export` / `import`

---

## 8. 实现分期（CLI / 配置）

| 阶段 | 交付 |
|---|---|
| **P0** | clap 骨架、`doctor`（**runtimes**+detect+paths）、`env list`、`agent list`、`config path/get`；L0 环境变量 + 默认 data_dir；db 迁移 |
| **P1** | `provider list/show/switch/import-live`、`skill list/sync/enable/disable`、`backup *`、`-o json`/`-y`、退出码；presets 进 core；agent list 附 `envReady` |
| **P2** | `account *`、`usage *`、`agent install/upgrade/uninstall`（两阶段 + `--install-deps`）、`env install`；可选 `agenthub.toml`（**契约预留 / 当前未实现**） |
| **P3+** | export/import CLI、oauth 辅助、更细 doctor / PATH 诊断 |

---

## 9. 验收清单（Definition of Done）

实现合并前至少满足：

- [ ] 任意写 live 的 CLI 路径与 GUI 走同一 service（切换可单测 backfill+backup 顺序）
- [ ] `agenthub provider switch ...` 无 `-y` 非 TTY → 退出码 `4`，不写盘
- [ ] `agenthub usage models` 仅来自 usage 表去重，文档/帮助字符串写明「非官方目录」
- [ ] `agenthub config set` 拒绝白名单外 key；拒绝写入密钥类 key
- [ ] `AGENTHUB_HOME` 与 `--data-dir` 优先级符合 §7.1
- [ ] GUI 运行中 CLI `switch` 不损坏文件（锁竞争时明确错误，而非各写各的）
- [ ] `--output json` 成功时 stdout 可被 `jq` 解析；密钥字段脱敏
- [ ] 帮助与本文命令树一致（无文档外隐藏写命令）
- [ ] 无 Node 时 `agenthub agent install codex`（npm 渠道）→ 退出码 `3`、`code=env.not_ready`，**不**执行 `npm install`
- [ ] `doctor` / `env list` 均报告 nodejs/npm 状态；字段与 GUI EnvStatusBar 同源
- [ ] `agent uninstall` 不卸载 Node；无 `env uninstall` 命令

---

## 10. 修订记录

| 版本 | 日期 | 说明 |
|---|---|---|
| v1.0 | 2026-07-27 | 初版：命令树 freeze、退出码、GUI 矩阵、L0–L3 配置契约、验收清单 |
| v1.1 | 2026-07-27 | 前置环境：`env` 资源、doctor runtimes、`agent install --install-deps`、EnvNotReady 契约 |
| v1.2 | 2026-07-27 | 平台环境差异：`doctor`/`env list` 仅返回宿主相关 Runtime；`env install` 默认 channel 与 native 底层命令按 Windows/macOS 分流 |
| v1.3 | 2026-08-15 | 命令树补齐 `agent outdated` / `backup delete` / `provider undo|test-latency` / `account undo`；doctor ⑤ Locks；`config get` 全白名单 + 只读 `app_version`；JSON 错误带 details；EnvNotReady 退出码 3 |
| v1.4 | 2026-08-15 | `agent uninstall --purge-config` 只走 core 一次 PreUninstall 备份；`env install` 无 brew/winget → `env.not_ready`（退出码 3）；`skill sync` 下沉 `SkillService::sync_targets`；`--quiet` 不再泄漏 capabilities markdown / install logs；`oauth-url` 标明 loopback 随进程退出 |
