import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Page } from '@playwright/test';
import { STATE_FILE } from './global-setup.ts';

function state(): { setupToken: string; dataDir: string } {
	return JSON.parse(readFileSync(STATE_FILE, 'utf8'));
}

export function setupToken(): string {
	return state().setupToken;
}

/** Where the signed-in admin's cookies are saved for later test files. */
export const AUTH_FILE = join(tmpdir(), 'mau-e2e-auth.json');

/** Adds the sample books to an account (the server allows it on localhost). */
export function seed(username: string) {
	execFileSync('../target/debug/mau', ['seed', '--user', username], {
		env: {
			...process.env,
			MAU_DATA_DIR: state().dataDir,
			MAU_PUBLIC_URL: 'http://localhost:8371',
			MAU_LOG: 'warn'
		}
	});
}

/** Adds a Chromium virtual authenticator that stores discoverable passkeys
 *  and always verifies the user. Shared across pages of the same context. */
export async function addAuthenticator(page: Page) {
	const cdp = await page.context().newCDPSession(page);
	await cdp.send('WebAuthn.enable');
	const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
		options: {
			protocol: 'ctap2',
			transport: 'internal',
			hasResidentKey: true,
			hasUserVerification: true,
			isUserVerified: true,
			automaticPresenceSimulation: true
		}
	});
	return { cdp, authenticatorId };
}
