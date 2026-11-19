// PostCSS config for the harness. Tailwind only (the harness does not need
// autoprefixer, which is not a hoisted dependency). Kept local so vite picks
// this up instead of ../postcss.config.js when root=harness.
const path = require("node:path");

module.exports = {
  plugins: {
    tailwindcss: { config: path.join(__dirname, "tailwind.config.cjs") },
  },
};
