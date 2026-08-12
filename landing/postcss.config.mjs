/* Tailwind v4 ships its Next.js integration as a PostCSS plugin. The Vite
   build this replaced used `@tailwindcss/vite` instead; the theme itself —
   the `@theme` block in globals.css — is unchanged by the swap. */
export default {
  plugins: {
    '@tailwindcss/postcss': {},
  },
}
