// Shared setup for page and component tests (jsdom).
import '@testing-library/jest-dom/vitest';
import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/svelte';
import { nav, page } from './app-mocks.ts';

// Node 26 has its own (experimental, unconfigured) localStorage, which hides
// jsdom's; the app reads the bare globals, so point them at jsdom's.
const dom = (globalThis as unknown as { jsdom: { window: Window } }).jsdom.window;
for (const name of ['localStorage', 'sessionStorage'] as const)
	Object.defineProperty(globalThis, name, { value: dom[name], configurable: true });

vi.mock('$app/navigation', () => nav);
vi.mock('$app/state', () => ({ page }));

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
	localStorage.clear();
	sessionStorage.clear();
});
