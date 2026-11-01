import { expect, test } from '@playwright/test';
import { addAuthenticator, setupToken } from './helpers.ts';

test('first admin signs up with the setup link, then signs in with the passkey', async ({
	page
}) => {
	await addAuthenticator(page);
	await page.goto(`/setup/${setupToken()}`);
	await page.getByRole('button', { name: 'Create the admin account' }).click();

	await page.getByLabel('Username').fill('rri');
	await expect(page.getByText('@rri is available.')).toBeVisible();
	await page.getByLabel('Display name').fill('R. Reader');
	await page.getByRole('button', { name: 'Continue' }).click();

	await page.getByRole('button', { name: 'Create passkey' }).click();
	await expect(page.getByRole('heading', { name: 'Save your recovery codes' })).toBeVisible();
	const start = page.getByRole('button', { name: 'Start using mau' });
	await expect(start).toBeDisabled();
	await page.getByLabel("I've saved these codes somewhere safe").check();
	await start.click();
	await expect(page.getByText('Welcome, R. Reader.')).toBeVisible();

	// The setup link no longer works.
	await page.goto(`/setup/${setupToken()}`);
	await expect(page.getByRole('heading', { name: "This setup link doesn't work" })).toBeVisible();

	// Sign out by dropping the cookie, then sign in with the stored passkey.
	await page.context().clearCookies();
	await page.evaluate(() => localStorage.clear());
	await page.goto('/');
	await expect(page).toHaveURL(/\/signin$/);
	await page.getByRole('button', { name: 'Sign in with a passkey' }).click();
	await expect(page.getByText('Welcome, R. Reader.')).toBeVisible();
});
