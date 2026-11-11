// Settings (spec §8): the index and each personal page, with the password
// confirmation for sensitive actions.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Index from './+page.svelte';
import Account from './account/+page.svelte';
import Password from './password/+page.svelte';
import TwoFactor from './two-factor/+page.svelte';
import Sessions from './sessions/+page.svelte';
import Recovery from './recovery/+page.svelte';
import Sharing from './sharing/+page.svelte';
import GoogleBooks from './google-books/+page.svelte';
import Invites from './invites/+page.svelte';
import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
import { auth } from '#lib/auth.svelte.ts';
import { library } from '#lib/library.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { at, nav } from '../../../test/app-mocks.ts';
import { freezeTime, ME, mockApi, signIn, useLibrary } from '../../../test/fixtures.ts';

const reauth = () =>
	new Response(JSON.stringify({ error: 'reauth_required', message: 'Confirm it’s you.' }), {
		status: 403
	});
const fail = (message: string) =>
	new Response(JSON.stringify({ error: 'x', message }), { status: 400 });
/** The JSON body sent with the last request to `path` that had one. */
const body = (fetch: ReturnType<typeof mockApi>, path: string) =>
	JSON.parse(
		String(
			fetch.mock.calls.findLast(
				([u, init]) => String(u).startsWith('/api' + path) && init?.body
			)![1]!.body
		)
	);

beforeEach(() => {
	freezeTime();
	signIn();
	useLibrary();
});

describe('Settings index', () => {
	it('SET-01 lists the pages with their current values; admins also get the admin pages', async () => {
		mockApi({
			'GET /recovery-codes': { left: 7, events: [] },
			'GET /shares': [{ id: 's' }, { id: 't' }]
		});
		render(Index);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Settings');
		const row = (label: string) => screen.getByText(label).closest('a')!;
		expect(row('R. Reader')).toHaveAttribute('href', '/settings/account');
		expect(row('R. Reader')).toHaveTextContent('@rri');
		expect(row('Two-factor')).toHaveTextContent('Off');
		expect(row('Google Books')).toHaveTextContent('Off');
		expect(await screen.findByText('7 of 10 left')).toBeInTheDocument();
		expect(screen.getByText('2 shared')).toBeInTheDocument();
		expect(screen.getByText('Admin')).toHaveAttribute('id', 'admin');
		for (const l of [
			'Users',
			'Signup & invites',
			'Catalog providers',
			'Review queue',
			'Shared catalog',
			'Audit log',
			'Instance stats'
		])
			expect(screen.getByText(l)).toBeInTheDocument();
		expect(screen.queryByText('Invites')).toBeNull();
	});

	it('SET-02 non-admins don’t see admin pages; Invites appears when they may invite', async () => {
		signIn({ is_admin: false, two_factor: true, google_books: true });
		mockApi({
			'GET /recovery-codes': { left: 0, events: [] },
			'GET /shares': [],
			'GET /invites': { can_create: true, pending: [] }
		});
		render(Index);
		expect(screen.queryByText('Users')).toBeNull();
		expect(await screen.findByText('Invites')).toBeInTheDocument();
		expect(screen.getByText('None left')).toBeInTheDocument();
		expect(screen.getByText('Nothing shared')).toBeInTheDocument();
		expect(screen.getByText('Two-factor').closest('a')).toHaveTextContent('On');
		expect(screen.getByText('Own key')).toBeInTheDocument();
	});

	it('SET-03 signing out with unsynced changes asks first', async () => {
		mockApi({
			'GET /recovery-codes': { left: 10, events: [] },
			'GET /shares': [],
			'POST /auth/logout': { ok: true }
		});
		library.pending = 3;
		render(Index);
		await userEvent.click(screen.getByRole('button', { name: 'Sign out' }));
		const dialog = screen.getByRole('dialog', { name: 'Sign out with unsynced changes?' });
		expect(dialog).toHaveTextContent("3 changes on this device haven't reached the server yet.");
		await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
		expect(nav.goto).not.toHaveBeenCalled();
		library.pending = 0;
		await userEvent.click(screen.getByRole('button', { name: 'Sign out' }));
		await vi.waitFor(() =>
			expect(nav.goto).toHaveBeenCalledWith('/signin', { replaceState: true })
		);
		expect(auth.me).toBeNull();
	});
});

