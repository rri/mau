// Runs the Firefox suites (`just browser-test`, which builds first).
import { OUT } from './harness.mjs';
import * as app from './app.mjs';
import * as auth from './auth.mjs';

const only = process.argv[2];
let failed = 0;
for (const [name, suite] of Object.entries({ app, auth }))
	if (!only || only === name) failed += await suite.run();
if (failed) console.log(`Screenshots of failures: ${OUT}`);
process.exit(failed ? 1 : 0);
