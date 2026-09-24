// File: packages/web-app/src/main.ts
//
// Demo entry point. Loads config and mounts the app.
//
// TELEMETRY IS CONFIGURED IN `App.svelte`, NOT HERE. It needs a getter for the
// current user token (GC's telemetry proxy is behind `require_user_auth`), and
// the token's single home is `App.svelte`'s retained `AuthSession`. Configuring
// it here would mean either reaching into a component's state from the entry
// point or standing up a second holder for a bearer credential — so the call
// moved to where the session already lives, which removes the ordering problem
// instead of working around it.

import { mount } from 'svelte';
import App from './App.svelte';
import { loadConfig } from './lib/config.js';

const config = loadConfig();

const target = document.getElementById('app');
if (target === null) {
  throw new Error('#app mount target not found');
}

const app = mount(App, { target, props: { config } });

export default app;
