// The signed-in app in real Firefox against a real server: reading, the book
// page, breadcrumbs, adding, sharing, every page loading without console
// errors, and axe accessibility checks in light and dark at two widths.

import { NETWORK, openBrowser, sleep, startServer, steps } from './harness.mjs';

export async function run() {
	console.log('App (seeded library):');
	const server = await startServer();
	const token = server.seedReader();
	const b = await openBrowser();
	const { page, until, click, type, text } = b;
	const { step, skip, done } = steps('App');
	const go = async (path, waitFor) => {
		await page.goto(server.base + path);
		if (waitFor) await until(waitFor);
	};
	// Firefox takes a cookie only for a site that's open.
	await page.goto(server.base + '/api/health');
	await page.setCookie({ name: 'mau_session', value: token, domain: 'localhost', path: '/' });

	try {
		await go('/reading', 'The Left Hand of Darkness');

		await step('Reading: start a never-read book after giving its pages', async () => {
			await click('button[aria-label="Start Station Eleven"]');
			await until('How many pages does your copy have?');
			await click('button', 'Catalog lists 333 pages');
			await click('button', 'Start reading');
			await until('Started Station Eleven');
			await until('page 0 of 333');
		});

		await step('Reading: edit pages inline, Enter saves', async () => {
			await click('button[aria-label^="Edit progress, now page 140 of 304"]');
			await type('input[aria-label="Current page"]', '120');
			await page.keyboard.press('Enter');
			await until('Moved back to page 120');
			await until('page 120 of 304');
		});

		await step('Reading: pause, then Undo', async () => {
			await click('button[aria-label="Pause Middlemarch"]');
			await until('Paused Middlemarch');
			await click('.toast button', 'Undo');
			await page.waitForSelector('button[aria-label="Pause Middlemarch"]', { timeout: 3000 });
		});

		await step('Reading: the last page finishes the book and it leaves the list', async () => {
			await click('button[aria-label="Add 1 page to A Wizard of Earthsea"]');
			await until('Finished A Wizard of Earthsea');
			await sleep(300);
			if (await page.$('button[aria-label="Add 1 page to A Wizard of Earthsea"]'))
				throw new Error('still listed');
		});

		await step('Reading: search', async () => {
			await type('input[type=search]', 'eliot');
			await until('Clear search');
			if ((await page.$$('ul.list > li')).length !== 1) throw new Error('search did not filter');
			await click('button', 'Clear search');
		});

		await step('Changes survive a reload (synced to the server)', async () => {
			await sleep(1500);
			await page.reload();
			await until('page 120 of 304');
			const r = server.sql(
				"SELECT pos FROM readings r JOIN books b ON b.id = r.book_id JOIN works w ON w.id = b.work_id WHERE w.title = 'The Left Hand of Darkness' AND r.finished_on IS NULL"
			);
			if (r[0]?.pos !== 120) throw new Error(`server has ${JSON.stringify(r)}`);
		});

		await step('Breadcrumbs: a book opened from Home leads back to Home', async () => {
			await go('/', 'Pick up where you left off');
			await click('a.t');
			await until('Reading activity');
			const trail = await page.$eval('nav.crumbs ol', (e) =>
				e.innerText.replace(/\s+/g, ' ').trim()
			);
			if (!trail.startsWith('Home')) throw new Error(trail);
			await click('nav.crumbs ol a', 'Home');
			await until('Pick up where you left off');
			if (new URL(page.url()).pathname !== '/') throw new Error(page.url());
		});

		await step('Book page: rating, notes save, tags, copy', async () => {
			await go('/catalog', 'books');
			await type('input[type=search]', 'piranesi');
			await sleep(200);
			await click('a.item', 'Piranesi');
			await until('Reading activity');
			await click('button[aria-label="4 stars"]');
			await type('textarea', 'A house of endless halls.');
			await until('Saved', 4000);
			await page.type('input[aria-label="Add tag"]', 'labyrinths');
			await page.keyboard.press('Enter');
			await until('labyrinths');
			await click('button', '+ Add copy');
			await type('input[placeholder="e.g. Office, top shelf"]', 'Hall shelf');
			await click('form button[type=submit]', 'Add copy');
			await until('Hall shelf');
			await page.reload();
			await until('Hall shelf');
			const notes = await page.$eval('textarea', (e) => e.value);
			if (notes !== 'A house of endless halls.') throw new Error(`notes: ${notes}`);
		});

		await step('Book page: Escape closes the remove dialog and nothing is removed', async () => {
			await click('button[aria-label="More actions"]');
			await click('[role=menuitem]', 'Remove from list');
			await until("This can't be undone.");
			await page.keyboard.press('Escape');
			await b.gone("This can't be undone.");
		});

		await step('Book page: removing returns to the catalog with a toast', async () => {
			await click('button[aria-label="More actions"]');
			await click('[role=menuitem]', 'Remove from list');
			await click('.dialog button', 'Remove');
			await until('Removed Piranesi from your list');
			if (new URL(page.url()).pathname !== '/catalog') throw new Error(page.url());
		});

		await step('Add: enter a book by hand (private), then find it in the catalog', async () => {
			await go('/add');
			await click('button', 'Enter it manually');
			await type('form input', 'Browser Test Zine');
			const inputs = await page.$$('form input');
			await inputs[1].type('Various');
			await click('button', 'Continue');
			await sleep(600);
			if ((await text()).includes('Is it one of these?')) await click('button', 'No, add my book');
			await until('(private to you)');
			await go('/catalog', 'books');
			await until('Browser Test Zine');
		});

		if (NETWORK)
			await step('Add: search Open Library and add a book with a copy', async () => {
				await go('/add');
				await type('input[type=search]', 'the lathe of heaven');
				await page.keyboard.press('Enter');
				await until('The Lathe of Heaven', 20_000);
				await click('button.result', 'The Lathe of Heaven');
				await until('I own a copy');
				await click('.toggle input[type=checkbox]');
				await click('button', 'Add to my list');
				await until('to the top of your inactive books', 20_000);
			});
		else skip('Add: search Open Library', 'set MAU_BROWSER_NETWORK=1');

		await step('Sharing: publish finished books; the public profile lists them', async () => {
			await go('/settings/sharing', 'Share something');
			await click('button', 'Share something');
			await click('button', 'Publish');
			await until('No ratings');
			await go('/u/rri', 'Finished');
			await until('A Wizard of Earthsea');
		});

		await step('Unknown addresses show the Page not found card', async () => {
			await go('/no/such/page', 'Page not found');
			const title = await page.title();
			if (title !== 'Page not found · mau') throw new Error(title);
			await click('a', 'Go to Home');
			await until('Pick up where you left off');
		});

		const pages = [
			['/', 'Home · mau'],
			['/reading', 'Reading · mau'],
			['/catalog', 'Catalog · mau'],
			['/add', 'Add a book · mau'],
			['/settings', 'Settings · mau'],
			['/settings/account', 'Account · mau'],
			['/settings/password', 'Password · mau'],
			['/settings/two-factor', 'Two-factor · mau'],
			['/settings/sessions', 'Sessions · mau'],
			['/settings/recovery', 'Recovery codes · mau'],
			['/settings/sharing', 'Sharing · mau'],
			['/settings/google-books', 'Google Books · mau'],
			['/settings/import', 'Import & export · mau'],
			['/settings/admin/users', 'Users · mau'],
			['/settings/admin/signup', 'Signup & invites · mau'],
			['/settings/admin/providers', 'Catalog providers · mau'],
			['/settings/admin/queue', 'Review queue · mau'],
			['/settings/admin/catalog', 'Shared catalog · mau'],
			['/settings/admin/audit', 'Audit log · mau'],
			['/settings/admin/stats', 'Instance stats · mau']
		];
		await step('Every page loads with its tab title and no console errors', async () => {
			const before = b.errors.length;
			for (const [path, title] of pages) {
				await go(path);
				// Pages set their title once loaded; a busy machine can take a while.
				const end = Date.now() + 5000;
				let seen = await page.title();
				while (seen !== title && Date.now() < end) {
					await sleep(100);
					seen = await page.title();
				}
				if (seen !== title) throw new Error(`${path}: title "${seen}"`);
			}
			if (b.errors.length > before) throw new Error(b.errors.slice(before).join('\n'));
		});

		await step('No axe violations (WCAG 2.1 AA) in light and dark, phone and desktop', async () => {
			const found = [];
			for (const theme of ['light', 'dark']) {
				await page.evaluate((t) => localStorage.setItem('mau.appearance', t), theme);
				for (const [w, h] of [
					[1440, 900],
					[390, 844]
				]) {
					await page.setViewport({ width: w, height: h });
					for (const [path] of pages.slice(0, 7)) {
						await go(path);
						await sleep(700);
						for (const v of await b.axe()) found.push(`${theme} ${w}px ${path}: ${v}`);
					}
				}
			}
			await page.evaluate(() => localStorage.removeItem('mau.appearance'));
			if (found.length) throw new Error(found.join('\n        '));
		});

		await step('Signed out: the landing page, and app pages go to sign-in', async () => {
			await page.deleteCookie({ name: 'mau_session', domain: 'localhost' });
			await page.evaluate(() => localStorage.clear());
			await go('/', 'your personal library and reading log');
			await until('Source code licensed under AGPL-3.0');
			const v = await b.axe();
			if (v.length) throw new Error(v.join('\n'));
			await go('/reading', 'Forgot your password?');
			if (new URL(page.url()).pathname !== '/signin') throw new Error(page.url());
		});
	} finally {
		await b.browser.close();
		server.stop();
	}
	return done();
}
