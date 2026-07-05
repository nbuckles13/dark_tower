import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// Svelte 5. `vitePreprocess` handles `<script lang="ts">` + `.svelte.ts` TS.
export default {
  preprocess: vitePreprocess(),
};
