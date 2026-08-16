# 硬编码治理

本文件记录 AgentHub 对「硬编码」的分层策略与落地状态。产品决策「不做凭据落盘加密」见 [AGENTS.md](../AGENTS.md)。

## 原则

| 级别 | 含义 | 处理 |
|------|------|------|
| L0 禁止 | 密钥、本机隐私缓存进仓 | 删除 + `.gitignore` + 写入 `data_dir` |
| L1 契约单点 | 安装渠道、OAuth 接入、agent 家目录 | `catalog` / `oauth` / `paths` 单点（勿在 docs 重复抄写密钥/端点） |
| L2 默认值 | 超时、扫描上限、日志保留 | `catalog::limits` |
| L3 镜像债 | 前端 `src/config/*` 静态模板 | 过渡期用测试锁漂移；目标改读 core API |
| L4 生成物 | `embedded-pricing.json` | 脚本生成，运行时不拉网 |

## 模块地图

```text
crates/agenthub-core/src/catalog/
  install.rs   # façade；真源 `platform/install`（InstallContribution + catalog API）
  limits.rs    # 超时 / 容量 / TTL / Node 最低主版本
  market.rs    # skills.sh / skillhub.cn base URL、UA、SkillMarketSource
```

- OAuth 接入配置：`crates/agenthub-core/src/oauth/`（**勿**把 client/端点常量贴进公开文档或 issue）
- Agent 家目录：`crates/agenthub-core/src/utils/paths.rs`
- 定价表：`pnpm pricing:update` → `crates/agenthub-core/src/usage/embedded-pricing.json`

## 环境变量

| 变量 | 作用 |
|------|------|
| `AGENTHUB_HOME` | 数据目录（含 `project_session_index.json`） |
| `AGENTHUB_SKILLS_SH_BASE` | skills.sh 源站 base（默认 `https://skills.sh`） |
| `AGENTHUB_SKILLHUB_API_BASE` | skillhub API base（默认 `https://api.skillhub.cn`） |

## 运行时文件

以下文件**只**应出现在 `data_dir`（默认 `~/.agenthub`），**不得**提交：

- `project_session_index.json`
- `project_metadata.json`

已由根目录 `.gitignore` 忽略。

## GUI 安装目录

- Core：`catalog::list_install_catalog`（唯一 allowlist + 展示命令）
- Tauri：`list_install_catalog_cmd`
- 启动：`main.tsx` → `loadAgentCatalog` / `applyAgentCatalog`（含 install channels + capabilities）
- `src/config/agents.ts` 仅 UI 装饰（name / letter；`color` 绑定 `agentCssVar`）
- 主题 / Agent 品牌色 hex 真源：`src/styles/tokens.ts`（禁止在 globals / index.html / 组件内再抄一套）
- mock：`src/dev/mocks/fixtures/install-catalog.ts`（仅 dev:mock / vitest）

改安装渠道时：改 `platform/install`（`catalog/install.rs` 只是 façade）；mock 快照若用于 UI 开发需同步。  
改配色时：只改 `src/styles/tokens.ts`。

## 后续（未做）

1. GUI preset / runtime 元数据改由 core API 下发，删除 `src/config/presets` 镜像。  
2. OAuth / install 契约仅在 core 内维护；若写 docs，只描述流程与安全约束，不抄 client/端点。  
3. mock 安装快照改为从 core 导出 JSON，避免手写漂移。
