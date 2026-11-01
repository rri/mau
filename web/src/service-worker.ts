/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />
// Offline support: the app shell and assets are cached at install; page loads
// fall back to the cached shell; cover images are cached as they're seen.
// The API is never cached here; offline data lives in IndexedDB.

import { version } from '$app/env';
import { assets, immutable } from '$app/manifest';

const sw = self as unknown as ServiceWorkerGlobalScope;
const build = immutable.map((f) => f.path);
const files = assets.map((f) => f.path);
const APP_CACHE = `mau-app-${version}`;
const COVER_CACHE = 'mau-covers';
const SHELL = '/200.html';
const ASSETS = [...build, ...files, SHELL];

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(APP_CACHE)
			.then((c) => c.addAll(ASSETS))
			.then(() => sw.skipWaiting())
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) =>
				Promise.all(
					keys
						.filter((k) => k.startsWith('mau-app-') && k !== APP_CACHE)
						.map((k) => caches.delete(k))
				)
			)
			.then(() => sw.clients.claim())
	);
});

sw.addEventListener('fetch', (event) => {
	const req = event.request;
	if (req.method !== 'GET') return;
	const url = new URL(req.url);
	if (url.origin !== location.origin || url.pathname.startsWith('/api/')) return;
	// Public profile and share pages are server-rendered; let them through.
	if (url.pathname.startsWith('/u/') || url.pathname.startsWith('/s/')) return;

	if (url.pathname.startsWith('/covers/')) {
		event.respondWith(
			caches.open(COVER_CACHE).then(async (cache) => {
				const hit = await cache.match(req);
				if (hit) return hit;
				const res = await fetch(req);
				if (res.ok) cache.put(req, res.clone());
				return res;
			})
		);
		return;
	}

	if (req.mode === 'navigate') {
		event.respondWith(
			fetch(req).catch(async () => (await caches.match(SHELL)) ?? Response.error())
		);
		return;
	}

	if (ASSETS.includes(url.pathname)) {
		event.respondWith(caches.match(req).then((hit) => hit ?? fetch(req)));
	}
});
