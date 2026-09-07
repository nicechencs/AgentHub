# AgentHub update mirror (Cloudflare Worker + R2)

Serves Tauri updater assets for domestic networks.

- Public base URL: `https://updates.agenthub.qooo.io`
- R2 bucket (intended): `agenthub-updates`
- Worker binding: **`UPDATES_BUCKET`** (required)
- Hostname scope: **only** `updates.agenthub.qooo.io` — do not attach this Worker to apex `agenthub.qooo.io`

## What is served

Flat object keys only (no directories, no listing):

| Path | Purpose |
|---|---|
| `/latest.json` | Tauri static updater feed (download URLs point at this mirror) |
| `/AgentHub_*.{exe,msi,dmg,deb,AppImage,app.tar.gz}` | Installers |
| matching `*.sig` | Updater signatures |

Everything else (including `/`) → **404**.

## Security

- Signing private key stays **CI-only** (`TAURI_SIGNING_PRIVATE_KEY`). Never put it in R2, Worker secrets, or this repo.
- Clients verify `.sig` with the pubkey in `src-tauri/tauri.conf.json`.
- WAF / Bot Fight / rate limits: configure in Cloudflare dashboard for `updates.agenthub.qooo.io` only.

## One-time Cloudflare setup (operator)

1. Create private R2 bucket `agenthub-updates` (no public access / no listing).
2. Create R2 S3 API token (Object Read & Write) for CI sync.
3. Set `account_id` in `wrangler.toml`, then `npx wrangler deploy` from this directory.
4. Attach **Workers Custom Domain** `updates.agenthub.qooo.io` only (leave apex alone).
5. Optional: Bot Fight + rate limit on this hostname.

## GitHub Actions secrets

| Secret | Example / purpose |
|---|---|
| `R2_ACCOUNT_ID` | Cloudflare account id |
| `R2_ACCESS_KEY_ID` | R2 S3 access key |
| `R2_SECRET_ACCESS_KEY` | R2 S3 secret |
| `R2_BUCKET` | `agenthub-updates` |
| `UPDATE_MIRROR_BASE_URL` | `https://updates.agenthub.qooo.io` |

Missing secrets → Release CI skips mirror sync with a warning (GitHub Release still succeeds).
