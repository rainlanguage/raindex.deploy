const path = require("node:path");

// Tailwind config for the harness. Absolute content globs (relative to this
// file) so classes are collected regardless of the process CWD: the real
// component source, the harness scenes, and flowbite-svelte's own component
// markup (whose utility classes live in node_modules).
const uiRoot = path.join(__dirname, "..");
const repoRoot = path.join(__dirname, "..", "..", "..");

/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    path.join(uiRoot, "src/**/*.{html,js,svelte,ts}"),
    path.join(__dirname, "**/*.{html,js,svelte,ts}"),
    path.join(
      repoRoot,
      "node_modules/flowbite-svelte/**/*.{html,js,svelte,ts}",
    ),
    path.join(
      repoRoot,
      "node_modules/flowbite-svelte-icons/**/*.{html,js,svelte,ts}",
    ),
  ],
  theme: {
    extend: {},
  },
  plugins: [],
};
