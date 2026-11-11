// Shared setup for page and component tests (jsdom).
// The device clock reads UTC in tests (greetings use the device's hour).
process.env.TZ = 'UTC';

import '@testing-library/jest-dom/vitest';
// The library keeps its offline copy in IndexedDB, which jsdom lacks.
import 'fake-indexeddb/auto';
import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/svelte';
import { nav, page } from './app-mocks.ts';

// Node 26 has its own (experimental, unconfigured) localStorage, which hides
// jsdom's; the app reads the bare globals, so point them at jsdom's.
const dom = (globalThis as unknown as { jsdom: { window: Window } }).jsdom.window;
for (const name of ['localStorage', 'sessionStorage'] as const)
	Object.defineProperty(globalThis, name, { value: dom[name], configurable: true });

// jsdom doesn't lay out pages.
Element.prototype.scrollIntoView = () => {};
// Browsers let a file input's files be set (to clear it); jsdom has no setter.
const files = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'files')!;
Object.defineProperty(HTMLInputElement.prototype, 'files', { ...files, set() {} });

vi.mock('$app/navigation', () => nav);
vi.mock('$app/state', () => ({ page }));

// Each test starts signed out with an empty library and no toast.
const { auth } = await import('#lib/auth.svelte.ts');
const { library } = await import('#lib/library.svelte.ts');
const { toasts } = await import('#lib/toast.svelte.ts');
const { confirmation } = await import('#lib/confirm.svelte.ts');
const { emptyData } = await import('#lib/sync/apply.ts');

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
	vi.useRealTimers();
	vi.unstubAllGlobals();
	localStorage.clear();
	sessionStorage.clear();
	auth.me = null;
	auth.status = null;
	auth.ready = false;
	library.close();
	library.view = emptyData();
	library.online = true;
	toasts.dismiss();
	if (confirmation.open) confirmation.cancel();
	page.status = 200;
	page.error = null;
});