describe('Account', () => {
	beforeEach(() => at('/settings/account'));

	it('SET-10 the trail, then profile fields from the account', () => {
		render(Account);
		const crumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
		expect(within(crumbs).getAllByRole('link', { name: 'Settings' })[0]).toHaveAttribute(
			'href',
			'/settings'
		);
		expect(screen.getByLabelText('Display name')).toHaveValue('R. Reader');
		expect(screen.getByLabelText('Username')).toHaveValue('rri');
		expect(screen.getByLabelText(/Time zone/)).toHaveValue('Europe/Berlin');
	});

	it('SET-11 saving asks for the password first when needed, then saves', async () => {
		let confirmed = false;
		const fetch = mockApi({
			'PATCH /account': (b) => (confirmed ? { ...ME, ...(b as object) } : reauth()),
			'POST /auth/confirm': () => {
				confirmed = true;
				return { ok: true };
			}
		});
		render(Account);
		render(ConfirmDialog);
		await userEvent.clear(screen.getByLabelText('Username'));
		await userEvent.type(screen.getByLabelText('Username'), 'reader');
		await userEvent.click(screen.getByRole('button', { name: 'Save' }));
		const dialog = await screen.findByRole('dialog', { name: "Confirm it's you" });
		await userEvent.type(within(dialog).getByLabelText('Password'), 'pw');
		await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));
		await vi.waitFor(() => expect(auth.me?.username).toBe('reader'));
		expect(body(fetch, '/auth/confirm')).toEqual({ password: 'pw', code: null });
		expect(toasts.current?.message).toBe(
			'Username changed. @rri stays reserved for you for 30 days.'
		);
	});

	it('SET-12 cancelling the confirmation changes nothing and says so', async () => {
		mockApi({ 'PATCH /account': reauth });
		render(Account);
		render(ConfirmDialog);
		await userEvent.click(screen.getByRole('button', { name: 'Save' }));
		await screen.findByRole('dialog', { name: "Confirm it's you" });
		await userEvent.keyboard('{Escape}');
		expect(await screen.findByRole('alert')).toHaveTextContent('Cancelled. Nothing was changed.');
		expect(auth.me?.username).toBe('rri');
	});

	it('SET-13 with two-factor on, the confirmation also asks for a code', async () => {
		signIn({ two_factor: true });
		mockApi({ 'PATCH /account': reauth });
		render(Account);
		render(ConfirmDialog);
		await userEvent.click(screen.getByRole('button', { name: 'Save' }));
		expect(await screen.findByLabelText('Authenticator code')).toBeInTheDocument();
	});

	it('SET-14 appearance is kept on this device', async () => {
		render(Account);
		await userEvent.click(screen.getByLabelText('Dark'));
		expect(localStorage.getItem('mau.appearance')).toBe('dark');
		expect(document.documentElement.dataset.theme).toBe('dark');
		await userEvent.click(screen.getByLabelText('Match the system'));
		expect(localStorage.getItem('mau.appearance')).toBeNull();
	});

	it('SET-15 deleting needs the username typed; then the account and this device’s copy go', async () => {
		const fetch = mockApi({ 'DELETE /account': { ok: true } });
		render(Account);
		await userEvent.click(screen.getByRole('button', { name: 'Delete account…' }));
		const dialog = screen.getByRole('dialog', { name: 'Delete your account?' });
		await userEvent.type(
			within(dialog).getByLabelText('Type your username, rri, to confirm'),
			'nope'
		);
		await userEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
		expect(within(dialog).getByRole('alert')).toHaveTextContent(
			"The username didn't match. Nothing was deleted."
		);
		expect(fetch).not.toHaveBeenCalled();
		const field = within(dialog).getByLabelText(/Type your username/);
		await userEvent.clear(field);
		await userEvent.type(field, ' RRI ');
		await userEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
		await vi.waitFor(() =>
			expect(nav.goto).toHaveBeenCalledWith('/signin', { replaceState: true })
		);
		expect(body(fetch, '/account')).toEqual({ username: 'rri' });
		expect(auth.me).toBeNull();
	});
});

