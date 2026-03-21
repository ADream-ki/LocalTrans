/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        primary: '#2563EB',
        accent: '#0EA5E9',
        success: '#10B981',
        warning: '#F59E0B',
        error: '#EF4444',
        bg: {
          primary: '#FFFFFF',
          secondary: '#F7F9FC',
          tertiary: '#EEF2F7',
          elevated: '#FFFFFF',
          hover: '#E8EEF8',
        },
        text: {
          primary: '#1E293B',
          secondary: '#64748B',
          tertiary: '#94A3B8',
          inverse: '#FFFFFF',
        }
      },
      spacing: {
        'xs': '4px',
        's': '8px',
        'm': '12px',
        'l': '16px',
        'xl': '24px',
        'xxl': '32px',
      },
      borderRadius: {
        'small': '6px',
        'medium': '8px',
        'large': '12px',
        'xlarge': '16px',
        'full': '9999px',
      },
      fontFamily: {
        sans: ['Segoe UI Variable', 'Microsoft YaHei UI', 'Noto Sans SC', 'sans-serif'],
        mono: ['Cascadia Code', 'JetBrains Mono', 'Consolas', 'monospace'],
      },
      fontSize: {
        'xs': '11px',
        's': '12px',
        'm': '14px',
        'l': '16px',
        'xl': '18px',
        'xxl': '22px',
      },
      animation: {
        'pulse-slow': 'pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        'fade-in': 'fadeIn 0.22s ease-out',
        'slide-up': 'slideUp 0.22s ease-out',
      },
      keyframes: {
        fadeIn: {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        slideUp: {
          '0%': { opacity: '0', transform: 'translateY(10px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
      },
      transitionDuration: {
        'instant': '60ms',
        'fast': '120ms',
        'normal': '220ms',
        'slow': '320ms',
      },
    },
  },
  plugins: [],
}
