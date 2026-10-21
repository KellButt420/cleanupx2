import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { seo } from './scripts/seo.ts';

export default defineConfig({
  plugins: [tailwindcss(), seo()],
  server: { port: 5180, fs: { allow: ['..'] } },
  preview: { port: 5180 },
  build: {
    rolldownOptions: {
      input: { main: 'index.html', buildNotes: 'build-notes.html' },
    },
  },
});
