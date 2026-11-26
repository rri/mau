// Where to go after signing in: `?next=` when it's a path on this site
// (the consent page sends people to sign in and back), else Home. Anything
// that could leave the site (`//host`, `/\host`, a scheme) goes Home.

export function safeNext(raw: string | null | undefined): string {
	if (!raw || !raw.startsWith('/') || raw.startsWith('//') || raw.startsWith('/\\')) return '/';
	try {
		const base = 'http://mau.invalid';
		const u = new URL(raw, base);
		return u.origin === base ? u.pathname + u.search + u.hash : '/';
	} catch {
		return '/';
	}
}
