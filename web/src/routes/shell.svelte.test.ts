// The app shell (spec §1, §10): who sees what, the tabs, tab titles, and the
// page for unknown addresses.
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import AppLayout from './(app)/+layout.svelte';
import RootLayout from './+layout.svelte';
import ErrorPage from './+error.svelte';
import { auth } from '#lib/auth.svelte.ts';
import { origin } from '#lib/origin.svelte.ts';
import { at, nav, page } from '../test/app-mocks.ts';
import { mockApi, signIn, signOut, useLibrary } from '../test/fixtures.ts';

const content = createRawSnippet(() => ({ render: () => '<p>page content</p>' }));

beforeEach(() => {
	mockApi({ 'GET /quote': { text: 'Q', author: 'A', title: 'T', year: 1900, gutenberg: 1 } });
	origin.visit('/catalog');
});

describe('App shell', () => {
	it('SHELL-01 signed out, the front page is the landing page', () => {
		signOut();
		at('/');
		render(AppLayout, { children: content });
		expect(screen.getByRole('heading', { level: 1, name: 'mau' })).toBeInTheDocument();
		expect(screen.queryByText('page content')).toBeNull();
		expect(nav.goto).not.toHaveBeenCalled();
	});

	it('SHELL-02 signed out, any other app page goes to sign-in', () => {
		signOut();
		at('/reading');
		render(AppLayout, { children: content });
		expect(nav.goto).toHaveBeenCalledWith('/signin', { replaceState: true });
		expect(screen.queryByText('page content')).toBeNull();
	});

	it('SHELL-03 signed in: the logo links home, then Home (phones), Reading, Catalog, Settings', () => {
		signIn();
		useLibrary();
		at('/reading');
		render(AppLayout, { children: content });
		const navEl = screen.getByRole('navigation', { name: 'Main' });
		const links = within(navEl).getAllByRole('link');
		expect(
			links.map((a) => [
				a.getAttribute('aria-label') ?? a.textContent?.trim(),
				a.getAttribute('href')
			])
		).toEqual([
			['mau home', '/'],
			['Home', '/'],
			['Reading', '/reading'],
			['Catalog', '/catalog'],
			['Settings', '/settings']
		]);
		expect(links[1]).toHaveClass('phone-only');
		expect(within(navEl).getByRole('link', { name: 'Reading' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		expect(screen.getByText('page content')).toBeInTheDocument();
	});

	it('SHELL-04 a book or Add a book keeps the tab it was opened from highlighted', () => {
		signIn();
		useLibrary();
		origin.visit('/reading');
		at('/catalog/lhod', { id: 'lhod' });
		render(AppLayout, { children: content });
		const navEl = screen.getByRole('navigation', { name: 'Main' });
		expect(within(navEl).getByRole('link', { name: 'Reading' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		expect(within(navEl).getByRole('link', { name: 'Catalog' })).not.toHaveAttribute(
			'aria-current'
		);
	});

	it('SHELL-05 Settings stays highlighted on every settings page; Home on the front page', () => {
		signIn();
		useLibrary();
		at('/settings/admin/audit');
		const { unmount } = render(AppLayout, { children: content });
		expect(screen.getByRole('link', { name: 'Settings' })).toHaveAttribute('aria-current', 'page');
		unmount();
		at('/');
		render(AppLayout, { children: content });
		expect(screen.getByRole('link', { name: 'mau home' })).toHaveAttribute('aria-current', 'page');
		expect(screen.getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page');
	});

	it('SHELL-06 visiting a main section makes it the origin for later books', () => {
		signIn();
		useLibrary();
		at('/');
		render(AppLayout, { children: content });
		expect(origin.section).toEqual({ label: 'Home', href: '/' });
		expect(sessionStorage.getItem('mau.origin')).toBe('/');
	});
});

describe('Tab titles and the error page', () => {
	it('SHELL-10 the tab title names the page (a book by its title)', async () => {
		signIn();
		mockApi({ 'GET /auth/status': { me: auth.me, signup_mode: 'invite', setup_required: false } });
		useLibrary();
		at('/catalog/mm', { id: 'mm' });
		render(RootLayout, { children: content });
		await new Promise((r) => setTimeout(r));
		expect(document.title).toBe('Middlemarch · mau');
	});

	it('SHELL-11 an unknown address shows a card with the way home', () => {
		signIn();
		page.status = 404;
		render(ErrorPage);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Page not found');
		expect(screen.getByRole('link', { name: 'Go to Home' })).toHaveAttribute('href', '/');
	});

	it('SHELL-12 signed out, the way back is the front page; other errors show their message', () => {
		signOut();
		page.status = 500;
		page.error = { message: 'Boom.' };
		render(ErrorPage);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Something went wrong');
		expect(screen.getByText('Boom.')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Go to the front page' })).toHaveAttribute('href', '/');
	});
});
