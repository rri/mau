import { readFileSync } from 'node:fs';
import type { Page } from '@playwright/test';
import { STATE_FILE } from './global-setup.ts';

export function setupToken(): string {
	return JSON.parse(readFileSync(STATE_FILE, 'utf8')).setupToken;
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
