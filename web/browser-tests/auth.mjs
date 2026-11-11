// Accounts in real Firefox against a fresh instance: the setup link and first
// admin, sign-in, two-factor, password confirmation, recovery codes, a reset
// link, and an invite for a second person.

import { execFileSync } from 'node:child_process';
import { BIN, openBrowser, sleep, startServer, steps, totp } from './harness.mjs';

const PW = 'correct horse battery';
const PW2 = 'a different passphrase';
const PW3 = 'third time lucky here';
const PW4 = 'fourth and final one';

export async function run() {
	console.log('Accounts (fresh instance):');
	const server = await startServer();
	const b = await openBrowser();
	const { page, until, click, type } = b;
	const { step, done } = steps('Accounts');
	const signOut = async () => {
		await page.goto(server.base + '/settings');
		await click('button', 'Sign out');
		await until('Forgot your password?');
	};
	const signIn = async (user, pw) => {
		await type('#si-user', user);
		await type('#si-pw', pw);
		await click('button', 'Sign in');
	};
	// Sensitive actions then ask for the password again.
	const forgetConfirmation = () => server.sql('UPDATE sessions SET reauth_at = 0');
	let codes = [];
	let secret = '';
	let usedCode = '';

	try {
		await step('Before setup, sign-in explains how to create the admin', async () => {
			await page.goto(server.base + '/signin');
			await until('This instance has no admin yet');
		});

		await step(
			'The setup link creates the first admin: name, password, recovery codes',
			async () => {
				const link = server.log().match(/Create the admin account: (\S+)/)?.[1];
				if (!link) throw new Error('no setup link in the log');
				await page.goto(link);
				await click('button', 'Create the admin account');
				await type('#su-user', 'rri');
				await until('@rri is available.');
				await type('#su-display', 'R. Reader');
				await click('button', 'Continue');
				await until('Choose a password');
				await type('#su-pw-1', 'short');
				await until('5 more characters needed.');
				await type('#su-pw-1', 'password1234');
				await type('#su-pw-2', 'password1234');
				await click('button', 'Create account');
				await until('most common passwords');
				await type('#su-pw-1', PW);
				await type('#su-pw-2', PW);
				await click('button', 'Create account');
				await until('Save your recovery codes');
				codes = await page.$$eval('.codes span', (s) => s.map((e) => e.textContent));
				if (codes.length !== 10) throw new Error(`${codes.length} codes`);
				await click('#su-saved');
				await click('button', 'Start using mau');
				await until('Welcome, R. Reader!');
			}
		);

		await step('A used setup link no longer works', async () => {
			const link = server.log().match(/Create the admin account: (\S+)/)[1];
			await page.goto(link);
			await until("This setup link doesn't work");
		});

		await step('Sign out, a wrong password is refused, the right one signs in', async () => {
			await signOut();
			await signIn('rri', 'wrong password!');
			await until("That username and password don't match.");
			await signIn('rri', PW);
			await until('Welcome, R. Reader!');
		});

		await step(
			'Two-factor: confirm the password, scan, a wrong code is refused, a right one turns it on',
			async () => {
				forgetConfirmation();
				await page.goto(server.base + '/settings/two-factor');
				await until('Two-factor is off.');
				await click('button', 'Set up two-factor');
				await until("Confirm it's you");
				await type('#cf-pw', 'wrong password!');
				await click('button', 'Confirm');
				await until("That password isn't right.");
				await type('#cf-pw', PW);
				await click('button', 'Confirm');
				await until('Scan this with your authenticator app');
				secret = await page.$eval('.key', (e) => e.textContent);
				await type('#tf-code', '000000');
				await click('button', 'Turn on');
				await until("That code didn't match.");
				usedCode = totp(secret);
				await type('#tf-code', usedCode);
				await click('button', 'Turn on');
				await until('Two-factor is on.');
			}
		);

		await step('Sign-in asks for a code; a reused code is refused', async () => {
			await signOut();
			await signIn('rri', PW);
			await until('Code from your authenticator app');
			await type('#si-code', usedCode);
			await click('button', 'Sign in');
			await until("That code didn't work.");
			// The next code (a code is good for its 30 seconds and the next).
			await type('#si-code', totp(secret, 1));
			await click('button', 'Sign in');
			await until('Welcome, R. Reader!');
		});

		await step('Changing the password confirms with password and a recovery code', async () => {
			forgetConfirmation();
			await page.goto(server.base + '/settings/password');
			await type('#pw-1', PW2);
			await type('#pw-2', PW2);
			await click('button', 'Change password');
			await until('Authenticator code');
			await type('#cf-pw', PW);
			await type('#cf-code', codes[0]);
			await click('button', 'Confirm');
			await until('Password changed');
		});

		await step('A recovery code works in place of an authenticator code', async () => {
			await signOut();
			await signIn('rri', PW2);
			await until('Code from your authenticator app');
			await type('#si-code', codes[1]);
			await click('button', 'Sign in');
			await until('Welcome, R. Reader!');
		});

		await step(
			'Forgot password: a used code is refused, a fresh one sets a new password',
			async () => {
				await signOut();
				await click('a', 'Forgot your password?');
				await type('#rc-user', 'rri');
				await type('#rc-code', codes[1]);
				await type('#rc-pw-1', PW3);
				await type('#rc-pw-2', PW3);
				await click('button', 'Set password');
				await until("That code and username don't match");
				await type('#rc-code', codes[2]);
				await click('button', 'Set password');
				await until('Welcome, R. Reader!');
				await until('7 recovery codes left');
			}
		);

		await step(
			'A reset link (from the command line) sets a password and turns two-factor off',
			async () => {
				const out = execFileSync(BIN, ['reset-password', 'rri'], { env: server.env }).toString();
				const link = out.match(/(http\S+\/reset\/\S+)/)[1];
				await signOut();
				await page.goto(link);
				await until('New password for @rri');
				await until('Two-factor will be turned off');
				await type('#rs-pw-1', PW4);
				await type('#rs-pw-2', PW4);
				await click('button', 'Set password');
				await until('Welcome, R. Reader!');
				await page.goto(link);
				await until("This link doesn't work");
				await page.goto(server.base + '/settings/two-factor');
				await until('Two-factor is off.');
			}
		);

		await step('An invite link creates a second account, once', async () => {
			await page.goto(server.base + '/settings/admin/signup');
			await click('button', 'Create invite link');
			await until('/invite/');
			const link = await page.$eval('.link-text', (e) => e.textContent.trim());
			await signOut();
			await page.goto(link);
			await until('R. Reader invited you to mau.');
			await type('#su-user', 'juno');
			await until('@juno is available.');
			await click('button', 'Continue');
			await type('#su-pw-1', PW);
			await type('#su-pw-2', PW);
			await click('button', 'Create account');
			await until('Save your recovery codes');
			await click('#su-saved');
			await click('button', 'Start using mau');
			await until('Welcome, juno!');
			// Not an admin: no admin pages.
			await page.goto(server.base + '/settings');
			await sleep(500);
			if (await page.$('#admin')) throw new Error('admin section shown');
			await signOut();
			await page.goto(link);
			await until("This invite link doesn't work");
		});

		await step('No console errors', async () => {
			// Wrong passwords and codes answer 4xx, which Firefox logs; nothing else.
			const other = b.errors.filter((e) => !/status of 4\d\d|Failed to load resource/.test(e));
			if (other.length) throw new Error(other.join('\n'));
		});
	} finally {
		await b.browser.close();
		server.stop();
	}
	return done();
}
