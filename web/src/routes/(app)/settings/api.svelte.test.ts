// Settings › Apps & API access (spec §8): connected apps, personal tokens,
// and making a token (confirmed, shown once).
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Api from './api/+page.svelte';
import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
import type { ApiAccess } from '#lib/api/generated/ApiAccess.ts';
import { library } from '#lib/library.svelte.ts';
import { freezeTime, mockApi, signIn, useLibrary } from '../../../test/fixtures.ts';

const DAY = 86_400_000;
const NOW = Date.parse('2026-11-11T08:00:00Z');
const scopes = [
	{ scope: 'profile:read', label: 'See your display name, username, bio and time zone' },
	{ scope: 'books:read', label: 'See your books, with their tags and when you added them' },
	{ scope: 'notes:write', label: 'Change your private notes' }
];
const access = (over: Partial<ApiAccess> = {}): ApiAccess => ({
	apps: [
		{
			id: 'g1',
			client_id: 'https://stats.example.com/',
			origin: 'https://stats.example.com',
			host: 'stats.example.com',
			name: 'Reading Stats',
			scopes: [scopes[1]],
			created_at: NOW - 30 * DAY,
			last_used_at: NOW - 2 * DAY
		}
	],
	tokens: [
		{
			id: 't1',
			name: 'Kindle script',
			hint: 'mau_pat_3fA9',
			scopes: ['books:read'],
			created_at: NOW - 10 * DAY,
			expires_at: null,
			last_used_at: null
		}
	],
	scopes,
	...over
});
const reauth = () =>
	new Response(JSON.stringify({ error: 'reauth_required', message: 'Confirm it’s you.' }), {
		status: 403
	});

beforeEach(() => {
	freezeTime();
	signIn();
	useLibrary();
});

describe('Apps & API access', () => {
	it('SET-50 lists connected apps by address and personal tokens by name, with Disconnect and Revoke', async () => {
		const fetch = mockApi({
			'GET /api-access': access(),
			'DELETE /api-access/apps/g1': { ok: true },
			'DELETE /api-access/tokens/t1': { ok: true }
		});
		render(Api);
		const app = (await screen.findByText('stats.example.com')).closest('.card') as HTMLElement;
		expect(app).toHaveTextContent('Reading Stats');
		expect(app).toHaveTextContent('See your books, with their tags and when you added them');
		expect(app).toHaveTextContent('Connected Oct 12, 2026 · Last used 2 days ago');
		const tok = screen.getByText('Kindle script').closest('.card') as HTMLElement;
		expect(tok).toHaveTextContent('mau_pat_3fA9…');
		expect(tok).toHaveTextContent('Never expires · Never used');
		await userEvent.click(within(app).getByRole('button', { name: 'Disconnect' }));
		await userEvent.click(within(tok).getByRole('button', { name: 'Revoke' }));
		const methods = fetch.mock.calls.map(([u, i]) => `${i?.method ?? 'GET'} ${u}`);
		expect(methods).toContain('DELETE /api/api-access/apps/g1');
		expect(methods).toContain('DELETE /api/api-access/tokens/t1');
	});

	it('SET-51 empty lists say what will appear there', async () => {
		mockApi({ 'GET /api-access': access({ apps: [], tokens: [] }) });
		render(Api);
		expect(await screen.findByText(/No apps connected/)).toBeInTheDocument();
		expect(screen.getByText('No tokens yet.')).toBeInTheDocument();
	});

	it('SET-52 making a token asks for a name, scopes and a confirmation, then shows it once', async () => {
		let sent: unknown = null;
		let confirmed = false;
		mockApi({
			'GET /api-access': access({ tokens: [] }),
			'POST /auth/confirm': () => ((confirmed = true), { ok: true }),
			'POST /api-access/tokens': (b: unknown) => {
				if (!confirmed) return reauth();
				sent = b;
				return {
					token: { ...access().tokens[0], name: 'Backup' },
					secret: 'mau_pat_3fA9secretsecret'
				};
			}
		});
		render(Api);
		render(ConfirmDialog);
		await userEvent.click(await screen.findByRole('button', { name: 'Make a token' }));
		await userEvent.click(screen.getByRole('button', { name: 'Make token' }));
		expect(screen.getByRole('alert')).toHaveTextContent('Give the token a name');
		await userEvent.type(screen.getByLabelText('Name'), 'Backup');
		await userEvent.click(screen.getByRole('button', { name: 'Make token' }));
		expect(screen.getByRole('alert')).toHaveTextContent('Choose at least one');
		await userEvent.click(screen.getByLabelText(scopes[1].label));
		await userEvent.selectOptions(screen.getByLabelText('Expires'), 'never');
		expect(screen.getByText(/works until you revoke it/)).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Make token' }));
		await userEvent.type(await screen.findByLabelText('Password'), 'pw');
		await userEvent.click(screen.getByRole('button', { name: 'Confirm' }));
		expect(await screen.findByText('mau_pat_3fA9secretsecret')).toBeInTheDocument();
		expect(sent).toEqual({ name: 'Backup', scopes: ['books:read'], expires_in_days: null });
		const done = screen.getByRole('button', { name: 'Done' });
		expect(done).toBeDisabled();
		await userEvent.click(screen.getByLabelText('I’ve saved it'.replace('’', "'")));
		expect(done).toBeEnabled();
		await userEvent.click(done);
		expect(screen.queryByText('mau_pat_3fA9secretsecret')).toBeNull();
	});

	it('SET-53 offline, it says a connection is needed', () => {
		mockApi({ 'GET /api-access': access() });
		library.online = false;
		render(Api);
		expect(screen.getByText('Apps and tokens need a connection.')).toBeInTheDocument();
	});
});
