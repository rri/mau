// "Sign in with mau" (spec §2a): sign in first, choose the account, then
// allow exactly what the app asked for, or deny.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Authorize from './authorize/[id]/+page.svelte';
import { auth } from '#lib/auth.svelte.ts';
import { at, nav } from '../../test/app-mocks.ts';
import { ME, mockApi, signIn, signOut } from '../../test/fixtures.ts';

const info = (over: object = {}) => ({
	client_id: 'https://stats.example.com/',
	origin: 'https://stats.example.com',
	host: 'stats.example.com',
	name: 'Reading Stats',
	local: false,
	redirect_to: 'stats.example.com',
	scopes: [
		{ scope: 'books:read', label: 'Books and tags', group: 'read' },
		{ scope: 'notes:read', label: 'Private notes', group: 'read' }
	],
	me: ME,
	...over
});
const reauth = () =>
	new Response(JSON.stringify({ error: 'reauth_required', message: 'Confirm it’s you.' }), {
		status: 403
	});

let assign: ReturnType<typeof vi.fn>;
beforeEach(() => {
	signIn();
	at('/authorize/r1', { id: 'r1' });
	assign = vi.fn();
	vi.stubGlobal('location', { ...window.location, assign });
});

describe('Connect an app', () => {
	it('AUTH-40 signed out, it sends the reader to sign in and back', () => {
		signOut();
		mockApi({});
		render(Authorize);
		expect(nav.goto).toHaveBeenCalledWith('/signin?next=%2Fauthorize%2Fr1', { replaceState: true });
	});

	it('AUTH-41 asks which account, then lists what the app wants, by its address', async () => {
		mockApi({ 'GET /oauth/requests/r1': info() });
		render(Authorize);
		expect(await screen.findByText(/wants to use your mau account/)).toHaveTextContent(
			'stats.example.com wants to use your mau account.'
		);
		expect(screen.getByText('@rri')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Continue as R. Reader' }));
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Allow stats.example.com?');
		expect(screen.getByText(/calls itself “Reading Stats”/)).toBeInTheDocument();
		expect(screen.getByText('READ')).toBeInTheDocument();
		expect(screen.getByText('Private notes').closest('li')).toHaveClass('private');
		expect(screen.getByText(/mau never shares them/)).toBeInTheDocument();
		expect(screen.getByText(/Once you allow access, you'll be redirected to/)).toHaveTextContent(
			'stats.example.com'
		);
		expect(screen.getByText(/Settings › Access Tokens/)).toBeInTheDocument();
	});

	it('AUTH-42 Allow confirms the password, then goes back to the app', async () => {
		let confirmed = false;
		mockApi({
			'GET /oauth/requests/r1': info(),
			'POST /auth/confirm': () => ((confirmed = true), { ok: true }),
			'POST /oauth/requests/r1/approve': () =>
				confirmed ? { redirect: 'https://stats.example.com/cb?code=c&state=s' } : reauth()
		});
		render(Authorize);
		await userEvent.click(await screen.findByRole('button', { name: 'Continue as R. Reader' }));
		await userEvent.click(screen.getByRole('button', { name: 'Allow' }));
		await userEvent.type(await screen.findByLabelText('Password'), 'pw');
		await userEvent.click(screen.getByRole('button', { name: 'Confirm' }));
		await vi.waitFor(() =>
			expect(assign).toHaveBeenCalledWith('https://stats.example.com/cb?code=c&state=s')
		);
	});

	it('AUTH-43 Deny goes back to the app with no access', async () => {
		mockApi({
			'GET /oauth/requests/r1': info(),
			'POST /oauth/requests/r1/deny': {
				redirect: 'https://stats.example.com/cb?error=access_denied'
			}
		});
		render(Authorize);
		await userEvent.click(await screen.findByRole('button', { name: 'Continue as R. Reader' }));
		await userEvent.click(screen.getByRole('button', { name: 'Deny' }));
		await vi.waitFor(() =>
			expect(assign).toHaveBeenCalledWith('https://stats.example.com/cb?error=access_denied')
		);
	});

	it('AUTH-44 Use another account signs out and comes back after signing in', async () => {
		mockApi({ 'GET /oauth/requests/r1': info(), 'POST /auth/logout': { ok: true } });
		render(Authorize);
		await userEvent.click(await screen.findByRole('button', { name: 'Use another account' }));
		await vi.waitFor(() =>
			expect(nav.goto).toHaveBeenCalledWith('/signin?next=%2Fauthorize%2Fr1', {
				replaceState: true
			})
		);
		expect(auth.me).toBeNull();
	});

	it('AUTH-45 a request that expired while the screen was open says so on Allow', async () => {
		mockApi({
			'GET /oauth/requests/r1': info(),
			'POST /oauth/requests/r1/approve': new Response(
				JSON.stringify({ error: 'not_found', message: 'expired' }),
				{ status: 404 }
			)
		});
		render(Authorize);
		await userEvent.click(await screen.findByRole('button', { name: 'Continue as R. Reader' }));
		await userEvent.click(screen.getByRole('button', { name: 'Allow' }));
		expect(await screen.findByText('This request has expired')).toBeInTheDocument();
		expect(assign).not.toHaveBeenCalled();
	});

	it('AUTH-45 an expired request says to start again; an app on this computer is called out', async () => {
		mockApi({
			'GET /oauth/requests/r1': new Response(
				JSON.stringify({ error: 'not_found', message: 'expired' }),
				{ status: 404 }
			)
		});
		const { unmount } = render(Authorize);
		expect(await screen.findByText('This request has expired')).toBeInTheDocument();
		unmount();
		mockApi({
			'GET /oauth/requests/r1': info({ local: true, host: 'localhost:5173', name: null })
		});
		render(Authorize);
		await userEvent.click(await screen.findByRole('button', { name: 'Continue as R. Reader' }));
		expect(screen.getByText(/running on this computer/)).toHaveTextContent('localhost:5173');
	});
});
