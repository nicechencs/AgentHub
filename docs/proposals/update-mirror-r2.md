---
title: 国内自动更新镜像（GitHub Release → Cloudflare R2 + Worker）
type: proposal
status: proposed
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-08
---

# 国内自动更新镜像（GitHub Release → Cloudflare R2 + Worker）

Status: proposed。YAML 保持 proposed：运维资源与国内真机未闭环，不能当现行运维契约。**App 镜像优先已在正式包（0.4.9）**，不要把本页当成未开工。现行 updater 入口见 [STATUS](../STATUS.md)。

2026-09-07 与用户确认方向：中国用户常连不上 GitHub，导致 App 无法拉取更新；在现有 GitHub Release + `latest.json` + `.sig`（Tauri updater）之上，发版后自动镜像到 Cloudflare R2，可选 Worker 对外提供更新入口。

## 进度（已发 / 剩余运维）

| 状态 | 内容 |
| --- | --- |
| **已发（0.4.9）** | App updater 优先 `https://updates.agenthub.qooo.io/latest.json`，失败回退 GitHub |
| **已发** | Release CI 在 secrets 齐全时同步安装包 / `.sig` / 改写后的清单到 R2 |
| **已发** | in-repo Worker `cloudflare/update-mirror` |
| **剩余运维** | Cloudflare 桶 / 自定义域 / WAF 由运维配置；本仓库不声称云端资源已建好 |
| **剩余验收** | 国内真机：检查更新 → 下载 → 验签 → 安装 |

运维一次性清单见下文「运维备忘」。

## 问题

- AgentHub 正式发版走 GitHub `release` + `v*` tag，产物含安装包、`.sig`、`latest.json`。
- 国内网络访问 GitHub Releases / `objects.githubusercontent.com` 不稳定或不可达时，应用内检查更新会失败。
- 卡点是**下载清单与安装包的 URL**，不是签名校验本身。

## 目标

1. 发版流程仍以 GitHub 为权威源（tag、CI、签名）。
2. 同一批产物自动同步到国内可访问的对象存储（首选 Cloudflare R2）。
3. App updater 默认读镜像上的 `latest.json`；可选失败时回退 GitHub。
4. 全程自动化：人只打 tag / 推 release，不必手动拷文件。

## 非目标

- 不自建复杂版本库或私有更新协议。
- 不在对象存储或 Worker 里做现场签名；私钥只留在 CI。
- 不提供无签名旁路包。
- 不保证 Cloudflare 在国内 100% 可达；必要时可再备国内 OSS。

## 架构

| 组件 | 职责 |
|---|---|
| GitHub Release CI | 编包、签名、生成 `latest.json`、发布 Release |
| R2 | 存 `latest.json`、各平台安装包、对应 `.sig` |
| Worker（可选） | 自定义域名路由到 R2；改写清单 URL、缓存头、简单统计/回退 |
| App Tauri updater | 检查 `https://updates.<domain>/latest.json`，验签后安装 |

Worker 非必须：R2 自定义域名也可直出。Worker 便于改写 URL、灰度与 GitHub 回退。

## 自动化流水线

1. 现有 Release workflow：构建 → 签名 → 上传 GitHub Release → 写出 `latest.json`。
2. **同 workflow 追加步骤**：把产物上传到 R2；写入/覆盖镜像侧 `latest.json`（下载 URL 指向镜像域名，避免清单在 R2、包链回 GitHub）。
3. Worker / 自定义域名常驻，发版时一般不用改。
4. 用户打开 App → 检查镜像 `latest.json` → 拉包 → 验 `.sig` → 安装。

一次性配置：R2 桶、（可选）Worker 路由、CI Secrets（`R2_ACCOUNT_ID` / `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` / 桶名）。之后发版与现在相同。

## App 侧

- updater endpoint 改为镜像 URL（或「镜像优先，GitHub 回退」）。
- 继续只信任现有公钥验签；镜像被篡改但无合法 `.sig` 时拒绝安装。
- 在国内网络真机验收：检查更新、下载、验签、安装全路径。

## 成本与运维注意

- 大安装包以 R2 直出为主，Worker 适合清单等小请求。
- 关注 R2 出站流量费用。
- 私钥永不进 R2 / Worker 环境变量。
- 若 Cloudflare 个别网络仍差，可并行同步国内 OSS，App 做多源。

## 实施清单

1. 建 R2 桶 + 自定义域名（或 Worker 路由）。
2. Release CI 增加同步步骤；改写 `latest.json` 内 URL。
3. App 配置镜像 endpoint（可选双源）。
4. 国内真机点测自动更新。
5. 文档：发版说明中写明镜像为用户更新源、GitHub 仍为发布权威源。

## 流量与滥用防护

- **CDN / WAF**：在 Cloudflare 仪表盘对主机名 `updates.agenthub.qooo.io`（仅此子域，勿改 apex `agenthub.qooo.io`）启用 Bot Fight Mode，并按 IP 对 `latest.json` 与大文件 GET 做速率限制。Worker 本身只放行白名单路径，根路径与未知对象一律 404，R2 桶保持私有、无公开列举。
- **客户端签名校验**：App 仍用 `src-tauri/tauri.conf.json` 内嵌公钥校验 `.sig`。镜像被篡改但无合法签名时拒绝安装。
- **私钥边界**：`TAURI_SIGNING_PRIVATE_KEY` 只存在于 GitHub Actions；**永不**写入 R2、Worker 环境变量或本仓库。

## 实施进度

- 2026-09-07：落地 in-repo Worker（`cloudflare/update-mirror`）、Release CI R2 同步（secrets 缺失则跳过）、`latest.json` URL 改写脚本与单测、App updater 镜像优先 + GitHub 回退（`https://updates.agenthub.qooo.io/latest.json`）。
- Cloudflare 桶 / 自定义域 / WAF 由运维自行创建；本变更不声称已在云端建好资源。
- **待验项**：国内真机检查更新 → 下载 → 验签 → 安装全路径（DNS/Worker 就绪后）。

## 运维备忘（CI Secrets 与一次性 CF）

GitHub repo secrets：`R2_ACCOUNT_ID`、`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、`R2_BUCKET`（建议 `agenthub-updates`）、`UPDATE_MIRROR_BASE_URL`（`https://updates.agenthub.qooo.io`）。

一次性 CF（操作者本地完成，勿动 apex）：

1. 建私有 R2 桶 `agenthub-updates`。
2. 发 R2 S3 API 凭证给 CI。
3. 填 `cloudflare/update-mirror/wrangler.toml` 的 `account_id` 后 `npx wrangler deploy`。
4. 仅为 `updates.agenthub.qooo.io` 绑定 Workers Custom Domain / DNS。
5. 仪表盘配置 WAF / Bot Fight / rate limit。

## 决议记录

- 2026-09-07：用户确认方向可行，要求自动化；选定 Cloudflare R2 +（可选）Worker 作为首选镜像方案。
