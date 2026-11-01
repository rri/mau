import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export const STATE_FILE = join(tmpdir(), 'mau-e2e-state.json');

// Starts mau with an empty data directory and records its setup link.
export default async function globalSetup() {
	const dataDir = mkdtempSync(join(tmpdir(), 'mau-e2e-'));
	const server = spawn('../target/debug/mau', ['serve'], {
		env: {
			...process.env,
			MAU_LISTEN: '127.0.0.1:8371',
			MAU_PUBLIC_URL: 'http://localhost:8371',
			MAU_DATA_DIR: dataDir,
			MAU_LOG: 'warn'
		},
		stdio: ['ignore', 'pipe', 'pipe']
	});
	const setupToken = await new Promise<string>((resolve, reject) => {
		const timer = setTimeout(() => reject(new Error('mau did not print a setup link')), 20000);
		let buf = '';
		server.stderr.on('data', (chunk: Buffer) => {
			buf += chunk.toString();
			const m = buf.match(/\/setup\/([A-Za-z0-9_-]+)/);
			if (m) {
				clearTimeout(timer);
				resolve(m[1]);
			}
		});
		server.on('exit', (code) => reject(new Error(`mau exited with ${code}: ${buf}`)));
	});
	writeFileSync(STATE_FILE, JSON.stringify({ pid: server.pid, dataDir, setupToken }));
	server.unref();
}
