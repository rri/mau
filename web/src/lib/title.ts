// The browser tab title for each page: "<page> · mau". One table for the
// whole app, so a page can't be left showing the previous page's title.

const PAGES: Record<string, string> = {
	'/reading': 'Reading',
	'/catalog': 'Catalog',
	'/add': 'Add a book',
	'/settings': 'Settings',
	'/settings/account': 'Account',
	'/settings/password': 'Password',
	'/settings/two-factor': 'Two-factor',
	'/settings/sessions': 'Sessions',
	'/settings/recovery': 'Recovery codes',
	'/settings/sharing': 'Sharing',
	'/settings/google-books': 'Google Books',
	'/settings/import': 'Import & export',
	'/settings/invites': 'Invites',
	'/settings/admin/users': 'Users',
	'/settings/admin/signup': 'Signup & invites',
	'/settings/admin/providers': 'Catalog providers',
	'/settings/admin/queue': 'Review queue',
	'/settings/admin/catalog': 'Shared catalog',
	'/settings/admin/audit': 'Audit log',
	'/settings/admin/stats': 'Instance stats',
	'/signin': 'Sign in',
	'/signin/recovery': 'Set a new password',
	'/signup': 'Create an account'
};

const PREFIXES: [string, string][] = [
	['/invite/', 'Invitation'],
	['/reset/', 'New password'],
	['/setup/', 'Set up mau']
];

export function pageTitle(
	path: string,
	{ signedIn, status = 200, book }: { signedIn: boolean; status?: number; book?: string | null }
): string {
	if (status === 404) return 'Page not found · mau';
	if (status >= 400) return 'Something went wrong · mau';
	if (path === '/') return signedIn ? 'Home · mau' : 'mau · your personal library and reading log';
	const p = path.length > 1 ? path.replace(/\/$/, '') : path;
	if (p.startsWith('/catalog/')) return `${book || 'Book'} · mau`;
	const name = PAGES[p] ?? PREFIXES.find(([pre]) => p.startsWith(pre))?.[1];
	return name ? `${name} · mau` : 'mau';
}
