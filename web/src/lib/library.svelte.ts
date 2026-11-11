// The signed-in user's library on this device: synced data from the server
// plus operations not yet sent, shown together. Changes apply locally at once
// and sync in the background, so everything works offline.

import { ApiError, post } from './api/client.ts';
import type { Op } from './api/generated/Op.ts';
import type { OpKind } from './api/generated/OpKind.ts';
import type { SyncResponse } from './api/generated/SyncResponse.ts';
import type { WorkRow } from './api/generated/WorkRow.ts';
import { auth } from './auth.svelte.ts';
import { applyOps, cloneData, emptyData, tagKey, type Data } from './sync/apply.ts';
import { now as hlcNow, observe } from './sync/hlc.ts';
import { uuidv7 } from './sync/ids.ts';
import { Store } from './sync/store.ts';
import { toasts } from './toast.svelte.ts';

const BATCH = 500;
const PUSH_DELAY_MS = 800;
const POLL_MS = 60_000;

class Library {
	/** What the screens show: server data with unsent operations applied. */
	view = $state.raw<Data>(emptyData());
	ready = $state(false);
	pending = $state(0);
	online = $state(typeof navigator === 'undefined' ? true : navigator.onLine);
	syncing = $state(false);

	private server: Data = emptyData();
	private outbox: Op[] = [];
	private since = 0;
	private store: Store | null = null;
	private userId: string | null = null;
	private pushTimer: ReturnType<typeof setTimeout> | undefined;
	private pollTimer: ReturnType<typeof setInterval> | undefined;
	private syncAgain = false;

	async open(userId: string) {
		if (this.userId === userId) return;
		this.close();
		this.userId = userId;
		this.store = await Store.open(userId);
		const p = await this.store.load();
		this.server = p.server;
		this.outbox = p.outbox;
		this.since = p.since;
		for (const r of this.server.readings.values()) observe(r.pos_clock);
		this.rebuild();
		this.ready = true;
		window.addEventListener('online', this.onOnline);
		window.addEventListener('offline', this.onOffline);
		document.addEventListener('visibilitychange', this.onVisible);
		this.pollTimer = setInterval(() => this.sync(), POLL_MS);
		this.sync();
	}

	close() {
		clearTimeout(this.pushTimer);
		clearInterval(this.pollTimer);
		if (typeof window !== 'undefined') {
			window.removeEventListener('online', this.onOnline);
			window.removeEventListener('offline', this.onOffline);
			document.removeEventListener('visibilitychange', this.onVisible);
		}
		this.store?.close();
		this.store = null;
		this.userId = null;
		this.server = emptyData();
		this.outbox = [];
		this.view = emptyData();
		this.ready = false;
		this.pending = 0;
	}

	/** Deletes this device's copy (signing out, or a revoked session). */
	async wipe() {
		const id = this.userId;
		this.close();
		if (id) await Store.destroy(id);
	}

	private onOnline = () => {
		this.online = true;
		this.sync();
	};
	private onOffline = () => (this.online = false);
	private onVisible = () => {
		if (document.visibilityState === 'visible') this.sync();
	};

	private rebuild() {
		this.view = applyOps(cloneData(this.server), this.outbox);
		this.pending = this.outbox.length;
	}

	/** Records operations: applies them locally now and queues them for the server. */
	commit(kinds: OpKind[]): Op[] {
		const ops: Op[] = kinds.map((op) => ({ id: uuidv7(), hlc: hlcNow(), op }));
		this.outbox.push(...ops);
		this.view = applyOps(cloneData(this.view), ops);
		this.pending = this.outbox.length;
		this.store?.addOps(ops);
		clearTimeout(this.pushTimer);
		this.pushTimer = setTimeout(() => this.sync(), PUSH_DELAY_MS);
		return ops;
	}

	/** Adds catalog works this device needs before the server confirms them. */
	addWorks(works: WorkRow[]) {
		for (const w of works) this.server.works.set(w.id, w);
		this.store?.putWorks(new Map(works.map((w) => [w.id, w])));
		this.rebuild();
	}

	async sync(): Promise<void> {
		if (!this.store || !this.userId) return;
		if (this.syncing) {
			this.syncAgain = true;
			return;
		}
		this.syncing = true;
		try {
			do {
				this.syncAgain = false;
				const batch = this.outbox.slice(0, BATCH);
				const res = await post<SyncResponse>('/sync', { since: this.since, ops: batch });
				this.online = true;
				const sent = new Set(batch.map((o) => o.id));
				this.outbox = this.outbox.filter((o) => !sent.has(o.id));
				await this.store.removeOps(sent);
				this.merge(res);
				if (res.rejected.length) {
					toasts.show(`Some changes couldn't be saved: ${res.rejected[0].message}`);
				}
				if (this.outbox.length) this.syncAgain = true;
			} while (this.syncAgain && this.store);
		} catch (e) {
			if (e instanceof ApiError && e.status === 401) {
				// The session was revoked: this device's copy and unsent changes go.
				await this.wipe();
				auth.clear();
				location.assign('/signin');
				return;
			}
			if (e instanceof ApiError && e.offline) this.online = false;
		} finally {
			this.syncing = false;
		}
		// A sync asked for while this one ran (coming back online, say) still
		// happens, even when this one failed.
		if (this.syncAgain && this.store) {
			this.syncAgain = false;
			return this.sync();
		}
	}

	private merge(res: SyncResponse) {
		const c = res.changes;
		const s = this.server;
		for (const b of c.books) s.books.set(b.id, b);
		for (const t of c.tags) s.tags.set(tagKey(t.book_id, t.tag), t);
		for (const x of c.copies) s.copies.set(x.id, x);
		for (const r of c.readings) {
			s.readings.set(r.id, r);
			observe(r.pos_clock);
		}
		for (const e of c.events) s.events.set(e.id, e);
		for (const w of c.works) s.works.set(w.id, w);
		this.since = c.seq;
		this.store?.saveChanges(c);
		this.rebuild();
		if (c.books.length || c.works.length) this.prefetchCovers();
	}

	private prefetchTimer: ReturnType<typeof setTimeout> | undefined;

	/** Keeps small and medium cover images in the offline cache (the service
	 *  worker's `mau-covers`), so covers show offline too. Full-size ones aren't kept. */
	private prefetchCovers() {
		clearTimeout(this.prefetchTimer);
		this.prefetchTimer = setTimeout(async () => {
			if (typeof caches === 'undefined') return;
			const hashes = new Set<string>();
			for (const b of this.view.books.values()) {
				if (b.deleted) continue;
				const w = this.view.works.get(b.work_id);
				const h = b.cover_override ?? w?.cover ?? w?.editions.find((e) => e.cover)?.cover;
				if (h) hashes.add(h);
			}
			const cache = await caches.open('mau-covers');
			const urls = [...hashes].flatMap((h) => [`/covers/${h}-s.jpg`, `/covers/${h}-m.jpg`]);
			const queue: string[] = [];
			for (const url of urls) if (!(await cache.match(url))) queue.push(url);
			const worker = async () => {
				for (let url = queue.shift(); url; url = queue.shift()) {
					try {
						const res = await fetch(url);
						if (res.ok) await cache.put(url, res);
					} catch {
						return; // Offline again; try after the next sync.
					}
				}
			};
			await Promise.all([worker(), worker(), worker()]);
		}, 2000);
	}
}

export const library = new Library();
