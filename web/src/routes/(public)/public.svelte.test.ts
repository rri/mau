// Signed-out pages (spec §2, §3a): landing, sign-in, recovery, signup,
// invites, reset links, and first-run setup.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Landing from '#lib/components/Landing.svelte';
import SignIn from './signin/+page.svelte';
import Recovery from './signin/recovery/+page.svelte';
import Signup from './signup/+page.svelte';
import Invite from './invite/[token]/+page.svelte';
import Reset from './reset/[token]/+page.svelte';
import Setup from './setup/[token]/+page.svelte';
import { auth } from '#lib/auth.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { at, nav } from '../../test/app-mocks.ts';
import { ME, mockApi, signOut } from '../../test/fixtures.ts';

const status = (me = ME) => ({ me, signup_mode: 'invite', setup_required: false });
const fail = (code: string, message: string, s = 400) =>
	new Response(JSON.stringify({ error: code, message }), { status: s });
const PW = 'a long enough password';

beforeEach(() => signOut());

describe('Landing', () => {
	it('LAND-01 the logo, name, tagline, a random quote with its source, and Sign in', async () => {
		mockApi({
			'GET /quote': {
				text: 'The stars are ours.',
				author: 'H. G. Wells',
				title: 'The Time Machine',
				year: 1895,
				gutenberg: 35
			}
		});
		render(Landing);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('mau');
		expect(screen.getByText('your personal library and reading log')).toBeInTheDocument();
		expect(await screen.findByText('“The stars are ours.”')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'The Time Machine' })).toHaveAttribute(
			'href',
			'https://www.gutenberg.org/ebooks/35'
		);
		expect(screen.getByText(/H. G. Wells,/).closest('figcaption')).toHaveTextContent(
			'H. G. Wells, The Time Machine, 1895'
		);
		expect(screen.getByRole('link', { name: 'Sign in' })).toHaveAttribute('href', '/signin');
		expect(screen.queryByRole('link', { name: 'Create account' })).toBeNull();
		expect(screen.getByRole('link', { name: 'github.com/rri/mau' })).toHaveAttribute(
			'href',
			'https://github.com/rri/mau'
		);
		expect(screen.getByText('Source code licensed under AGPL-3.0')).toBeInTheDocument();
	});

	it('LAND-02 open signup adds Create account; no quote (offline) leaves the rest', async () => {
		signOut('open');
		mockApi({ 'GET /quote': () => Promise.reject(new TypeError('offline')) });
		render(Landing);
		expect(screen.getByRole('link', { name: 'Create account' })).toHaveAttribute('href', '/signup');
		await new Promise((r) => setTimeout(r));
		expect(screen.queryByRole('blockquote')).toBeNull();
	});
});

