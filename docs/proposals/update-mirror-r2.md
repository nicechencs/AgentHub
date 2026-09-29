---
title: 国内自动更新镜像（GitHub Release → Cloudflare R2 + Worker）
type: proposal
status: proposed
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-29
---

# 国内自动更新镜像（GitHub Release → Cloudflare R2 + Worker）

> 状态：proposed。仓库内代码已全部在正式包中（自 0.4.9）；剩下的是云端运维配置和国内真机验收，所以仍不是现行运维契约。现行 updater 入口见 [STATUS](../STATUS.md)。

国内常连不上 GitHub，App 拉不到更新。卡点是清单和安装包的下载地址，不是验签。

## 当前基线

| 项 | 状态 | 证据 |
| --- | --- | --- |
| App 先查镜像，失败回退 GitHub | 已发 | `src-tauri/tauri.conf.json` 的 `plugins.updater.endpoints`：`https://updates.agenthub.qooo.io/latest.json` → GitHub |
| Release CI 同步安装包、`.sig`、改写后的清单到 R2 | 已发；secrets 缺失时跳过 | `.github/workflows/release.yml`（`R2_MIRROR_SYNC`） |
| 清单 URL 改写 | 已发 | `scripts/rewrite-latest-json-mirror.mjs` 及其 `.test.mjs` |
| Worker（只放行白名单路径，其余 404） | 代码在仓库 | `cloudflare/update-mirror/` |
| R2 桶、自定义域、WAF | 运维负责；仓库不声称已建好 | — |
| 国内真机：检查 → 下载 → 验签 → 安装 | **未验** | — |

## 目标

1. GitHub 仍是发布权威源（tag、CI、签名）。
2. 同一批产物自动同步到 R2，人只打 tag。
3. App 镜像优先，GitHub 回退。

## 门槛（转为现行前）

- 国内真机全路径通过。
- 运维确认桶、域名、WAF / 速率限制就绪。
- 运维步骤迁到 `docs/operations/` 的现行页。

## 非目标

- 不自建版本库或私有更新协议。
- 不在 R2 / Worker 签名；`TAURI_SIGNING_PRIVATE_KEY` 只在 GitHub Actions。
- 不提供无签名包。镜像被篡改而签名不合法时，App 拒绝安装。
- 不保证 Cloudflare 国内全可达；需要时再加国内 OSS。

## 运维备忘

GitHub secrets：`R2_ACCOUNT_ID`、`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、`R2_BUCKET`（建议 `agenthub-updates`）、`UPDATE_MIRROR_BASE_URL`（`https://updates.agenthub.qooo.io`）。

一次性 Cloudflare 配置（只动 `updates.agenthub.qooo.io`，不动 apex `agenthub.qooo.io`）：

1. 建私有 R2 桶 `agenthub-updates`，不开公开列举。
2. 生成 R2 S3 API 凭证给 CI。
3. 填 `cloudflare/update-mirror/wrangler.toml` 的 `account_id`，运行 `npx wrangler deploy`。
4. 为 `updates.agenthub.qooo.io` 绑定 Workers Custom Domain。
5. 开 Bot Fight Mode，并按 IP 对 `latest.json` 和大文件 GET 限速。

注意 R2 出站流量费用；大文件由 R2 直出，Worker 只处理小请求。

## 未决问题

- 真机验收由谁、在哪种网络下做。
- 是否需要国内 OSS 作为第二镜像。
