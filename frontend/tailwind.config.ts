import type { Config } from 'tailwindcss';

/**
 * Every colour resolves to a CSS variable defined in globals.css, so the dark
 * and light themes swap by flipping `data-theme` on <html> with no `dark:`
 * variants in the markup. The palette is monochrome (ink on paper); the
 * spec's own names (ink, ink-2…, rule, invert) are available next to the
 * older aliases.
 */
const config: Config = {
  content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  darkMode: ['selector', '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: {
        bg: 'var(--color-bg)',
        'bg-soft': 'var(--color-bg-soft)',
        surface: 'var(--color-surface)',
        'surface-2': 'var(--color-surface-2)',
        border: 'var(--color-border)',
        'border-soft': 'var(--color-border-soft)',
        text: 'var(--color-text)',
        soft: 'var(--color-text-soft)',
        mute: 'var(--color-text-mute)',
        strong: 'var(--color-text-strong)',
        accent: 'var(--color-accent)',
        'accent-2': 'var(--color-accent-2)',
        'on-accent': 'var(--color-on-accent)',
        positive: 'var(--color-positive)',
        negative: 'var(--color-negative)',
        warning: 'var(--color-warning)',
        faint: 'var(--color-text-faint)',
        ink: 'var(--ink)',
        'ink-2': 'var(--ink-2)',
        'ink-3': 'var(--ink-3)',
        'ink-4': 'var(--ink-4)',
        rule: 'var(--rule)',
        'rule-soft': 'var(--rule-soft)',
        'bg-alt': 'var(--bg-alt)',
        invert: 'var(--invert-bg)',
        'invert-ink': 'var(--invert-ink)',
        'invert-ink-3': 'var(--invert-ink-3)',
        'invert-rule': 'var(--invert-rule)',
      },
      // Square corners: 0 or 2px everywhere; pills only for tiny status dots and spinners.
      borderRadius: {
        none: '0',
        DEFAULT: '2px',
        sm: '2px',
        md: '2px',
        lg: '2px',
        xl: '2px',
        '2xl': '2px',
        '3xl': '2px',
        full: '9999px',
      },
      fontFamily: {
        sans: ['var(--font-sans)', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['var(--font-mono)', 'ui-monospace', 'SFMono-Regular', 'monospace'],
        display: ['var(--font-display)', 'var(--font-mono)', 'ui-monospace', 'monospace'],
      },
      fontSize: {
        '2xs': ['0.6875rem', { lineHeight: '1rem' }],
      },
      minHeight: {
        10: '2.5rem',
      },
    },
  },
  plugins: [],
  corePlugins: {
    // No shadows anywhere: surfaces are told apart by a 1px rule.
    boxShadow: false,
  },
};

export default config;
