// Stand-ins for SvelteKit's $app modules, which need a running router.
import { vi } from 'vitest';

export const nav = {
	goto: vi.fn(async () => {}),
	afterNavigate: vi.fn(),
	beforeNavigate: vi.fn(),
	invalidateAll: vi.fn(async () => {})
};

/** The current page; tests set `page.url` (and `page.params`) before rendering. */
export const page: {
	url: URL;
	params: Record<string, string>;
	status: number;
	error: { message: string } | null;
} = {
	url: new URL('http://localhost/'),
	params: {},
	status: 200,
	error: null
};

/** A navigation under way (`$app/state`'s `navigating`); null when none. */
export const navigating: { to: { url: URL } | null } = { to: null };

export function at(path: string, params: Record<string, string> = {}) {
	page.url = new URL('http://localhost' + path);
	navigating.to = null;
	page.params = params;
}
