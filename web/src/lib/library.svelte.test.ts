// The library on this device (spec §11): changes apply at once, wait while
// offline, sync when back, survive a reload, and a revoked session wipes them.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { OpKind } from './api/generated/OpKind.ts';
import { auth } from './auth.svelte.ts';
import { library } from './library.svelte.ts';
import { toasts } from './toast.svelte.ts';
import { Store } from './sync/store.ts';
import { ME, mockApi as mockAll, signIn, work } from '../test/fixtures.ts';

const changes = (over: object = {}) => ({
	seq: 1,
	books: [],
	tags: [],
	copies: [],
	readings: [],
	events: [],
	works: [],
	...over
});
const W = work({ id: 'a', title: 'Alpha', author: 'A' });

// A new device first downloads its library (SYNC-09); unless a test says
// otherwise, that's one empty page, after which syncs start from 1.
const page = (over: object = {}, next: unknown = null) => ({
	changes: changes(over),
	next,
	books_total: 0
});
const mockApi: typeof mockAll = (routes) =>
	mockAll({ 'POST /sync/download': page({ seq: 1 }), ...routes });
const addBook: OpKind[] = [
	{ type: 'book.create', id: 'b1', work_id: W.id, rank: 'M', added_at: 1 }
];
const sent = (fetch: ReturnType<typeof mockApi>) =>
	fetch.mock.calls
		.filter(([u]) => String(u) === '/api/sync')
		.map(([, i]) => JSON.parse(String(i!.body)) as { since: number; ops: { op: OpKind }[] });

beforeEach(async () => {
	signIn();
	await Store.destroy(ME.id);
});

