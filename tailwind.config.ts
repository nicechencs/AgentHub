import type { Config } from 'tailwindcss';
import { buildTailwindFontSize } from './src/styles/tokens';

export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    // 覆盖默认 2xl/3xl… 阶，只保留 TYPE_SCALE 三档 + 同像素别名
    fontSize: buildTailwindFontSize(),
    extend: {
      colors: {
        // 设计 token 真源：src/styles/tokens.ts → CSS 变量（docs/ui-design.md §2）
        // 命名避开 Tailwind font-size 的 base,否则 text-base 会被同时解析成文字颜色
        canvas: 'var(--bg-canvas)',
        panel: 'var(--bg-panel)',
        subtle: 'var(--bg-subtle)',
        hover: 'var(--bg-hover)',
        active: 'var(--bg-active)',
        border: {
          DEFAULT: 'var(--border)',
          strong: 'var(--border-strong)',
        },
        primary: 'var(--text-primary)',
        secondary: 'var(--text-secondary)',
        muted: 'var(--text-muted)',
        disabled: 'var(--text-disabled)',
        accent: 'var(--accent)',
        success: 'var(--success)',
        warning: 'var(--warning)',
        danger: 'var(--danger)',
        info: 'var(--info)',
        claude: 'var(--agent-claude)',
        codex: 'var(--agent-codex)',
        kimi: 'var(--agent-kimi)',
        grok: 'var(--agent-grok)',
        pi: 'var(--agent-pi)',
        workbuddy: 'var(--agent-workbuddy)',
        cursor: 'var(--agent-cursor)',
      },
      maxWidth: {
        content: '1200px',
      },
      borderRadius: {
        // 语义真源：docs/ui-design.md §2 — 6 控件 / 8 卡片 / 12 composer
        // 优先用 btn | card | composer；sm/md/lg 为兼容别名
        sm: 'var(--radius-sm)',
        DEFAULT: 'var(--radius)',
        md: 'var(--radius)',
        lg: 'var(--radius-lg)',
        btn: 'var(--radius-sm)', // 6px：按钮、输入、菜单项、小控件
        card: 'var(--radius)', // 8px：卡片、弹层、列表行、分段轨
        composer: 'var(--radius-lg)', // 12px：Chat 输入壳、大气泡等
      },
      boxShadow: {
        xs: 'var(--shadow-xs)',
        sm: 'var(--shadow-sm)',
        md: 'var(--shadow-md)',
        lg: 'var(--shadow-lg)',
      },
      fontFamily: {
        sans: [
          'system-ui',
          '-apple-system',
          '"Segoe UI"',
          'Roboto',
          '"PingFang SC"',
          '"Microsoft YaHei"',
          'sans-serif',
        ],
        mono: ['"JetBrains Mono"', 'Consolas', 'Menlo', 'monospace'],
      },
    },
  },
  plugins: [],
} satisfies Config;
