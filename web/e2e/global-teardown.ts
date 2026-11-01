import { readFileSync, rmSync } from 'node:fs';
import { STATE_FILE } from './global-setup.ts';

export default async function globalTeardown() {
	const { pid, dataDir } = JSON.parse(readFileSync(STATE_FILE, 'utf8'));
	try {
		process.kill(pid);
	} catch {
		// Already gone.
	}
	rmSync(dataDir, { recursive: true, force: true });
}
