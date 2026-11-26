// Apps and API access in real Firefox: a static website on another origin
// signs in with mau (the consent screen, allowing and denying), gets its
// tokens from its own JavaScript (PKCE, CORS, no cookies), and calls the
// API; personal tokens; disconnecting; Admin › Apps and blocking; mau's own
// error pages; axe in light and dark at two widths.

import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { BIN, openBrowser, sleep, startServer, steps } from './harness.mjs';

const PW = 'correct horse battery';

/** A static "third-party app": two pages, no server-side code at all. */
function startApp(mau) {
	const page = (body) => `<!doctype html><html lang="en"><head><meta charset="utf-8">
<title>Reading Stats</title></head><body><main>${body}</main></body></html>`;
	const common = `
const mau = ${JSON.stringify(mau)};
const b64 = (bytes) => btoa(String.fromCharCode(...bytes)).replace(/\\+/g, '-').replace(/\\//g, '_').replace(/=+$/, '');
const show = (t) => { const p = document.createElement('p'); p.textContent = t; document.querySelector('main').append(p); };`;
	const index = page(`<h1>Reading Stats</h1>
<button id="signin">Sign in with mau</button>
<script type="module">${common}
document.getElementById('signin').onclick = async () => {
	const verifier = b64(crypto.getRandomValues(new Uint8Array(32)));
	const challenge = b64(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))));
	const state = b64(crypto.getRandomValues(new Uint8Array(16)));
	sessionStorage.setItem('mau', JSON.stringify({ verifier, state }));
	location.assign(mau + '/oauth/authorize?' + new URLSearchParams({
		response_type: 'code', client_id: location.origin + '/',
		redirect_uri: location.origin + '/callback.html',
		scope: 'profile:read books:read notes:read', state,
		code_challenge: challenge, code_challenge_method: 'S256'
	}));
};
</script>`);
	const callback = page(`<h1>Callback</h1>
<script type="module">${common}
const p = new URLSearchParams(location.search);
const saved = JSON.parse(sessionStorage.getItem('mau') || '{}');
if (p.get('error')) show('error: ' + p.get('error'));
else if (p.get('state') !== saved.state || p.get('iss') !== mau) show('state or iss mismatch');
else {
	history.replaceState(null, '', location.pathname);
	const form = (f) => fetch(mau + '/oauth/token', { method: 'POST', body: new URLSearchParams(f) }).then((r) => r.json());
	const t = await form({ grant_type: 'authorization_code', code: p.get('code'),
		redirect_uri: location.origin + '/callback.html', client_id: location.origin + '/', code_verifier: saved.verifier });
	if (!t.access_token) show('token error: ' + JSON.stringify(t));
	else {
		const get = (path, tok) => fetch(mau + '/api/v1' + path, { headers: { Authorization: 'Bearer ' + tok } });
		const me = await (await get('/profile', t.access_token)).json();
		show('Hello ' + me.display_name);
		const books = await (await get('/books?limit=200', t.access_token)).json();
		show(books.books.length + ' books');
		show('notes ' + (books.books.some((b) => 'notes' in b) ? 'visible' : 'hidden'));
		const denied = await get('/books/x/readings', t.access_token);
		show('readings ' + denied.status);
		const r = await form({ grant_type: 'refresh_token', refresh_token: t.refresh_token, client_id: location.origin + '/' });
		show(r.access_token && r.refresh_token !== t.refresh_token ? 'refreshed' : 'refresh failed');
		show('token=' + r.access_token);
	}
}
</script>`);
	const server = createServer((req, res) => {
		const path = new URL(req.url, 'http://x').pathname;
		const body = path === '/' ? index : path === '/callback.html' ? callback : null;
		res.writeHead(body ? 200 : 404, { 'content-type': 'text/html; charset=utf-8' });
		res.end(body ?? 'not found');
	});
	return new Promise((ok) =>
		server.listen(0, '127.0.0.1', () => {
			const base = `http://localhost:${server.address().port}`;
			ok({ base, host: base.slice('http://'.length), stop: () => server.close() });
		})
	);
}

