/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        primary: {
          DEFAULT: '#5048E5',
          hover: 'rgba(80, 72, 229, 0.08)',
        },
        accent: '#F5A623',
        bg: '#F0F0EC',
        surface: '#FFFFFF',
        sidebar: '#F8F6FE',
        border: '#E0DCF0',
        text: {
          primary: '#1A1A32',
          secondary: '#6A62A0',
          tertiary: '#9A92C8',
        },
        heatmap: {
          bg: '#F0ECF8',
          low: 'rgba(80, 72, 229, 0.2)',
          mid: 'rgba(80, 72, 229, 0.5)',
          high: '#5048E5',
        },
        success: '#4CAF50',
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
        card: '0 2px 8px rgba(0,0,0,0.05)',
      },
    },
  },
  plugins: [],
};
