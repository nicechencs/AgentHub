import type { Config } from 'tailwindcss';
import { buildTailwindFontSize } from './src/styles/tokens';

export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    // 覆盖默认 2xl/3xl… 阶，只保留 TYPE_SCALE 三档 + 同像素别名
    fontSize: buildTailwindFontSize(),
    // 全量替换默认阶，只留语义名；sm/md/lg/xl 不再生成
    borderRadius: {
      none: '0px',
      btn: 'var(--radius-sm)', // 6px：按钮、输入、菜单项、小控件
      card: 'var(--radius)', // 8px：卡片、弹层、嵌套面板、代码井、列表行
      composer: 'var(--radius-lg)', // 12px：Chat 输入壳、用户气泡
      mark: 'var(--radius-mark)', // 22%：仅 AppLogo 产品标
      full: '9999px',
    },
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
