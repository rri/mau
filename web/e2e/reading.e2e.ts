import { expect, test } from '@playwright/test';
import { AUTH_FILE, seed } from './helpers.ts';

// Runs after auth.e2e.ts, which creates the admin account "rri".
test.use({ storageState: AUTH_FILE });

test.beforeAll(() => seed('rri'));

const PLUS = 'Add 1 page to The Left Hand of Darkness';

test('+ logs one page and Undo takes it back', async ({ page }) => {
	await page.goto('/');
	const row = page.getByRole('listitem').filter({ hasText: 'The Left Hand of Darkness' });
	await expect(row.getByText('page 140 of 304')).toBeVisible();
	await page.getByRole('button', { name: PLUS }).click();
	await expect(row.getByText('page 141 of 304')).toBeVisible();
	await expect(page.getByRole('status')).toContainText('+1 page on The Left Hand of Darkness');
	await page.getByRole('button', { name: 'Undo' }).click();
	await expect(row.getByText('page 140 of 304')).toBeVisible();
});

test('taps made offline sync when the connection returns', async ({ page, context }) => {
	await page.goto('/');
	const row = page.getByRole('listitem').filter({ hasText: 'The Left Hand of Darkness' });
	await expect(row.getByText('page 140 of 304')).toBeVisible();
	await context.setOffline(true);
	await page.getByRole('button', { name: PLUS }).click();
	await page.getByRole('button', { name: PLUS }).click();
	await expect(row.getByText('page 142 of 304')).toBeVisible();
	await expect(page.getByText(/Offline · 2 unsynced/)).toBeVisible();
	await context.setOffline(false);
	await expect(page.getByText(/unsynced|Syncing/)).toHaveCount(0, { timeout: 10_000 });
	// A fresh page load reads the synced position from the server.
	await page.evaluate(() =>
		indexedDB
			.databases()
			.then((dbs) => dbs.forEach((d) => d.name && indexedDB.deleteDatabase(d.name)))
	);
	await page.reload();
	await expect(row.getByText('page 142 of 304')).toBeVisible();
});

test('pausing moves a book to the top of the inactive books', async ({ page }) => {
	await page.goto('/');
	await page.getByRole('button', { name: 'Pause Middlemarch' }).click();
	const rows = page.getByRole('listitem');
	// Middlemarch is now the first book with a Resume button.
	const firstInactive = rows
		.filter({ has: page.getByRole('button', { name: /^(Resume|Start) / }) })
		.first();
	await expect(firstInactive).toContainText('Middlemarch');
	await page.getByRole('button', { name: 'Undo' }).click();
	await expect(page.getByRole('button', { name: 'Pause Middlemarch' })).toBeVisible();
});

test('catalog filters and the detail page', async ({ page }) => {
	await page.goto('/catalog');
	await expect(page.getByText('16 books')).toBeVisible();
	await page.getByRole('button', { name: 'Active', exact: true }).click();
	await page.getByRole('button', { name: 'Inactive', exact: true }).click();
	await expect(page.getByText('5 of 16 books')).toBeVisible();
	await page.getByRole('link', { name: /Piranesi/ }).click();
	await expect(page.getByRole('heading', { name: 'Piranesi' })).toBeVisible();
	await expect(page.getByText('Reading activity')).toBeVisible();
	await page.getByRole('radio', { name: '5 stars' }).click();
	await expect(page.getByRole('radio', { name: '5 stars' })).toHaveAttribute(
		'aria-checked',
		'true'
	);
});
