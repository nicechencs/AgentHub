# Agent logos

界面上的 Agent 方标只通过 `AgentLogo` 展示。网上的原图只当来源，不直接上界面。商标归各自所有者。

接入新 Agent 时按本页改编一枚图，再在 `src/config/agents.ts` 的 `AGENT_DISPLAY` 登记。步骤总览见 [添加 Agent](../../../docs/guides/adding-an-agent.md)。

## 格子

- 正方形圆角框（`rounded-mark`），细边。圆角只裁外框。
- 列表 **24×24**（`AgentLogo` 默认 `sm`）；头像 / 选择卡片 **32×32**（`md`）。不要再加第三档。
- 背景由格子给。不要随浅色/深色界面反色商标。
- 光学边距做进 SVG，组件不再加内边。

## 两种装法

在 `AGENT_DISPLAY.logoFit` 里声明，缺省 `glyph`。

| 装法 | 何时 | 要求 |
| --- | --- | --- |
| `glyph` | 官方素材是透明底符号 | 正方形画布；图形视觉上约占格子的 70%～75%；`logoBackground` 为对比底（多数白，Grok / Cursor 深色） |
| `bleed` | 官方素材已经是铺满的应用图标 | 正方形画布，贴边裁进圆角框；`logoBackground` 与图标底色一致，避免露边 |

不要在 SVG 里再画一层圆角底或圆形底（会和格子套两层）。不要给每家写 CSS `scale`。

## 收货

1. 优先官方静态 SVG；去掉脚本、外链、嵌入远程内容。没有可靠 SVG 时才用 PNG（当前仅 ZCode）。
2. 画布必须是正方形（`viewBox` 宽高相等）。符号型按 24×24 网格把图形收到中间安全区；整图铺满。
3. 把新图和现有方标排成一行，在 **24px 和 32px** 下用眼睛看：不明显偏大或偏小，并且一眼能认出是谁。细、疏的图形可以略大于 75%，以视觉重量为准，不按路径外框自动裁。
4. 只通过 `AgentLogo` 展示；连接池供应商图若复用同一文件，也走改编后的版本。

## 来源

`AgentLogo` 优先加载 SVG，失败后加载 PNG；没有图则回退首字母。Pi 的 SVG 去掉了随系统主题变白的规则，保证浅色底上可见。

| 本地资源 | 对应集成 | 装法 | SVG 来源 | PNG 来源 |
| --- | --- | --- | --- | --- |
| `claude.svg` / `.png` | Claude | glyph | [theSVG Claude Code](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/claude-code/color.svg) | [claude.ai](https://claude.ai) |
| `codex.svg` | Codex | glyph | [theSVG Codex (OpenAI)](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/codex-openai/default.svg) | — |
| `kimi.svg` | Kimi Code | glyph | [KIMI 官方品牌指南：K Only 浅色背景](https://moonshotai.github.io/Branding-Guide/scenarios/04-k-only/k-only-light.svg) | — |
| `grok.svg` / `.png` | Grok | glyph | [theSVG Grok](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/grok/default.svg) | [grok.com](https://grok.com) |
| `pi.svg` / `.png` | Pi | glyph | [theSVG Pi](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/pi/default.svg) | [pi.dev](https://pi.dev) |
| `workbuddy.svg` | WorkBuddy | bleed | [WorkBuddy 官方页面 logo.svg](https://download.codebuddy.ai/web/workbuddy/00aa368996ce0f8793afd87db1bcdf458d8ba952/assets/logo.svg) | — |
| `cursor.svg` / `.png` | Cursor | glyph | [theSVG Cursor](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/cursor/default.svg) | [cursor.com](https://cursor.com) |
| `deepseek.svg` / `.png` | DeepSeek（`dsh`） | glyph | [theSVG DeepSeek](https://cdn.jsdelivr.net/gh/glincker/thesvg@main/public/icons/deepseek/default.svg) | [deepseek.com](https://deepseek.com) |
| `zcode.png` | ZCode | bleed | —（公开 SVG 是位图封装，未采用） | [z.ai](https://z.ai) |
| `kiro.svg` | Kiro | bleed | [kiro.dev 官方 icon.svg](https://kiro.dev/icon.svg) | — |
| `gemini.svg` | Gemini（连接池供应商图） | glyph | lobe-icons Gemini | — |

说明：OpenAI 官方品牌页目前公开的是 OpenAI Blossom，并未提供可直接下载的 Codex 专属 SVG。`codex.svg` 保留 theSVG 的 Codex 专属轮廓，并按 OpenAI 官方 Codex 页面展示的蓝紫渐变云朵与白色终端符号配色处理。
