// ESLint v9 flat config for the Dark Tower client workspace.
//
// Targets: packages/**/*.ts (and packages/**/*.svelte when task #15 lands).
// Proto-gen has no TypeScript sources — only buf-generated output under
// packages/sdk-core/src/proto/ which is excluded below.
//
// svelte-plugin: wired as a placeholder comment. Task #15 (sdk-svelte /
// web-app) uncomments the svelte-related lines — no workflow change needed.

import js from '@eslint/js';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  // Base JS recommended rules.
  js.configs.recommended,

  // TypeScript-aware rules for all .ts files under packages/.
  ...tseslint.configs.recommended,

  {
    // Apply TS rules only to client package source.
    files: ['packages/**/*.ts'],
    languageOptions: {
      parserOptions: {
        // Use the per-package tsconfig.json for type-aware linting.
        // project: true is the v8 idiom; each package must have tsconfig.json
        // extending tsconfig.base.json (already the case for sdk-core and
        // test-utils per task #9).
        project: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Enforce consistent use of `type` imports (avoids value import of
      // type-only symbols, which causes runtime errors in strict ESM).
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
      ],

      // Prefer explicit return types on exported functions for public API
      // clarity; internal helpers can rely on inference.
      '@typescript-eslint/explicit-module-boundary-types': 'off',

      // The codebase uses `any` sparingly and intentionally in transport
      // adapter boundaries (IWebTransport event callbacks). Warn rather
      // than error to surface new unintentional uses without blocking CI.
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },

  // --- svelte placeholder (task #15) ---
  // When sdk-svelte / web-app land, uncomment the block below and add:
  //   pnpm add -w -D eslint-plugin-svelte globals
  //
  // import sveltePlugin from 'eslint-plugin-svelte';
  // import globals from 'globals';
  // ...sveltePlugin.configs['flat/recommended'],
  // {
  //   files: ['packages/**/*.svelte'],
  //   languageOptions: {
  //     globals: { ...globals.browser },
  //     parserOptions: { parser: tseslint.parser },
  //   },
  // },

  {
    // Global ignores: generated code, build artifacts, test outputs.
    ignores: [
      '**/node_modules/**',
      '**/dist/**',
      '**/coverage/**',
      '**/.nx/**',
      // Proto-generated TypeScript (buf codegen output — not hand-authored).
      'packages/sdk-core/src/proto/**',
      // Vitest / Vite config files that reference vitest globals — not
      // in scope for linting (they're tooling config, not SDK source).
      '**/vitest.config.ts',
      '**/vitest.*.config.ts',
      '**/vite.config.ts',
    ],
  },
);
