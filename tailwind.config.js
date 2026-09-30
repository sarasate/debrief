/** `{ base: "rgb(var(--bg-base) / <alpha-value>)", … }` for a token group. */
function tokens(group, names) {
  const kebab = (n) => n.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  return Object.fromEntries(
    names.map((n) => [n, `rgb(var(--${group}-${kebab(n)}) / <alpha-value>)`]),
  );
}

/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Every palette colour is a theme variable (src/styles/themes.css),
        // stored as RGB channels so alpha modifiers like `/45` keep working.
        bg: tokens("bg", ["base", "panel", "panelDeep", "deep", "grad1", "grad2"]),
        ac: "var(--ac)", // the live accent, set per theme and accent
        // The live accent at any alpha: `border-hud/[.13]` is
        // color-mix(var(--ac) 13%, transparent), so it follows runtime --ac.
        hud: "color-mix(in srgb, var(--ac) calc(<alpha-value> * 100%), transparent)",
        ink: tokens("ink", [
          "base", "bright", "light", "dim", "mid", "dimmer", "darkest", "deepest",
          "label", // status strip labels
          "faint", // directories, empty states
          "void", // text on an accent fill
          "agentVoid", // text on an agent fill
        ]),
        sig: tokens("sig", [
          "warn", "danger", "ok", "okDark", "add", "delete", "violet",
          "agent", // everything that comes from Claude
          "warnInk", // body text inside a flag
          "deleteHi", // D status chip
          "dangerInk", // discard button
          "dangerDark",
        ]),
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