describe('Library sync', () => {
	it('SYNC-01 opening loads the server’s changes and shows them', async () => {
		mockApi({ 'POST /sync': { dropped: [], rejected: [], changes: changes({ works: [W] }) } });
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.view.works.get(W.id)?.title).toBe('Alpha'));
		expect(library.ready).toBe(true);
		expect(library.online).toBe(true);
	});

	it('SYNC-02 a change shows at once and is sent shortly after; nothing is left pending', async () => {
		const fetch = mockApi({ 'POST /sync': { dropped: [], rejected: [], changes: changes() } });
		await library.open(ME.id);
		library.addWorks([W]);
		library.commit(addBook);
		expect(library.view.books.get('b1')).toBeDefined();
		expect(library.pending).toBe(1);
		await vi.waitFor(() => expect(library.pending).toBe(0), { timeout: 2000 });
		expect(
			sent(fetch)
				.at(-1)!
				.ops.map((o) => o.op.type)
		).toEqual(['book.create']);
	});

	it('SYNC-03 offline, changes wait (and survive a reload); back online they’re sent', async () => {
		let up = false;
		const fetch = mockApi({
			'POST /sync': () =>
				up
					? { dropped: [], rejected: [], changes: changes() }
					: Promise.reject(new TypeError('offline'))
		});
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.online).toBe(false));
		library.addWorks([W]);
		library.commit(addBook);
		await new Promise((r) => setTimeout(r, 900));
		expect(library.pending).toBe(1);

		// A reload: the queued change is still there.
		library.close();
		await library.open(ME.id);
		expect(library.pending).toBe(1);
		expect(library.view.books.get('b1')).toBeDefined();

		up = true;
		window.dispatchEvent(new Event('online'));
		await vi.waitFor(() => expect(library.pending).toBe(0));
		expect(library.online).toBe(true);
		expect(sent(fetch).at(-1)!.ops).toHaveLength(1);
	});

	it('SYNC-04 changes the server rejects are dropped, with a toast saying why', async () => {
		mockApi({
			'POST /sync': (b: { ops: { id: string }[] }) => ({
				dropped: [],
				rejected: b.ops.map((o) => ({ id: o.id, message: 'That book is gone.' })),
				changes: changes()
			})
		});
		await library.open(ME.id);
		library.commit(addBook);
		await vi.waitFor(() =>
			expect(toasts.current?.message).toBe("Some changes couldn't be saved: That book is gone.")
		);
		expect(library.pending).toBe(0);
	});

	it('SYNC-05 a revoked session wipes this device’s copy and goes to sign-in', async () => {
		const assign = vi.fn();
		vi.stubGlobal('location', { ...location, assign });
		let revoked = false;
		mockApi({
			'POST /sync': () =>
				revoked
					? new Response('{"error":"unauthorized","message":"Sign in."}', { status: 401 })
					: { dropped: [], rejected: [], changes: changes({ works: [W] }) }
		});
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.view.works.size).toBe(1));
		revoked = true;
		await library.sync();
		expect(assign).toHaveBeenCalledWith('/signin');
		expect(auth.me).toBeNull();
		expect(library.ready).toBe(false);
		const s = await Store.open(ME.id);
		expect((await s.load()).server.works.size).toBe(0);
		s.close();
	});

	it('SYNC-06 later syncs ask only for what changed since the last one', async () => {
		let seq = 0;
		const fetch = mockApi({
			'POST /sync': () => ({ dropped: [], rejected: [], changes: changes({ seq: ++seq * 10 }) })
		});
		await library.open(ME.id);
		await vi.waitFor(() => expect(sent(fetch)).toHaveLength(1));
		await library.sync();
		await vi.waitFor(() => expect(sent(fetch).map((b) => b.since)).toEqual([1, 10]));
	});
	it('SYNC-07 a new device shows its library once the first page arrives, not an empty one before', async () => {
		let answer: (v: unknown) => void = () => {};
		mockApi({
			'POST /sync/download': () =>
				new Promise((r) => (answer = r)).then(() => page({ seq: 5, works: [W] })),
			'POST /sync': { dropped: [], rejected: [], changes: changes({ seq: 5 }) }
		});
		await library.open(ME.id);
		expect(library.ready).toBe(false);
		answer(null);
		await vi.waitFor(() => expect(library.ready).toBe(true));
		expect(library.view.works.size).toBe(1);

		// Next time, this device's copy shows at once.
		library.close();
		mockApi({ 'POST /sync': () => new Promise(() => {}) });
		await library.open(ME.id);
		expect(library.ready).toBe(true);
		expect(library.view.works.size).toBe(1);
	});

	it('SYNC-07 offline on a new device, it stops waiting when the first sync fails', async () => {
		mockApi({ 'POST /sync/download': () => Promise.reject(new TypeError('offline')) });
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.ready).toBe(true));
		expect(library.online).toBe(false);
	});

	it('SYNC-08 a sync that brings nothing new keeps the same view (the screens don’t redraw)', async () => {
		mockApi({ 'POST /sync': { dropped: [], rejected: [], changes: changes({ works: [W] }) } });
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.view.works.size).toBe(1));
		mockApi({ 'POST /sync': { dropped: [], rejected: [], changes: changes({ seq: 2 }) } });
		const before = library.view;
		await library.sync();
		expect(library.view).toBe(before);
	});

	it('SYNC-09 a new device downloads its library a page at a time, usable from the first', async () => {
		const book = (id: string, active = false) => ({
			id,
			work_id: W.id,
			rank: 'V',
			active,
			rating: 0,
			notes: '',
			title_override: null,
			authors_override: null,
			cover_override: null,
			added_at: 1,
			deleted: false,
			clock: {}
		});
		const cursor = { upto: 7, active_only: false, after: 'b1' };
		let second: (v: unknown) => void = () => {};
		let offline = false;
		const fetch = mockApi({
			'POST /sync/download': (body: { cursor: unknown }) =>
				body.cursor === null
					? { ...page({ seq: 7, books: [book('b1', true)], works: [W] }, cursor), books_total: 2 }
					: offline
						? Promise.reject(new TypeError('offline'))
						: new Promise((r) => (second = r)).then(() => ({
								...page({ seq: 7, books: [book('b2')], works: [W] }),
								books_total: 2
							})),
			'POST /sync': { dropped: [], rejected: [], changes: changes({ seq: 9 }) }
		});
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.view.books.size).toBe(1));
		expect(library.ready).toBe(true);
		expect(library.downloading).toEqual({ done: 1, total: 2 });
		// A change made meanwhile waits until the download is done.
		library.commit([{ type: 'book.set', id: 'b1', fields: { rating: 5 } }]);
		second(null);
		await vi.waitFor(() => expect(library.downloading).toBeNull());
		expect(library.view.books.size).toBe(2);
		await vi.waitFor(() => expect(library.pending).toBe(0), { timeout: 2000 });
		expect(sent(fetch)[0]).toMatchObject({ since: 7, ops: [{ op: { type: 'book.set' } }] });

		// Cut off part-way, a reload shows what came and carries on from there.
		library.close();
		await Store.destroy(ME.id);
		offline = true;
		await library.open(ME.id);
		await vi.waitFor(() => expect(library.online).toBe(false));
		expect(library.view.books.size).toBe(1);
		library.close();
		offline = false;
		await library.open(ME.id);
		expect(library.ready).toBe(true);
		expect(library.view.books.size).toBe(1);
		const downloads = fetch.mock.calls.filter(([u]) => String(u) === '/api/sync/download');
		expect(JSON.parse(String(downloads.at(-1)![1]!.body))).toEqual({ cursor });
		second(null);
		await vi.waitFor(() => expect(library.view.books.size).toBe(2));
	});
});
