/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: {
          base: "#06090c",
          panel: "#080d10",
          panelDeep: "#070b0e",
          deep: "#05080b",
          grad1: "#0d161d",
          grad2: "#080f14",
        },
        ac: "#3df0ff", // default accent — overridden at runtime via --ac
        // The live accent at any alpha: `border-hud/[.13]` is
        // color-mix(var(--ac) 13%, transparent), so it follows runtime --ac.
        hud: "color-mix(in srgb, var(--ac) calc(<alpha-value> * 100%), transparent)",
        ink: {
          base: "#bcccd0",
          bright: "#eaf6f8",
          light: "#dceef1",
          dim: "#8aa0a6",
          mid: "#9fb0b5",
          dimmer: "#5a6e72",
          darkest: "#46585e",
          deepest: "#3a4a50",
          label: "#7f9298", // status strip labels
          faint: "#6f8589", // directories, empty states
          void: "#04080b", // text on an accent fill
          agentVoid: "#0c0612", // text on an agent fill
        },
        sig: {
          warn: "#ffb000",
          danger: "#ff5a3c",
          ok: "#7fd49a",
          okDark: "#3a8f4e",
          add: "#7fd49a",
          delete: "#d98a7d",
          violet: "#c08bff",
          agent: "#c08bff", // everything that comes from Claude
          warnInk: "#f3e2c0", // body text inside a flag
          deleteHi: "#ff7a5e", // D status chip
          dangerInk: "#ff9a86", // discard button
          dangerDark: "#b54a3a",
        },
      },
      fontFamily: {
        chrome: ["Saira", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
      keyframes: {
        scan: {
          "0%": { transform: "translateY(0)" },
          "100%": { transform: "translateY(6px)" },
        },
        sweep: {
          "0%": { top: "-34%" },
          "100%": { top: "130%" },
        },
        flick: {
          "0%,95%,100%": { opacity: "1" },
          "96%": { opacity: ".86" },
          "98%": { opacity: ".98" },
          "99%": { opacity: ".9" },
        },
        blink: {
          "0%,49%": { opacity: "1" },
          "50%,100%": { opacity: "0" },
        },
        pulse: {
          "0%,100%": { opacity: ".5" },
          "50%": { opacity: "1" },
        },
      },
      animation: {
        scan: "scan 1.1s steps(3) infinite",
        sweep: "sweep 7s linear infinite",
        flick: "flick 8s infinite",
        blink: "blink 1s steps(1) infinite",
        pulse: "pulse 2s infinite",
      },
    },
  },
  plugins: [],
};
