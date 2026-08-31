# Agent logos

这些图标用于在 AgentHub 中识别对应集成，商标归各自所有者。`AgentLogo`
会优先加载 SVG、失败后加载 PNG；没有可靠 PNG 回退的集成会回退到首字母。SVG 已检查为没有脚本、外链或嵌入远程内容的静态文件。
Pi 的 SVG 移除了随系统主题改成白色的规则，以保证它在头像的浅色底上始终可见。

| 本地资源 | 对应集成 | SVG 来源 | PNG 来源 |
| --- | --- | --- | --- |
| `claude.svg` / `.png` | Claude | [theSVG Claude Code](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/claude-code/color.svg) | [claude.ai](https://claude.ai) |
| `codex.svg` | Codex | [theSVG Codex (OpenAI)](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/codex-openai/default.svg) | — |
| `kimi.svg` | Kimi Code | [KIMI 官方品牌指南：K Only 浅色背景](https://moonshotai.github.io/Branding-Guide/scenarios/04-k-only/k-only-light.svg) | — |
| `grok.svg` / `.png` | Grok | [theSVG Grok](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/grok/default.svg) | [grok.com](https://grok.com) |
| `pi.svg` / `.png` | Pi | [theSVG Pi](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/pi/default.svg) | [pi.dev](https://pi.dev) |
| `workbuddy.svg` | WorkBuddy | [WorkBuddy 官方页面 logo.svg](https://download.codebuddy.ai/web/workbuddy/00aa368996ce0f8793afd87db1bcdf458d8ba952/assets/logo.svg) | — |
| `cursor.svg` / `.png` | Cursor | [theSVG Cursor](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/cursor/default.svg) | [cursor.com](https://cursor.com) |
| `deepseek.svg` / `.png` | DeepSeek（`dsh`） | [theSVG DeepSeek](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/deepseek/default.svg) | [deepseek.com](https://deepseek.com) |
| `zcode.png` | ZCode | —（公开 SVG 是位图封装，未采用） | [z.ai](https://z.ai) |

说明：OpenAI 官方品牌页目前公开的是 OpenAI Blossom，并未提供可直接下载的 Codex 专属 SVG。`codex.svg` 保留 theSVG 的 Codex 专属轮廓，并按 OpenAI 官方 Codex 页面展示的蓝紫渐变云朵与白色终端符号配色处理。