describe('Password and two-factor', () => {
	it('SET-20 a new password needs 10+ characters typed twice; success clears the fields', async () => {
		const fetch = mockApi({ 'PUT /account/password': { ok: true } });
		render(Password);
		const btn = screen.getByRole('button', { name: 'Change password' });
		expect(btn).toBeDisabled();
		await userEvent.type(screen.getByLabelText('New password'), 'a fine new password');
		await userEvent.type(screen.getByLabelText('Type it again'), 'a fine new password');
		await userEvent.click(btn);
		expect(body(fetch, '/account/password')).toEqual({ password: 'a fine new password' });
		expect(toasts.current?.message).toBe('Password changed. Other devices were signed out.');
		expect(await screen.findByLabelText('New password')).toHaveValue('');
	});

	it('SET-21 the server’s password rules show as an error', async () => {
		mockApi({ 'PUT /account/password': fail('That password is too common.') });
		render(Password);
		await userEvent.type(screen.getByLabelText('New password'), 'password123');
		await userEvent.type(screen.getByLabelText('Type it again'), 'password123');
		await userEvent.click(screen.getByRole('button', { name: 'Change password' }));
		expect(await screen.findByRole('alert')).toHaveTextContent('That password is too common.');
	});

	it('SET-22 setting up two-factor: QR code and key, then a code turns it on', async () => {
		mockApi({
			'POST /two-factor/setup': {
				ticket: 't',
				secret: 'ABCDEFGHIJKLMNOP',
				uri: 'otpauth://x',
				qr_svg: '<svg/>'
			},
			'POST /two-factor/enable': { ...ME, two_factor: true }
		});
		render(TwoFactor);
		expect(screen.getByText('Two-factor is off.')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Set up two-factor' }));
		expect(
			await screen.findByRole('img', { name: 'QR code for your authenticator app' })
		).toBeInTheDocument();
		expect(screen.getByText('ABCD EFGH IJKL MNOP')).toBeInTheDocument();
		await userEvent.type(screen.getByLabelText('Code'), '123456');
		await userEvent.click(screen.getByRole('button', { name: 'Turn on' }));
		expect(await screen.findByText('Two-factor is on.')).toBeInTheDocument();
		expect(auth.me?.two_factor).toBe(true);
	});

	it('SET-23 turning two-factor off asks first', async () => {
		signIn({ two_factor: true });
		mockApi({ 'DELETE /two-factor': { ...ME, two_factor: false } });
		render(TwoFactor);
		await userEvent.click(screen.getByRole('button', { name: 'Turn off…' }));
		const dialog = screen.getByRole('dialog', { name: 'Turn off two-factor?' });
		await userEvent.click(within(dialog).getByRole('button', { name: 'Turn off' }));
		expect(await screen.findByText('Two-factor is off.')).toBeInTheDocument();
	});
});

describe('Sessions, recovery codes', () => {
	it('SET-30 sessions: this device first, others can be signed out singly or all at once', async () => {
		const now = Date.now();
		let list = [
			{ id: 'a', device: 'Firefox on Linux', last_seen_at: now, current: true },
			{ id: 'b', device: 'Safari on iPhone', last_seen_at: now - 10 * 60_000, current: false },
			{ id: 'c', device: 'Chrome on Mac', last_seen_at: now - 3 * 86_400_000, current: false }
		];
		mockApi({
			'GET /sessions': () => list,
			'DELETE /sessions/b': () => {
				list = list.filter((s) => s.id !== 'b');
				return { ok: true };
			},
			'POST /sessions/revoke-others': () => {
				list = list.filter((s) => s.current);
				return { ok: true };
			}
		});
		render(Sessions);
		const card = async (d: string) => (await screen.findByText(d)).closest('.card')!;
		expect(await card('Firefox on Linux')).toHaveTextContent('This device');
		expect(await card('Firefox on Linux')).toHaveTextContent('Active now');
		expect(await card('Safari on iPhone')).toHaveTextContent('Active in the last hour');
		expect(await card('Chrome on Mac')).toHaveTextContent('Last active 3 days ago');
		await userEvent.click(
			within(await card('Safari on iPhone')).getByRole('button', { name: 'Sign out' })
		);
		expect(toasts.current?.message).toBe('Signed out Safari on iPhone');
		await userEvent.click(
			await screen.findByRole('button', { name: 'Sign out all other sessions' })
		);
		await vi.waitFor(() => expect(screen.queryByText('Chrome on Mac')).toBeNull());
		expect(screen.queryByRole('button', { name: 'Sign out all other sessions' })).toBeNull();
	});

	it('SET-31 a failed sign-out shows why', async () => {
		mockApi({
			'GET /sessions': [
				{ id: 'a', device: 'This', last_seen_at: Date.now(), current: true },
				{ id: 'b', device: 'Other', last_seen_at: 0, current: false }
			],
			'DELETE /sessions/b': () => Promise.reject(new TypeError('offline'))
		});
		render(Sessions);
		await userEvent.click(await screen.findByRole('button', { name: 'Sign out' }));
		expect(await screen.findByRole('alert')).toHaveTextContent('Needs a connection.');
	});

	it('SET-32 recovery codes: how many are left, a warning when few or none, and recent activity', async () => {
		mockApi({
			'GET /recovery-codes': {
				left: 2,
				events: [{ at: Date.parse('2026-11-01T12:00:00Z'), message: 'A recovery code was used.' }]
			}
		});
		const { unmount } = render(Recovery);
		expect(await screen.findByText('Only 2 left. Generate a new set soon.')).toBeInTheDocument();
		expect(screen.getByText('A recovery code was used.').closest('li')).toHaveTextContent(
			'Nov 1, 2026'
		);
		unmount();
		mockApi({ 'GET /recovery-codes': { left: 0, events: [] } });
		render(Recovery);
		expect(await screen.findByText(/No codes left. Generate a new set now/)).toBeInTheDocument();
		expect(screen.queryByText(/Only 0 left/)).toBeNull();
		expect(screen.getByText('None.')).toBeInTheDocument();
	});

	it('SET-33 new codes are shown once; Done waits until they’re saved', async () => {
		mockApi({
			'GET /recovery-codes': { left: 10, events: [] },
			'POST /recovery-codes/regenerate': { codes: ['11111-aaaaa', '22222-bbbbb'] }
		});
		render(Recovery);
		await userEvent.click(await screen.findByRole('button', { name: 'Generate new codes' }));
		expect(await screen.findByText('11111-aaaaa')).toBeInTheDocument();
		const done = screen.getByRole('button', { name: 'Done' });
		expect(done).toBeDisabled();
		await userEvent.click(screen.getByLabelText("I've saved them"));
		await userEvent.click(done);
		expect(await screen.findByText('of 10 codes left')).toBeInTheDocument();
	});
});

describe('Sharing, Google Books, invites', () => {
	it('SET-40 shares list with their link and rating setting; tag shares start on the first tag', async () => {
		let shares = [{ id: 's1', kind: 'finished', tag: null, include_ratings: true, token: 'tok1' }];
		const fetch = mockApi({
			'GET /shares': () => shares,
			'POST /shares': (b) => {
				shares = [
					...shares,
					{
						id: 's2',
						...(b as { kind: string; tag: string; include_ratings: boolean }),
						token: 'tok2'
					}
				];
				return { ok: true };
			}
		});
		render(Sharing);
		const card = (await screen.findByText('Finished books')).closest('.card')!;
		expect(card).toHaveTextContent('Public');
		expect(card).toHaveTextContent('Includes ratings · /s/tok1');
		await userEvent.click(screen.getByRole('button', { name: 'Share something' }));
		await userEvent.selectOptions(screen.getByLabelText('What to share'), 'tag');
		expect(screen.getByLabelText('Tag')).toHaveDisplayValue('#book club');
		await userEvent.click(screen.getByRole('button', { name: 'Publish' }));
		expect(body(fetch, '/shares')).toEqual({
			kind: 'tag',
			tag: 'book club',
			include_ratings: false
		});
		expect(await screen.findByText('#book club')).toBeInTheDocument();
	});

	it('SET-41 a tag share can’t be chosen without tags', async () => {
		useLibrary((await import('#lib/sync/apply.ts')).emptyData());
		mockApi({ 'GET /shares': [] });
		render(Sharing);
		expect(await screen.findByText("You're not sharing anything.")).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Share something' }));
		expect(screen.getByRole('option', { name: 'A tag… (no tags yet)' })).toBeDisabled();
	});

	it('SET-42 hiding ratings, a new link, and stopping a share each confirm with a toast', async () => {
		mockApi({
			'GET /shares': [{ id: 's1', kind: 'active', tag: null, include_ratings: false, token: 't' }],
			'PATCH /shares/s1': { ok: true },
			'DELETE /shares/s1': fail('Not allowed.')
		});
		render(Sharing);
		await userEvent.click(await screen.findByRole('button', { name: 'Show ratings' }));
		expect(toasts.current?.message).toBe('Ratings shown');
		await userEvent.click(screen.getByRole('button', { name: 'Make a new link' }));
		expect(toasts.current?.message).toBe('New link made. The old link no longer works.');
		await userEvent.click(screen.getByRole('button', { name: 'Stop sharing' }));
		expect(await screen.findByRole('alert')).toHaveTextContent('Not allowed.');
	});

	it('SET-43 Google Books: save a key, see its last characters, remove it', async () => {
		mockApi({
			'GET /account/google-books': { ends_with: null },
			'PUT /account/google-books': { ends_with: 'WXYZ' },
			'DELETE /account/google-books': { ends_with: null }
		});
		render(GoogleBooks);
		const save = await screen.findByRole('button', { name: 'Save key' });
		expect(save).toBeDisabled();
		await userEvent.type(screen.getByLabelText('API key'), 'AIzaWXYZ');
		await userEvent.click(save);
		expect(await screen.findByText('Key ending in …WXYZ')).toBeInTheDocument();
		expect(auth.me?.google_books).toBe(true);
		await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
		expect(await screen.findByLabelText('API key')).toBeInTheDocument();
		expect(auth.me?.google_books).toBe(false);
	});

	it('SET-44 invites: create a link (shown once), list pending ones with expiry, revoke', async () => {
		let pending = [{ id: 'i1', created_by: 'rri', expires_at: Date.now() + 86_400_000 }];
		mockApi({
			'GET /invites': () => ({ can_create: true, pending }),
			'POST /invites': () => {
				pending = [
					...pending,
					{ id: 'i2', created_by: 'rri', expires_at: Date.now() + 7 * 86_400_000 }
				];
				return { url: 'https://mau.example/invite/abc' };
			},
			'DELETE /invites/i1': () => {
				pending = pending.filter((p) => p.id !== 'i1');
				return { ok: true };
			}
		});
		render(Invites);
		expect(await screen.findByText('Expires tomorrow')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Create invite link' }));
		expect(await screen.findByText('https://mau.example/invite/abc')).toBeInTheDocument();
		expect(await screen.findByText('Expires in 7 days')).toBeInTheDocument();
		await userEvent.click(screen.getAllByRole('button', { name: 'Revoke' })[0]);
		expect(toasts.current?.message).toBe('Invite revoked');
	});

	it('SET-45 without permission to invite, it says so; no flash of “No pending invites” while loading', async () => {
		mockApi({ 'GET /invites': { can_create: false, pending: [] } });
		render(Invites);
		expect(screen.queryByText('No pending invites.')).toBeNull();
		expect(await screen.findByText("You can't create more invites right now.")).toBeInTheDocument();
		expect(screen.getByText('No pending invites.')).toBeInTheDocument();
	});
});
