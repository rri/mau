import { describe, expect, it } from 'vitest';
import { pageTitle } from './title.ts';

describe('page titles', () => {
	it('names the main sections', () => {
		expect(pageTitle('/', { signedIn: true })).toBe('Home · mau');
		expect(pageTitle('/', { signedIn: false })).toBe('mau · your personal library and reading log');
		expect(pageTitle('/reading', { signedIn: true })).toBe('Reading · mau');
		expect(pageTitle('/catalog/', { signedIn: true })).toBe('Catalog · mau');
		expect(pageTitle('/add', { signedIn: true })).toBe('Add a book · mau');
	});
	it('uses the book title on a book page', () => {
		expect(pageTitle('/catalog/b1', { signedIn: true, book: 'Piranesi' })).toBe('Piranesi · mau');
		expect(pageTitle('/catalog/b1', { signedIn: true, book: null })).toBe('Book · mau');
	});
	it('names settings and admin pages as their headings do', () => {
		expect(pageTitle('/settings/recovery', { signedIn: true })).toBe('Recovery codes · mau');
		expect(pageTitle('/settings/admin/audit', { signedIn: true })).toBe('Audit log · mau');
	});
	it('names the public pages', () => {
		expect(pageTitle('/signin', { signedIn: false })).toBe('Sign in · mau');
		expect(pageTitle('/invite/abc', { signedIn: false })).toBe('Invitation · mau');
		expect(pageTitle('/reset/abc', { signedIn: false })).toBe('New password · mau');
	});
	it('says when a page is missing or failed', () => {
		expect(pageTitle('/nope', { signedIn: true, status: 404 })).toBe('Page not found · mau');
		expect(pageTitle('/x', { signedIn: true, status: 500 })).toBe('Something went wrong · mau');
		expect(pageTitle('/unlisted', { signedIn: true })).toBe('mau');
	});
});
