import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
