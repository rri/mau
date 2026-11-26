// Admin › Apps: connected apps by address, with people counts, and blocking.
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Apps from './apps/+page.svelte';
import { freezeTime, mockApi, signIn, useLibrary } from '../../../../test/fixtures.ts';

const NOW = Date.parse('2026-11-11T08:00:00Z');
const row = (over: object) => ({
	client_id: 'https://stats.example.com/',
	origin: 'https://stats.example.com',
	host: 'stats.example.com',
	name: 'Reading Stats',
	people: 3,
	first_seen: NOW - 5 * 86_400_000,
	last_used_at: NOW,
	blocked: false,
	...over
});

beforeEach(() => {
	freezeTime();
	signIn();
	useLibrary();
});

describe('Admin › Apps', () => {
	it('ADM-50 lists apps by address with how many people connected them, not who', async () => {
		mockApi({
			'GET /admin/apps': [
				row({}),
				row({
					client_id: 'https://x.example/',
					origin: 'https://x.example',
					host: 'x.example',
					name: null,
					people: 0,
					blocked: true,
					last_used_at: null
				})
			]
		});
		render(Apps);
		const a = (await screen.findByText('stats.example.com')).closest('.card') as HTMLElement;
		expect(a).toHaveTextContent('Calls itself “Reading Stats”');
		expect(a).toHaveTextContent(
			'Connected by 3 people · First connected Nov 6, 2026 · last used today'
		);
		const b = screen.getByText('x.example').closest('.card') as HTMLElement;
		expect(within(b).getByText('Blocked', { selector: '.pill' })).toBeInTheDocument();
		expect(within(b).getByRole('button', { name: 'Unblock' })).toBeInTheDocument();
	});

	it('ADM-51 blocking asks first, saying who is disconnected', async () => {
		let sent: unknown = null;
		mockApi({
			'GET /admin/apps': [row({})],
			'POST /admin/apps/block': (b: unknown) => ((sent = b), { ok: true })
		});
		render(Apps);
		await userEvent.click(await screen.findByRole('button', { name: 'Block' }));
		const dialog = screen.getByRole('dialog');
		expect(dialog).toHaveTextContent('Block stats.example.com?');
		expect(dialog).toHaveTextContent('3 people will be disconnected');
		await userEvent.click(within(dialog).getByRole('button', { name: 'Block' }));
		expect(sent).toEqual({ client_id: 'https://stats.example.com/', blocked: true });
	});

	it('ADM-52 no apps yet', async () => {
		mockApi({ 'GET /admin/apps': [] });
		render(Apps);
		expect(await screen.findByText('No apps connected yet.')).toBeInTheDocument();
	});
});
