// Admin pages (spec §8, Admin): guards, users, signup, providers, review
// queue, shared catalog, audit log, and stats.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { createRawSnippet } from 'svelte';
import Layout from './+layout.svelte';
import Users from './users/+page.svelte';
import Signup from './signup/+page.svelte';
import Providers from './providers/+page.svelte';
import Queue from './queue/+page.svelte';
import Catalog from './catalog/+page.svelte';
import Audit from './audit/+page.svelte';
import Stats from './stats/+page.svelte';
import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
import { library } from '#lib/library.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { at } from '../../../../test/app-mocks.ts';
import { freezeTime, ME, mockApi, signIn, useLibrary, work } from '../../../../test/fixtures.ts';

const content = createRawSnippet(() => ({ render: () => '<p>admin content</p>' }));
const sent = (fetch: ReturnType<typeof mockApi>, method: string, path: string) =>
	fetch.mock.calls.filter(
		([u, i]) => String(u).startsWith('/api' + path) && (i?.method ?? 'GET') === method
	);
const user = (over: object) => ({
	id: 'u',
	username: 'u',
	display_name: 'U',
	is_admin: false,
	suspended: false,
	books: 0,
	created_at: 0,
	...over
});

beforeEach(() => {
	freezeTime();
	signIn();
	useLibrary();
});

describe('Admin guards', () => {
	it('ADM-01 non-admins are told the pages are for admins; offline, that they need a connection', () => {
		signIn({ is_admin: false });
		const { unmount } = render(Layout, { children: content });
		expect(screen.getByText('Only admins can see this.')).toBeInTheDocument();
		expect(screen.queryByText('admin content')).toBeNull();
		unmount();
		signIn();
		library.online = false;
		render(Layout, { children: content });
		expect(screen.getByText('Admin pages need a connection.')).toBeInTheDocument();
	});

	it('ADM-02 the trail is Settings › Admin › page', () => {
		at('/settings/admin/stats');
		mockApi({
			'GET /admin/stats': {
				users: 1,
				books: 2,
				shared_works: 3,
				private_works: 0,
				cover_bytes: 10,
				database_bytes: 2_500_000
			}
		});
		render(Stats);
		const crumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
		expect(
			within(crumbs)
				.getAllByRole('link')
				.slice(0, 2)
				.map((a) => a.getAttribute('href'))
		).toEqual(['/settings', '/settings#admin']);
	});
});

