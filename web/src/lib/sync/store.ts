// IndexedDB persistence for one user's synced data and unsent operations.

import { openDB, type IDBPDatabase } from 'idb';
import type { Changes } from '../api/generated/Changes.ts';
import type { DownloadCursor } from '../api/generated/DownloadCursor.ts';
import type { Op } from '../api/generated/Op.ts';
import { emptyData, tagKey, type Data } from './apply.ts';

const STORES = ['books', 'tags', 'copies', 'readings', 'events', 'works'] as const;

/** `download`: where a new device's download stands, while it runs. */
export type Persisted = {
	server: Data;
	outbox: Op[];
	since: number;
	download: DownloadCursor | null;
};

export class Store {
	private constructor(private db: IDBPDatabase) {}

	static dbName(userId: string) {
		return `mau-${userId}`;
	}

	static async open(userId: string): Promise<Store> {
		const db = await openDB(Store.dbName(userId), 1, {
			upgrade(db) {
				for (const s of STORES) db.createObjectStore(s);
				db.createObjectStore('outbox', { autoIncrement: true });
				db.createObjectStore('meta');
			}
		});
		return new Store(db);
	}

	async load(): Promise<Persisted> {
		const server = emptyData();
		const tx = this.db.transaction([...STORES, 'outbox', 'meta']);
		for (const s of STORES) {
			const store = tx.objectStore(s);
			const [keys, values] = await Promise.all([store.getAllKeys(), store.getAll()]);
			const map = server[s] as Map<string, unknown>;
			keys.forEach((k, i) => map.set(String(k), values[i]));
		}
		const outbox = (await tx.objectStore('outbox').getAll()) as Op[];
		const since = ((await tx.objectStore('meta').get('since')) as number | undefined) ?? 0;
		const download = ((await tx.objectStore('meta').get('download')) ??
			null) as DownloadCursor | null;
		await tx.done;
		return { server, outbox, since, download };
	}

	async addOps(ops: Op[]) {
		const tx = this.db.transaction('outbox', 'readwrite');
		for (const op of ops) await tx.store.add(op);
		await tx.done;
	}

	/** Removes sent operations (by op ID) from the outbox. */
	async removeOps(ids: Set<string>) {
		const tx = this.db.transaction('outbox', 'readwrite');
		let cursor = await tx.store.openCursor();
		while (cursor) {
			if (ids.has((cursor.value as Op).id)) await cursor.delete();
			cursor = await cursor.continue();
		}
		await tx.done;
	}

	/** Saves synced rows. A download page also saves where the download
	 *  stands (`download`), and the last one (`download: null`) the change
	 *  number to sync from (`c.seq`), together with its rows. */
	async saveChanges(c: Changes, download?: DownloadCursor | null) {
		const tx = this.db.transaction([...STORES, 'meta'], 'readwrite');
		for (const b of c.books) tx.objectStore('books').put(b, b.id);
		for (const t of c.tags) tx.objectStore('tags').put(t, tagKey(t.book_id, t.tag));
		for (const x of c.copies) tx.objectStore('copies').put(x, x.id);
		for (const r of c.readings) tx.objectStore('readings').put(r, r.id);
		for (const e of c.events) tx.objectStore('events').put(e, e.id);
		for (const w of c.works) tx.objectStore('works').put(w, w.id);
		if (download) tx.objectStore('meta').put(download, 'download');
		else {
			tx.objectStore('meta').put(c.seq, 'since');
			if (download === null) tx.objectStore('meta').delete('download');
		}
		await tx.done;
	}

	/** Stores catalog works the device learned about before syncing (adding a book). */
	async putWorks(works: Data['works']) {
		const tx = this.db.transaction('works', 'readwrite');
		for (const [id, w] of works) tx.store.put(w, id);
		await tx.done;
	}

	close() {
		this.db.close();
	}

	static async destroy(userId: string) {
		await new Promise<void>((resolve) => {
			const req = indexedDB.deleteDatabase(Store.dbName(userId));
			req.onsuccess = req.onerror = req.onblocked = () => resolve();
		});
	}
}
