// File: packages/web-app/src/main.ts
//
// Demo entry point. Loads config, optionally configures SDK telemetry (OFF by
// default — only when an endpoint is set, @observability), and mounts the app.

import { mount } from 'svelte';
import App from './App.svelte';
import { loadConfig } from './lib/config.js';
import { configureTelemetryIfEnabled } from './lib/session.js';

const config = loadConfig();
configureTelemetryIfEnabled(config);

const target = document.getElementById('app');
if (target === null) {
  throw new Error('#app mount target not found');
}

const app = mount(App, { target, props: { config } });

export default app;