export async function run() {
	console.log('Access tokens and apps (seeded library, an app on another origin):');
	const server = await startServer();
	server.seedReader();
	const app = await startApp(server.base);
	const b = await openBrowser();
	const { page, until, click, type, text } = b;
	const { step, done } = steps('API');
	const go = async (path, waitFor) => {
		await page.goto(server.base + path);
		if (waitFor) await until(waitFor);
	};
	const forgetConfirmation = () => server.sql('UPDATE sessions SET reauth_at = 0');
	const confirm = async () => {
		await until("Confirm it's you");
		await type('#cf-pw', PW);
		await click('.dialog button', 'Confirm');
	};
	const api = (path, token) =>
		fetch(server.base + '/api/v1' + path, { headers: { Authorization: `Bearer ${token}` } });
	let appToken = '';

	try {
		await step(
			'Signed out, an app’s link goes to sign-in and comes back to the consent screen',
			async () => {
				await page.goto(app.base + '/');
				await click('#signin');
				await until('Forgot your password?');
				const next = new URL(page.url()).searchParams.get('next');
				if (!next?.startsWith('/authorize/')) throw new Error(`next was ${next}`);
				// The reader has no password yet: set one with a reset link, which
				// also signs in, then sign in again through the link the app made.
				const link = execFileSync(BIN, ['reset-password', 'rri'], { env: server.env })
					.toString()
					.match(/(http\S+\/reset\/\S+)/)[1];
				await page.goto(link);
				await until('New password for @rri');
				await type('#rs-pw-1', PW);
				await type('#rs-pw-2', PW);
				await click('button', 'Set password');
				await until('Pick up where you left off');
				await go(`/signin?next=${encodeURIComponent(next)}`, 'wants to use your mau account');
			}
		);

		await step(
			'The consent screen names the app by its address, then lists what it wants',
			async () => {
				await until(`${app.host} wants to use your mau account`);
				await until('Signed in as');
				await click('button', 'Continue as R. Reader');
				await until(`Allow ${app.host}?`);
				await until('Private notes');
				await until('This app is running on this computer');
				const scopes = await page.$$eval('.scopes li', (l) => l.map((x) => x.textContent.trim()));
				if (scopes.length !== 3) throw new Error(`scopes: ${scopes.join(' | ')}`);
				if (!scopes.at(-1).includes('Private')) throw new Error('notes not marked Private');
			}
		);

		await step(
			'Allow asks for the password, then the app gets tokens from its own page (CORS, PKCE)',
			async () => {
				forgetConfirmation();
				await click('button', 'Allow');
				await confirm();
				await until('Hello R. Reader', 15_000);
				if (!page.url().startsWith(app.base)) throw new Error(`ended at ${page.url()}`);
				await until('16 books');
				await until('notes visible');
				await until('readings 403');
				await until('refreshed');
				appToken = (await text()).match(/token=(mau_at_\S+)/)[1];
				if ((await api('/profile', appToken)).status !== 200) throw new Error('token refused');
			}
		);

		await step('Deny sends the reader back to the app with access_denied', async () => {
			await page.goto(app.base + '/');
			await click('#signin');
			await until('wants to use your mau account');
			await click('button', 'Continue as R. Reader');
			await until(`Allow ${app.host}?`);
			await click('button', 'Deny');
			await until('error: access_denied');
		});

		await step('Bad links get mau’s own error page, never a redirect', async () => {
			const q = new URLSearchParams({
				response_type: 'code',
				client_id: app.base + '/',
				redirect_uri: 'https://evil.example/cb',
				scope: 'books:read',
				state: 's',
				code_challenge: 'x'.repeat(43),
				code_challenge_method: 'S256'
			});
			await go(`/oauth/authorize?${q}`, "This sign-in link doesn't work");
			if (!page.url().startsWith(server.base)) throw new Error(`left mau: ${page.url()}`);
			await until('Source code licensed under AGPL-3.0');
		});

		await step(
			'Settings › Access Tokens lists the app; generating a token shows it once',
			async () => {
				await go('/settings', 'Access Tokens');
				await click('a', 'Access Tokens');
				await until('Connected apps');
				await until(app.host);
				await until('Private notes');
				forgetConfirmation();
				await click('button', 'Generate a token');
				await type('#tk-name', 'Firefox test script');
				await page.select('#tk-exp', 'never');
				await until('works until you revoke it');
				await click('.check', 'Profile');
				await click('button', 'Generate token');
				await confirm();
				await until('Your new token');
				const secret = (await text()).match(/mau_pat_[A-Za-z0-9_-]{43}/)[0];
				const r = await api('/profile', secret);
				if (r.status !== 200) throw new Error(`token answered ${r.status}`);
				if ((await api('/books', secret)).status !== 403) throw new Error('books allowed');
				await click('.check', "I've saved it");
				await click('button', 'Done');
				await until('Firefox test script');
				await until('Never expires');
				if ((await text()).includes(secret)) throw new Error('token shown again');
				const card = await page.$$eval('.card', (cs) =>
					cs.findIndex((c) => c.textContent.includes('Firefox test script'))
				);
				await page.$$eval(
					'.card',
					(cs, i) =>
						[...cs[i].querySelectorAll('button')]
							.find((x) => x.textContent.includes('Revoke'))
							.click(),
					card
				);
				await until('Revoked “Firefox test script”');
				if ((await api('/profile', secret)).status !== 401) throw new Error('revoked token works');
			}
		);

		await step(
			'Admin › Apps counts people (never who); blocking ends access and refuses the app',
			async () => {
				await go('/settings/admin/apps', app.host);
				await until('Connected by 1 person');
				const cards = await page.$eval('.cards', (c) => c.textContent);
				if (cards.includes('@rri') || cards.includes('R. Reader'))
					throw new Error('names who connected it');
				await click('button', 'Block');
				await until(`Block ${app.host}?`);
				await click('.dialog button.danger');
				await until(`Blocked ${app.host}`);
				if ((await api('/profile', appToken)).status !== 401) throw new Error('blocked app works');
				await page.goto(app.base + '/');
				await click('#signin');
				await until('This app is blocked');
				await go('/settings/admin/apps', 'Unblock');
				await click('button', 'Unblock');
				await until(`Unblocked ${app.host}`);
			}
		);

		await step('Disconnecting an app ends its tokens', async () => {
			await page.goto(app.base + '/');
			await click('#signin');
			await until('wants to use your mau account');
			await click('button', 'Continue as R. Reader');
			await click('button', 'Allow');
			await until('Hello R. Reader', 15_000);
			const tok = (await text()).match(/token=(mau_at_\S+)/)[1];
			await go('/settings/api', app.host);
			await click('button', 'Disconnect');
			await until(`Disconnected ${app.host}`);
			if ((await api('/profile', tok)).status !== 401) throw new Error('disconnected app works');
		});

		await step('No axe violations (WCAG 2.1 AA) in light and dark, phone and desktop', async () => {
			const found = [];
			for (const theme of ['light', 'dark']) {
				await page.goto(server.base + '/api/health');
				await page.evaluate((t) => localStorage.setItem('mau.appearance', t), theme);
				for (const [w, h] of [
					[1440, 900],
					[390, 844]
				]) {
					await page.setViewport({ width: w, height: h });
					await page.goto(app.base + '/');
					await click('#signin');
					await until('wants to use your mau account');
					for (const v of await b.axe()) found.push(`${theme} ${w}px consent: ${v}`);
					await click('button', 'Continue as R. Reader');
					await until('Allow');
					for (const v of await b.axe()) found.push(`${theme} ${w}px consent scopes: ${v}`);
					for (const [path, wait] of [
						['/settings/api', 'Personal Access Tokens'],
						['/settings/admin/apps', app.host],
						['/oauth/authorize?client_id=nope', "doesn't work"]
					]) {
						await go(path, wait);
						await sleep(300);
						for (const v of await b.axe()) found.push(`${theme} ${w}px ${path}: ${v}`);
					}
				}
			}
			await page.evaluate(() => localStorage.removeItem('mau.appearance'));
			await page.setViewport({ width: 420, height: 900 });
			if (found.length) throw new Error(found.join('\n        '));
		});

		await step('No console errors on any of these pages', async () => {
			if (b.errors.length) throw new Error(b.errors.join('\n        '));
		});
	} finally {
		await b.browser.close();
		app.stop();
		server.stop();
	}
	return done();
}
