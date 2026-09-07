---
title: 国内自动更新镜像（GitHub Release → Cloudflare R2 + Worker）
type: proposal
status: proposed
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-07
---

# 国内自动更新镜像（GitHub Release → Cloudflare R2 + Worker）

Status: proposed。2026-09-07 与用户确认方向：中国用户常连不上 GitHub，导致 App 无法拉取更新；在现有 GitHub Release + `latest.json` + `.sig`（Tauri updater）之上，发版后自动镜像到 Cloudflare R2，可选 Worker 对外提供更新入口。

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

## 实施清单（待开发）

1. 建 R2 桶 + 自定义域名（或 Worker 路由）。
2. Release CI 增加同步步骤；改写 `latest.json` 内 URL。
3. App 配置镜像 endpoint（可选双源）。
4. 国内真机点测自动更新。
5. 文档：发版说明中写明镜像为用户更新源、GitHub 仍为发布权威源。

## 决议记录

- 2026-09-07：用户确认方向可行，要求自动化；选定 Cloudflare R2 +（可选）Worker 作为首选镜像方案。
