import { expect, test } from '@playwright/test';
import { AUTH_FILE, setupToken } from './helpers.ts';

const PASSWORD = 'correct horse battery';

test('first admin signs up with the setup link, then signs in with the password', async ({
	page
}) => {
	await page.goto(`/setup/${setupToken()}`);
	await page.getByRole('button', { name: 'Create the admin account' }).click();

	await page.getByLabel('Username').fill('rri');
	await expect(page.getByText('@rri is available.')).toBeVisible();
	await page.getByLabel('Display name').fill('R. Reader');
	await page.getByRole('button', { name: 'Continue' }).click();

	const create = page.getByRole('button', { name: 'Create account' });
	await page.getByLabel('Password', { exact: true }).fill(PASSWORD);
	await page.getByLabel('Type it again').fill('something else');
	await expect(page.getByText("The two don't match.")).toBeVisible();
	await expect(create).toBeDisabled();
	await page.getByLabel('Type it again').fill(PASSWORD);
	await create.click();

	await expect(page.getByRole('heading', { name: 'Save your recovery codes' })).toBeVisible();
	const start = page.getByRole('button', { name: 'Start using mau' });
	await expect(start).toBeDisabled();
	await page.getByLabel("I've saved these codes somewhere safe").check();
	await start.click();
	await expect(page.getByText('Welcome, R. Reader.')).toBeVisible();

	// The setup link no longer works.
	await page.goto(`/setup/${setupToken()}`);
	await expect(page.getByRole('heading', { name: "This setup link doesn't work" })).toBeVisible();

	// Sign out by dropping the cookie, then sign in again.
	await page.context().clearCookies();
	await page.evaluate(() => localStorage.clear());
	await page.goto('/');
	await expect(page).toHaveURL(/\/signin$/);
	await page.getByLabel('Username').fill('rri');
	await page.getByLabel('Password').fill('not the password');
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByText("That username and password don't match.")).toBeVisible();
	await page.getByLabel('Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByText('Welcome, R. Reader.')).toBeVisible();

	// Later test files reuse this session.
	await page.context().storageState({ path: AUTH_FILE });
});
