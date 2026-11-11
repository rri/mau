// Shared pieces for the Firefox suites: a throwaway mau server, a headless
// Firefox, and small helpers. Run with `just browser-test`.
//
//   MAU_BIN                path to the mau binary (default: target/debug/mau)
//   FIREFOX                path to Firefox (default: /usr/bin/firefox)
//   MAU_BROWSER_NETWORK=1  also run steps that call the real Open Library
//   MAU_BROWSER_OUT        where failure screenshots go (default: a temp dir)

import { spawn, execFileSync } from 'node:child_process';
import { createHash, createHmac } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, mkdirSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import puppeteer from 'puppeteer-core';

const ROOT = resolve(import.meta.dirname, '../..');
export const BIN = process.env.MAU_BIN ?? join(ROOT, 'target/debug/mau');
const FIREFOX = process.env.FIREFOX ?? '/usr/bin/firefox';
export const NETWORK = process.env.MAU_BROWSER_NETWORK === '1';
export const OUT = process.env.MAU_BROWSER_OUT ?? mkdtempSync(join(tmpdir(), 'mau-browser-'));
mkdirSync(OUT, { recursive: true });

const freePort = () =>
	new Promise((ok) => {
		const s = createServer().listen(0, '127.0.0.1', () => {
			const { port } = s.address();
			s.close(() => ok(port));
		});
	});

/** Starts mau on a free port with an empty data directory. */
export async function startServer() {
	const dir = mkdtempSync(join(tmpdir(), 'mau-data-'));
	const port = await freePort();
	const base = `http://localhost:${port}`;
	const env = {
		...process.env,
		MAU_DATA_DIR: dir,
		MAU_LISTEN: `127.0.0.1:${port}`,
		MAU_PUBLIC_URL: base,
		MAU_LOG: 'info'
	};
	let log = '';
	const proc = spawn(BIN, ['serve'], { env });
	proc.stdout.on('data', (d) => (log += d));
	proc.stderr.on('data', (d) => (log += d));
	for (let i = 0; i < 100; i++) {
		try {
			if ((await fetch(base + '/api/health')).ok) break;
		} catch {
			// not up yet
		}
		await sleep(100);
	}
	const db = () => new DatabaseSync(join(dir, 'mau.db'));
	return {
		base,
		dir,
		env,
		log: () => log,
		sql(query, ...args) {
			const d = db();
			try {
				return d.prepare(query).all(...args);
			} finally {
				d.close();
			}
		},
		/** An admin `rri` with the sample books and a signed-in session cookie. */
		seedReader() {
			const token = 'browser-test-session';
			const hash = createHash('sha256').update(token).digest('base64url');
			this.sql(
				"INSERT INTO users (id, username, display_name, bio, time_zone, is_admin, created_at) VALUES ('u1', 'rri', 'R. Reader', 'Slow reader of long novels.', 'Europe/Berlin', 1, 0)"
			);
			this.sql(
				"INSERT INTO sessions (id, token_hash, user_id, device, created_at, last_seen_at, expires_at, reauth_at) VALUES ('s1', ?, 'u1', 'Firefox', 0, 0, 9999999999999, 9999999999999)",
				hash
			);
			execFileSync(BIN, ['seed', '--user', 'rri'], { env });
			return token;
		},
		stop() {
			proc.kill();
			rmSync(dir, { recursive: true, force: true });
		}
	};
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Headless Firefox with one page, collecting console errors. */
export async function openBrowser() {
	const browser = await puppeteer.launch({
		browser: 'firefox',
		executablePath: FIREFOX,
		headless: true
	});
	const page = await browser.newPage();
	await page.setViewport({ width: 420, height: 900 });
	const errors = [];
	page.on('console', (m) => {
		if (m.type() === 'error') errors.push(m.text());
	});
	page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));

	const text = () => page.evaluate(() => document.body.innerText);
	const shot = (name) => page.screenshot({ path: join(OUT, `${name}.png`) });
	return {
		browser,
		page,
		errors,
		text,
		shot,
		/** Waits until the page shows `s` (case-insensitive). */
		async until(s, ms = 10_000) {
			const end = Date.now() + ms;
			while (Date.now() < end) {
				if ((await text()).toLowerCase().includes(s.toLowerCase())) return;
				await sleep(100);
			}
			await shot('fail-' + s.replace(/\W+/g, '_').slice(0, 40));
			throw new Error(`never saw "${s}". Page: ${(await text()).slice(0, 300)}`);
		},
		async gone(s, ms = 5000) {
			const end = Date.now() + ms;
			while (Date.now() < end) {
				if (!(await text()).toLowerCase().includes(s.toLowerCase())) return;
				await sleep(100);
			}
			throw new Error(`still showing "${s}"`);
		},
		/** Clicks the first `sel` whose text is (or contains) `txt`. */
		async click(sel, txt) {
			await page.waitForSelector(sel);
			for (const el of await page.$$(sel)) {
				const t = await el.evaluate((e) => e.textContent.trim());
				if (txt === undefined || t === txt || t.includes(txt)) {
					await el.evaluate((e) => e.click());
					return;
				}
			}
			throw new Error(`no ${sel} with text "${txt}"`);
		},
		async type(sel, value) {
			await page.waitForSelector(sel);
			await page.$eval(sel, (e) => (e.value = ''));
			await page.type(sel, value);
		},
		/** WCAG 2.1 A/AA and best-practice violations on the current page. */
		async axe() {
			await page.evaluate(
				readFileSync(join(import.meta.dirname, '../node_modules/axe-core/axe.min.js'), 'utf8')
			);
			return page.evaluate(async () =>
				(
					await globalThis.axe.run(document, {
						runOnly: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice']
					})
				).violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)
			);
		}
	};
}

/** Runs named steps; a failing step is reported and the run continues. */
export function steps(suite) {
	let failed = 0;
	let skipped = 0;
	return {
		async step(name, fn) {
			try {
				await fn();
				console.log(`  ok    ${name}`);
			} catch (e) {
				failed++;
				console.log(`  FAIL  ${name}\n        ${e.message}`);
			}
		},
		skip(name, why) {
			skipped++;
			console.log(`  skip  ${name} (${why})`);
		},
		done() {
			console.log(
				`${suite}: ${failed ? `${failed} failed` : 'all passed'}${skipped ? `, ${skipped} skipped` : ''}`
			);
			return failed;
		}
	};
}

/** Time-based one-time codes, as an authenticator app makes them. */
export function totp(secret, offset = 0) {
	const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
	let bits = '';
	for (const c of secret.replace(/\s/g, '')) bits += A.indexOf(c).toString(2).padStart(5, '0');
	const key = Buffer.from(bits.match(/.{8}/g).map((b) => parseInt(b, 2)));
	const msg = Buffer.alloc(8);
	msg.writeBigInt64BE(BigInt(Math.floor(Date.now() / 30_000) + offset));
	const h = createHmac('sha1', key).update(msg).digest();
	const o = h[19] & 15;
	return String((h.readUInt32BE(o) & 0x7fffffff) % 1e6).padStart(6, '0');
}
