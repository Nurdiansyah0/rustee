/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  darkMode: "class",
  safelist: ["dark"],
  theme: {
    extend: {
      colors: {
        // Surface hierarchy
        surface: {
          canvas: "rgb(var(--surface-canvas) / <alpha-value>)",
          card: "rgb(var(--surface-card) / <alpha-value>)",
          elevated: "rgb(var(--surface-elevated) / <alpha-value>)",
          modal: "rgb(var(--surface-modal) / <alpha-value>)",
          subtle: "rgb(var(--surface-subtle) / <alpha-value>)",
          sunken: "rgb(var(--surface-sunken) / <alpha-value>)",
          overlay: "rgb(var(--surface-overlay) / <alpha-value>)",
          primary: "rgb(var(--surface-card) / <alpha-value>)",
          secondary: "rgb(var(--surface-canvas) / <alpha-value>)",
          tertiary: "rgb(var(--surface-subtle) / <alpha-value>)",
          inverse: "rgb(var(--text-primary) / <alpha-value>)",
        },
        // Content / Typography hierarchy
        content: {
          primary: "rgb(var(--text-primary) / <alpha-value>)",
          secondary: "rgb(var(--text-secondary) / <alpha-value>)",
          muted: "rgb(var(--text-muted) / <alpha-value>)",
          inverse: "rgb(var(--text-inverse) / <alpha-value>)",
          brand: "rgb(var(--text-brand) / <alpha-value>)",
        },
        // Financial Income / Growth (Refined Emerald)
        income: {
          default: "rgb(var(--income-default) / <alpha-value>)",
          muted: "rgb(var(--income-muted) / <alpha-value>)",
          emphasis: "rgb(var(--income-emphasis) / <alpha-value>)",
          border: "rgb(var(--income-border) / <alpha-value>)",
        },
        // Financial Expense / Danger (Refined Rose)
        expense: {
          default: "rgb(var(--expense-default) / <alpha-value>)",
          muted: "rgb(var(--expense-muted) / <alpha-value>)",
          emphasis: "rgb(var(--expense-emphasis) / <alpha-value>)",
          border: "rgb(var(--expense-border) / <alpha-value>)",
        },
        // Financial Warning / Attention (Refined Amber)
        warning: {
          default: "rgb(var(--warning-default) / <alpha-value>)",
          muted: "rgb(var(--warning-muted) / <alpha-value>)",
          emphasis: "rgb(var(--warning-emphasis) / <alpha-value>)",
          border: "rgb(var(--warning-border) / <alpha-value>)",
        },
        // Financial Transfer / Neutral (Refined Sky)
        transfer: {
          default: "rgb(var(--transfer-default) / <alpha-value>)",
          muted: "rgb(var(--transfer-muted) / <alpha-value>)",
          emphasis: "rgb(var(--transfer-emphasis) / <alpha-value>)",
          border: "rgb(var(--transfer-border) / <alpha-value>)",
        },
        // Brand / Primary Action
        brand: {
          default: "rgb(var(--brand-default) / <alpha-value>)",
          emphasis: "rgb(var(--brand-emphasis) / <alpha-value>)",
          muted: "rgb(var(--brand-muted) / <alpha-value>)",
          border: "rgb(var(--brand-border) / <alpha-value>)",
          50: "#ecfdf5",
          100: "#d1fae5",
          200: "#a7f3d0",
          300: "#6ee7b7",
          400: "#34d399",
          500: "#10b981",
          600: "#059669",
          700: "#047857",
          800: "#065f46",
          900: "#064e3b",
          950: "#022c22",
        },
        // Border tokens
        border: {
          subtle: "rgb(var(--border-subtle) / <alpha-value>)",
          default: "rgb(var(--border-default) / <alpha-value>)",
          strong: "rgb(var(--border-strong) / <alpha-value>)",
          interactive: "rgb(var(--border-interactive) / <alpha-value>)",
          focus: "rgb(var(--border-interactive) / <alpha-value>)",
        },
        // Backward compatibility primary
        primary: {
          50: "#ecfdf5",
          100: "#d1fae5",
          500: "#10b981",
          600: "#059669",
          700: "#047857",
        },
      },
      boxShadow: {
        card: "0 1px 2px 0 rgb(0 0 0 / 0.04), 0 1px 1px -1px rgb(0 0 0 / 0.02)",
        subtle: "0 1px 3px 0 rgb(0 0 0 / 0.05), 0 1px 2px -1px rgb(0 0 0 / 0.03)",
        dropdown: "0 4px 6px -1px rgb(0 0 0 / 0.07), 0 2px 4px -2px rgb(0 0 0 / 0.05)",
        floating: "0 10px 15px -3px rgb(0 0 0 / 0.08), 0 4px 6px -4px rgb(0 0 0 / 0.04)",
        modal: "0 20px 25px -5px rgb(0 0 0 / 0.12), 0 8px 10px -6px rgb(0 0 0 / 0.06)",
        "card-hover": "0 4px 6px -1px rgb(0 0 0 / 0.08), 0 2px 4px -2px rgb(0 0 0 / 0.04)",
        sheet: "0 -10px 25px -5px rgb(0 0 0 / 0.1)",
        "glow-income": "0 0 16px -2px rgb(5 150 105 / 0.18)",
        "glow-expense": "0 0 16px -2px rgb(225 29 72 / 0.18)",
        "border-highlight": "inset 0 1px 0 0 rgba(255, 255, 255, 0.06)",
      },
      borderRadius: {
        sm: "6px",
        md: "8px",
        lg: "12px",
        xl: "16px",
        "2xl": "20px",
        "3xl": "24px",
      },
      fontFamily: {
        sans: [
          '"Plus Jakarta Sans"',
          "Inter",
          "system-ui",
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          "Roboto",
          "sans-serif",
        ],
        mono: [
          '"JetBrains Mono"',
          '"Fira Code"',
          "ui-monospace",
          "monospace",
        ],
      },
      spacing: {
        "13": "3.25rem", // 52px for ergonomic keypad keys
        "safe-top": "env(safe-area-inset-top, 0px)",
        "safe-bottom": "env(safe-area-inset-bottom, 0px)",
        "safe-left": "env(safe-area-inset-left, 0px)",
        "safe-right": "env(safe-area-inset-right, 0px)",
      },
    },
  },
  plugins: [],
}
