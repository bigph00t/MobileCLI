/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Claude-inspired color palette
        primary: {
          50: '#fdf4f3',
          100: '#fce8e6',
          200: '#f9d4d0',
          300: '#f4b4ad',
          400: '#ec8b7f',
          500: '#e06352',
          600: '#cc4637',
          700: '#ab382b',
          800: '#8d3128',
          900: '#752f28',
          950: '#3f1410',
        },
        surface: {
          DEFAULT: '#ffffff',
          secondary: '#f9fafb',
          tertiary: '#f3f4f6',
          dark: '#1f2937',
          'dark-secondary': '#111827',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Menlo', 'monospace'],
      },
    },
  },
  plugins: [],
}