describe('Users', () => {
	const users = [
		user({ id: ME.id, username: 'rri', display_name: 'R. Reader', is_admin: true, books: 5 }),
		user({ id: 'u2', username: 'bo', display_name: 'Bo', books: 1 }),
		user({ id: 'u3', username: 'cy', display_name: 'Cy', suspended: true })
	];

	it('ADM-10 lists people with their role, state, and books; the last admin can’t be demoted', async () => {
		mockApi({ 'GET /admin/users': users });
		render(Users);
		const card = (n: string) => screen.getByText(n).closest('.card')!;
		await screen.findByText('Bo');
		expect(card('R. Reader')).toHaveTextContent('Admin');
		expect(card('R. Reader')).toHaveTextContent('5 books');
		expect(card('Bo')).toHaveTextContent('1 book');
		expect(card('Cy')).toHaveTextContent('Suspended');
		const mine = within(card('R. Reader'));
		expect(mine.getByRole('button', { name: 'Remove admin' })).toBeDisabled();
		expect(mine.getByRole('button', { name: 'Suspend' })).toBeDisabled();
		expect(mine.getByRole('button', { name: 'Delete' })).toBeDisabled();
		expect(within(card('Cy')).getByRole('button', { name: 'Unsuspend' })).toBeEnabled();
	});

	it('ADM-11 deleting asks first, then confirms the password, then deletes', async () => {
		let confirmed = false;
		const fetch = mockApi({
			'GET /admin/users': users,
			'DELETE /admin/users/u2': () =>
				confirmed
					? { ok: true }
					: new Response('{"error":"reauth_required","message":"Confirm."}', { status: 403 }),
			'POST /auth/confirm': () => ((confirmed = true), { ok: true })
		});
		render(Users);
		render(ConfirmDialog);
		const bo = (await screen.findByText('Bo')).closest('.card')!;
		await userEvent.click(within(bo as HTMLElement).getByRole('button', { name: 'Delete' }));
		const dialog = screen.getByRole('dialog', { name: 'Delete @bo?' });
		expect(dialog).toHaveTextContent('Deletes their account and their 1 book with its history.');
		await userEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
		const confirm = await screen.findByRole('dialog', { name: "Confirm it's you" });
		await userEvent.type(within(confirm).getByLabelText('Password'), 'pw');
		await userEvent.click(within(confirm).getByRole('button', { name: 'Confirm' }));
		await vi.waitFor(() => expect(toasts.current?.message).toBe('Deleted @bo'));
		expect(sent(fetch, 'DELETE', '/admin/users/u2')).toHaveLength(2);
	});

	it('ADM-12 a reset link is shown once, to copy', async () => {
		mockApi({
			'GET /admin/users': users,
			'POST /admin/users/u2/reset-link': { url: 'https://mau.example/reset/xyz' }
		});
		render(Users);
		const bo = (await screen.findByText('Bo')).closest('.card') as HTMLElement;
		await userEvent.click(within(bo).getByRole('button', { name: 'Reset link' }));
		const dialog = screen.getByRole('dialog', { name: 'Password reset link for @bo' });
		expect(dialog).toHaveTextContent(
			'It lets them set a new password and turns off their two-factor.'
		);
		await userEvent.click(within(dialog).getByRole('button', { name: 'Create link' }));
		expect(await screen.findByText('https://mau.example/reset/xyz')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Done' }));
		expect(screen.queryByText('https://mau.example/reset/xyz')).toBeNull();
	});

	it('ADM-13 making someone admin and suspending send the change', async () => {
		const fetch = mockApi({ 'GET /admin/users': users, 'PATCH /admin/users/u2': { ok: true } });
		render(Users);
		const bo = (await screen.findByText('Bo')).closest('.card') as HTMLElement;
		await userEvent.click(within(bo).getByRole('button', { name: 'Make admin' }));
		await userEvent.click(within(bo).getByRole('button', { name: 'Suspend' }));
		expect(
			sent(fetch, 'PATCH', '/admin/users/u2').map(([, i]) => JSON.parse(String(i!.body)))
		).toEqual([{ is_admin: true }, { suspended: true }]);
	});
});

describe('Signup, providers', () => {
	it('ADM-20 each choice saves as soon as it’s made', async () => {
		const fetch = mockApi({
			'GET /admin/signup': { mode: 'invite', invite_who: 'admins', invite_limit: 3 },
			'PUT /admin/signup': (b) => b,
			'GET /invites': { can_create: true, pending: [] }
		});
		render(Signup);
		expect(await screen.findByLabelText(/Invite only/)).toBeChecked();
		await userEvent.click(screen.getByLabelText(/^Open/));
		await userEvent.click(screen.getByLabelText(/Every user/));
		const puts = sent(fetch, 'PUT', '/admin/signup').map(([, i]) => JSON.parse(String(i!.body)));
		expect(puts).toEqual([
			{ mode: 'open', invite_who: 'admins', invite_limit: 3 },
			{ mode: 'open', invite_who: 'all', invite_limit: 3 }
		]);
		expect(toasts.current?.message).toBe('Saved');
	});

	it('ADM-21 providers: reorder with up/down, keys never shown, a keyless provider can’t be on', async () => {
		const fetch = mockApi({
			'GET /admin/providers': [
				{
					name: 'openlibrary',
					label: 'Open Library',
					enabled: true,
					needs_key: false,
					has_key: false
				},
				{ name: 'isbndb', label: 'ISBNdb', enabled: false, needs_key: true, has_key: false }
			],
			'PUT /admin/providers': (b) =>
				(b as { name: string; enabled: boolean }[]).map((p) => ({
					...p,
					label: p.name === 'isbndb' ? 'ISBNdb' : 'Open Library',
					needs_key: p.name === 'isbndb',
					has_key: p.name === 'isbndb'
				}))
		});
		render(Providers);
		expect(await screen.findByRole('button', { name: 'Move Open Library up' })).toBeDisabled();
		const toggles = screen.getAllByRole('checkbox');
		expect(toggles[1]).toBeDisabled();
		await userEvent.type(screen.getByLabelText('API key for ISBNdb'), 'k-123');
		expect(toggles[1]).toBeEnabled();
		await userEvent.click(toggles[1]);
		await userEvent.click(screen.getByRole('button', { name: 'Move ISBNdb up' }));
		await userEvent.click(screen.getByRole('button', { name: 'Save' }));
		const put = JSON.parse(String(sent(fetch, 'PUT', '/admin/providers')[0][1]!.body));
		expect(put).toEqual([
			{ name: 'isbndb', enabled: true, key: 'k-123' },
			{ name: 'openlibrary', enabled: true, key: null }
		]);
		expect(await screen.findByPlaceholderText('Key saved (type to replace)')).toHaveValue('');
	});
});

describe('Review queue, shared catalog', () => {
	const queued = {
		work: {
			...work({ id: 'z', title: 'Zine', author: 'Me', year: 2024 }),
			private: true,
			suggested: true
		},
		owner: 'bo',
		similar: [{ id: 'w-x', title: 'Zine (2nd)', first_year: 2025 }]
	};

	it('ADM-30 suggestions show the book and owner; add, decline, or merge into a likely match', async () => {
		let items = [queued];
		const fetch = mockApi({
			'GET /admin/queue': () => items,
			'POST /admin/queue/w-z/merge': () => ((items = []), { ok: true })
		});
		render(Queue);
		const card = (await screen.findByText('Zine')).closest('.card')!;
		expect(card).toHaveTextContent('from @bo');
		expect(card).toHaveTextContent('Me · 2024');
		expect(card).toHaveTextContent('Possibly the same as Zine (2nd) (2025).');
		await userEvent.click(
			within(card as HTMLElement).getByRole('button', { name: 'Merge into it' })
		);
		expect(JSON.parse(String(sent(fetch, 'POST', '/admin/queue/w-z/merge')[0][1]!.body))).toEqual({
			into: 'w-x'
		});
		expect(toasts.current?.message).toBe("Merged into Zine (2nd). @bo's book now links to it.");
		expect(await screen.findByText('No suggestions waiting.')).toBeInTheDocument();
	});

	it('ADM-31 an empty queue says so only after loading', async () => {
		mockApi({ 'GET /admin/queue': [] });
		render(Queue);
		expect(screen.queryByText('No suggestions waiting.')).toBeNull();
		expect(await screen.findByText('No suggestions waiting.')).toBeInTheDocument();
	});

	it('ADM-32 shared catalog: find a book, edit it for everyone; no match says so', async () => {
		const w = work({ id: 'd', title: 'Dune', author: 'Frank Herbert', year: 1965 });
		mockApi({
			'GET /admin/catalog': (_b: unknown) => [w],
			'PUT /admin/catalog/works/w-d': (b) => ({ ...w, ...(b as object) })
		});
		render(Catalog);
		await userEvent.type(screen.getByRole('searchbox', { name: 'Find a book' }), 'dune{Enter}');
		const card = (await screen.findByText('Dune')).closest('.card') as HTMLElement;
		expect(card).toHaveTextContent('Frank Herbert · 1965 · 1 edition');
		await userEvent.click(within(card).getAllByRole('button', { name: 'Edit' })[0]);
		const title = within(card).getByLabelText('Title');
		await userEvent.clear(title);
		await userEvent.type(title, 'Dune (50th)');
		await userEvent.click(within(card).getByRole('button', { name: 'Save' }));
		expect(await screen.findByText('Dune (50th)')).toBeInTheDocument();
		expect(toasts.current?.message).toBe('Saved for everyone. Recorded in the audit log.');
		mockApi({ 'GET /admin/catalog': [] });
		await userEvent.click(screen.getByRole('button', { name: 'Find' }));
		expect(await screen.findByText('No books in the catalog match “dune”.')).toBeInTheDocument();
	});
});

describe('Audit log, stats', () => {
	it('ADM-40 entries newest first with time and actor; Older loads the next page', async () => {
		const page = (n: number, from: number) =>
			Array.from({ length: n }, (_, i) => ({
				at: from - i * 60_000,
				actor_name: 'rri',
				action: `did ${i}`
			}));
		const fetch = mockApi({
			'GET /admin/audit': () =>
				fetch.mock.calls.length === 1 ? page(200, Date.parse('2026-11-10T12:00:00Z')) : page(3, 0)
		});
		render(Audit);
		expect(await screen.findByText('did 0')).toBeInTheDocument();
		const log = () => document.querySelectorAll('ul.log > li');
		expect(log()).toHaveLength(200);
		await userEvent.click(screen.getByRole('button', { name: 'Older' }));
		await vi.waitFor(() => expect(log()).toHaveLength(203));
		expect(String(fetch.mock.calls[1][0])).toMatch(/before=\d+/);
		expect(screen.queryByRole('button', { name: 'Older' })).toBeNull();
	});

	it('ADM-41 an empty log says so once loaded', async () => {
		mockApi({ 'GET /admin/audit': [] });
		render(Audit);
		expect(await screen.findByText('Nothing recorded yet.')).toBeInTheDocument();
		expect(document.querySelector('ul.log')).toBeNull();
	});

	it('ADM-42 stats show counts and sizes', async () => {
		mockApi({
			'GET /admin/stats': {
				users: 1200,
				books: 2,
				shared_works: 3,
				private_works: 0,
				cover_bytes: 10,
				database_bytes: 2_500_000
			}
		});
		render(Stats);
		const stat = async (label: string) => (await screen.findByText(label)).parentElement;
		expect(await stat('Users')).toHaveTextContent('1,200');
		expect(await stat('Cover images')).toHaveTextContent('1 KB');
		expect(await stat('Database')).toHaveTextContent('2.4 MB');
	});
});
