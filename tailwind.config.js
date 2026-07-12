/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        primary: {
          DEFAULT: 'var(--color-primary)',
          hover: 'var(--color-primary-hover)',
        },
        accent: 'var(--color-accent)',
        bg: 'var(--color-bg)',
        surface: 'var(--color-surface)',
        sidebar: 'var(--color-sidebar)',
        border: 'var(--color-border)',
        text: {
          primary: 'var(--color-text-primary)',
          secondary: 'var(--color-text-secondary)',
          tertiary: 'var(--color-text-tertiary)',
        },
        heatmap: {
          bg: 'var(--color-heatmap-bg)',
          low: 'var(--color-heatmap-low)',
          mid: 'var(--color-heatmap-mid)',
          high: 'var(--color-heatmap-high)',
        },
        success: 'var(--color-success)',
      },
      fontFamily: {
        display: ['Segoe UI Variable Display', 'Segoe UI', 'sans-serif'],
        body: ['Segoe UI', 'sans-serif'],
        mono: ['JetBrains Mono', 'Cascadia Code', 'Consolas', 'monospace'],
      },
      fontSize: {
        hero: ['1.2rem', { lineHeight: '1.4' }],
        title: ['0.85rem', { lineHeight: '1.4' }],
        stat: ['0.75rem', { lineHeight: '1.3' }],
        body: ['0.65rem', { lineHeight: '1.4' }],
        small: ['0.5rem', { lineHeight: '1.3' }],
        tiny: ['0.4rem', { lineHeight: '1.2' }],
      },
      borderRadius: {
        DEFAULT: '4px',
      },
      boxShadow: {
        card: 'var(--shadow-card)',
      },
    },
  },
  plugins: [],
};
