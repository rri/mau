import { defineConfig } from '@playwright/test';

// End-to-end tests run against a real `mau` binary on port 8371 with a fresh
// data directory (see e2e/global-setup.ts). Build first: `just e2e`.
export default defineConfig({
	testDir: 'e2e',
	testMatch: '**/*.e2e.ts',
	globalSetup: './e2e/global-setup.ts',
	globalTeardown: './e2e/global-teardown.ts',
	fullyParallel: false,
	workers: 1,
	use: {
		baseURL: 'http://localhost:8371',
		browserName: 'chromium'
	}
});