describe('Sign in', () => {
	it('AUTH-01 a right password signs in and goes home', async () => {
		const fetch = mockApi({
			'POST /auth/login': { code_ticket: null },
			'GET /auth/status': status()
		});
		render(SignIn);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(
			'your personal library and reading log'
		);
		await userEvent.type(screen.getByLabelText('Username'), 'rri');
		await userEvent.type(screen.getByLabelText('Password'), PW);
		await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
		expect(JSON.parse(String(fetch.mock.calls[0][1]!.body))).toEqual({
			username: 'rri',
			password: PW
		});
		expect(auth.me?.username).toBe('rri');
		expect(nav.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('AUTH-02 a wrong password shows the server’s message', async () => {
		mockApi({
			'POST /auth/login': fail('bad_login', 'That username and password don’t match.', 401)
		});
		render(SignIn);
		await userEvent.type(screen.getByLabelText('Username'), 'rri');
		await userEvent.type(screen.getByLabelText('Password'), 'nope');
		await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
		expect(screen.getByRole('alert')).toHaveTextContent('That username and password don’t match.');
		expect(nav.goto).not.toHaveBeenCalled();
	});

	it('AUTH-03 with two-factor, a code is asked next; Back returns; an expired ticket starts over', async () => {
		mockApi({
			'POST /auth/login': { code_ticket: 't1' },
			'POST /auth/login/code': fail('ticket_expired', 'That took too long. Sign in again.')
		});
		render(SignIn);
		await userEvent.type(screen.getByLabelText('Username'), 'rri');
		await userEvent.type(screen.getByLabelText('Password'), PW);
		await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
		expect(screen.getByLabelText('Code from your authenticator app')).toBeInTheDocument();
		expect(
			screen.getByText('Lost your phone? Enter one of your recovery codes instead.')
		).toBeInTheDocument();
		await userEvent.type(screen.getByLabelText('Code from your authenticator app'), '123456');
		await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
		expect(screen.getByRole('alert')).toHaveTextContent('That took too long. Sign in again.');
		expect(screen.getByLabelText('Password')).toHaveValue('');
	});

	it('AUTH-04 a right code signs in', async () => {
		const fetch = mockApi({
			'POST /auth/login': { code_ticket: 't1' },
			'POST /auth/login/code': { ok: true },
			'GET /auth/status': status({ ...ME, two_factor: true })
		});
		render(SignIn);
		await userEvent.type(screen.getByLabelText('Username'), 'rri');
		await userEvent.type(screen.getByLabelText('Password'), PW);
		await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
		await userEvent.type(screen.getByLabelText('Code from your authenticator app'), '123456');
		await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
		expect(JSON.parse(String(fetch.mock.calls[1][1]!.body))).toEqual({
			ticket: 't1',
			code: '123456'
		});
		expect(nav.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('AUTH-05 the links: recovery always; Create an account only when signup is open', () => {
		const { unmount } = render(SignIn);
		expect(screen.getByRole('link', { name: 'Forgot your password?' })).toHaveAttribute(
			'href',
			'/signin/recovery'
		);
		expect(screen.getByText('New here? Ask for an invite link.')).toBeInTheDocument();
		unmount();
		signOut('open');
		render(SignIn);
		expect(screen.getByRole('link', { name: 'Create an account' })).toHaveAttribute(
			'href',
			'/signup'
		);
	});

	it('AUTH-06 before setup, it explains how to create the admin', () => {
		signOut('open', true);
		render(SignIn);
		expect(screen.getByText(/This instance has no admin yet/)).toBeInTheDocument();
		expect(screen.queryByRole('link', { name: 'Create an account' })).toBeNull();
	});

	it('AUTH-07 someone already signed in goes home', () => {
		auth.me = ME;
		render(SignIn);
		expect(nav.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});
});

describe('Recovery and reset', () => {
	it('AUTH-10 a recovery code sets a new password and says how many codes are left', async () => {
		const fetch = mockApi({
			'POST /auth/recovery': { codes_left: 1 },
			'GET /auth/status': status()
		});
		render(Recovery);
		const submit = screen.getByRole('button', { name: 'Set password' });
		await userEvent.type(screen.getByLabelText('Username'), 'rri');
		await userEvent.type(screen.getByLabelText('Recovery code'), 'abcde-fghij');
		await userEvent.type(screen.getByLabelText('New password'), PW);
		expect(submit).toBeDisabled();
		await userEvent.type(screen.getByLabelText('Type it again'), PW);
		expect(submit).toBeEnabled();
		await userEvent.click(submit);
		expect(JSON.parse(String(fetch.mock.calls[0][1]!.body))).toEqual({
			username: 'rri',
			code: 'abcde-fghij',
			password: PW
		});
		expect(toasts.current?.message).toBe('New password set. 1 recovery code left.');
		expect(nav.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('AUTH-11 a reset link names the account, warns about two-factor, and sets the password', async () => {
		at('/reset/tok', { token: 'tok' });
		mockApi({
			'GET /auth/reset/tok': { valid: true, username: 'rri', two_factor: true },
			'POST /auth/reset/tok': { ok: true },
			'GET /auth/status': status()
		});
		render(Reset);
		expect(screen.getByText('Checking the link…')).toBeInTheDocument();
		expect(
			await screen.findByRole('heading', { name: 'New password for @rri' })
		).toBeInTheDocument();
		expect(screen.getByText(/Two-factor will be turned off/)).toBeInTheDocument();
		await userEvent.type(screen.getByLabelText('New password'), PW);
		await userEvent.type(screen.getByLabelText('Type it again'), PW);
		await userEvent.click(screen.getByRole('button', { name: 'Set password' }));
		expect(nav.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('AUTH-12 a used or expired reset link says so', async () => {
		at('/reset/old', { token: 'old' });
		mockApi({ 'GET /auth/reset/old': { valid: false, username: '', two_factor: false } });
		render(Reset);
		expect(
			await screen.findByRole('heading', { name: "This link doesn't work" })
		).toBeInTheDocument();
	});
});

describe('Signup', () => {
	it('AUTH-20 the signup page explains when signup is invite-only or closed', () => {
		const { unmount } = render(Signup);
		expect(
			screen.getByText('You need an invite link to create an account on this instance.')
		).toBeInTheDocument();
		unmount();
		signOut('closed');
		render(Signup);
		expect(
			screen.getByText("This instance isn't taking new accounts right now.")
		).toBeInTheDocument();
	});

	it('AUTH-21 open signup: name (checked as you type), password, then recovery codes', async () => {
		signOut('open');
		const fetch = mockApi({
			'POST /auth/username-available': (b) =>
				(b as { username: string }).username === 'taken'
					? { available: false, message: 'That username is taken.' }
					: { available: true, message: 'Available.' },
			'POST /auth/register': { codes: ['aaaaa-11111', 'bbbbb-22222'] },
			'GET /auth/status': status()
		});
		render(Signup);
		expect(screen.getByText('Create your mau account.')).toBeInTheDocument();
		const cont = screen.getByRole('button', { name: 'Continue' });
		expect(cont).toBeDisabled();
		await userEvent.type(screen.getByLabelText('Username'), 'taken');
		expect(await screen.findByText('That username is taken.')).toBeInTheDocument();
		expect(cont).toBeDisabled();
		await userEvent.clear(screen.getByLabelText('Username'));
		await userEvent.type(screen.getByLabelText('Username'), 'newbie');
		expect(await screen.findByText('Available.')).toBeInTheDocument();
		await userEvent.click(cont);
		expect(screen.getByRole('heading', { name: 'Choose a password' })).toBeInTheDocument();
		await userEvent.type(screen.getByLabelText('Password'), PW);
		await userEvent.type(screen.getByLabelText('Type it again'), PW);
		await userEvent.click(screen.getByRole('button', { name: 'Create account' }));
		const reg = fetch.mock.calls.find(([u]) => String(u) === '/api/auth/register')!;
		expect(JSON.parse(String(reg[1]!.body))).toMatchObject({
			username: 'newbie',
			display_name: 'newbie',
			password: PW,
			invite: null,
			setup: null
		});
		expect(screen.getByRole('heading', { name: 'Save your recovery codes' })).toBeInTheDocument();
		expect(screen.getByText('aaaaa-11111')).toBeInTheDocument();
		const start = screen.getByRole('button', { name: 'Start using mau' });
		expect(start).toBeDisabled();
		await userEvent.click(screen.getByLabelText("I've saved these codes somewhere safe"));
		await userEvent.click(start);
		expect(nav.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('AUTH-22 an invite link names who invited; a used one says so', async () => {
		at('/invite/good', { token: 'good' });
		mockApi({ 'GET /auth/invite/good': { valid: true, inviter: 'R. Reader' } });
		const { unmount } = render(Invite);
		expect(await screen.findByText('R. Reader invited you to mau.')).toBeInTheDocument();
		unmount();
		at('/invite/used', { token: 'used' });
		mockApi({ 'GET /auth/invite/used': { valid: false, inviter: null } });
		render(Invite);
		expect(
			await screen.findByRole('heading', { name: "This invite link doesn't work" })
		).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'I already have an account' })).toHaveAttribute(
			'href',
			'/signin'
		);
	});

	it('AUTH-23 the setup link creates the admin account; a stale one explains what to do', async () => {
		at('/setup/s1', { token: 's1' });
		mockApi({ 'POST /auth/setup-check': { valid: true } });
		const { unmount } = render(Setup);
		expect(
			await screen.findByRole('heading', { name: 'Set up this instance' })
		).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Create the admin account' }));
		expect(
			screen.getByText('This is the first account on this instance. It will be the admin.')
		).toBeInTheDocument();
		unmount();
		mockApi({ 'POST /auth/setup-check': { valid: false } });
		render(Setup);
		expect(
			await screen.findByRole('heading', { name: "This setup link doesn't work" })
		).toBeInTheDocument();
	});
});

describe('New password fields', () => {
	it('AUTH-30 counts down to the minimum length and flags a mismatch', async () => {
		vi.useRealTimers();
		mockApi({ 'POST /auth/recovery': {} });
		render(Recovery);
		await userEvent.type(screen.getByLabelText('New password'), 'short');
		expect(screen.getByText('5 more characters needed.')).toBeInTheDocument();
		await userEvent.type(screen.getByLabelText('New password'), 'abcd');
		expect(screen.getByText('1 more character needed.')).toBeInTheDocument();
		await userEvent.type(screen.getByLabelText('Type it again'), 'x');
		expect(screen.getByText("The two don't match.")).toBeInTheDocument();
		await userEvent.click(screen.getByLabelText('Show password'));
		expect(screen.getByLabelText('New password')).toHaveAttribute('type', 'text');
	});
});
