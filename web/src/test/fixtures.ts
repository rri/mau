// Mock content for page and component tests: a small, realistic library
// (active, inactive, finished, and never-started books, with progress on known
// dates), a signed-in reader, and a fixed "today".

import { vi } from 'vitest';
import type { Me } from '#lib/api/generated/Me.ts';
import type { WorkRow } from '#lib/api/generated/WorkRow.ts';
import { auth } from '#lib/auth.svelte.ts';
import { library } from '#lib/library.svelte.ts';
import { emptyData, tagKey, type Data } from '#lib/sync/apply.ts';

/** Today in the tests: Wednesday 11 November 2026, 09:00 in Berlin. */
export const TODAY = '2026-11-11';

export function freezeTime(iso = '2026-11-11T08:00:00Z') {
	vi.useFakeTimers({ toFake: ['Date'] });
	vi.setSystemTime(new Date(iso));
}

export const ME: Me = {
	id: 'u1',
	username: 'rri',
	display_name: 'R. Reader',
	bio: 'Slow reader of long novels.',
	time_zone: 'Europe/Berlin',
	is_admin: true,
	two_factor: false,
	google_books: false
};

export function signIn(me: Partial<Me> = {}, signup_mode: 'open' | 'invite' | 'closed' = 'invite') {
	auth.me = { ...ME, ...me };
	auth.status = { me: auth.me, signup_mode, setup_required: false };
	auth.ready = true;
}

export function signOut(
	signup_mode: 'open' | 'invite' | 'closed' = 'invite',
	setup_required = false
) {
	auth.me = null;
	auth.status = { me: null, signup_mode, setup_required };
	auth.ready = true;
}

type BookSpec = {
	id: string;
	title: string;
	author: string;
	/** a rank string; lower sorts first */
	rank?: string;
	active?: boolean;
	pages?: number;
	year?: number;
	/** current reading: position and total; omit for a never-started book */
	reading?: { pos: number; total: number; started: string; finished?: string | null };
	/** extra finished readings */
	finished?: string[];
	tags?: string[];
	copies?: { format: string; location: string }[];
	rating?: number;
	notes?: string;
	added?: string;
	/** forward progress events: [date, from, to] */
	events?: [string, number, number][];
	cover?: string | null;
};

let seq = 0;
const id = (p: string) => `${p}${++seq}`;

export function work(
	spec: Pick<BookSpec, 'id' | 'title' | 'author' | 'pages' | 'year' | 'cover'>
): WorkRow {
	return {
		id: 'w-' + spec.id,
		title: spec.title,
		first_year: spec.year ?? null,
		cover: spec.cover ?? null,
		private: false,
		suggested: false,
		people: [{ name: spec.author, role: 'author' }],
		editions: [
			{
				id: 'e-' + spec.id,
				work_id: 'w-' + spec.id,
				publisher: 'Ace',
				binding: 'Paperback',
				year: spec.year ?? null,
				isbn13: null,
				pages: spec.pages ?? null,
				cover: null,
				private: false
			}
		]
	};
}

export function addBook(d: Data, b: BookSpec) {
	const w = work(b);
	d.works.set(w.id, w);
	d.books.set(b.id, {
		id: b.id,
		work_id: w.id,
		rank: b.rank ?? 'V',
		active: b.active ?? false,
		rating: b.rating ?? 0,
		notes: b.notes ?? '',
		title_override: null,
		authors_override: null,
		cover_override: null,
		added_at: Date.parse((b.added ?? '2026-10-01') + 'T10:00:00Z'),
		deleted: false,
		clock: {}
	});
	for (const f of b.finished ?? []) {
		// IDs sort like real ones (UUIDv7, by creation time): older readings first,
		// and all before readings the app creates during a test.
		const rid = id('000rf');
		d.readings.set(rid, {
			id: rid,
			book_id: b.id,
			started_on: '2026-01-01',
			finished_on: f,
			total: b.pages ?? 300,
			pos: b.pages ?? 300,
			pos_clock: '',
			deleted: false,
			clock: {}
		});
	}
	if (b.reading) {
		const rid = '00r-' + b.id;
		d.readings.set(rid, {
			id: rid,
			book_id: b.id,
			started_on: b.reading.started,
			finished_on: b.reading.finished ?? null,
			total: b.reading.total,
			pos: b.reading.pos,
			pos_clock: '',
			deleted: false,
			clock: {}
		});
		for (const [date, from, to] of b.events ?? []) {
			const eid = id('ev');
			d.events.set(eid, {
				id: eid,
				reading_id: rid,
				hlc: `${date}-${String(seq).padStart(6, '0')}`,
				local_date: date,
				from_page: from,
				to_page: to,
				deleted: false
			});
		}
	}
	for (const t of b.tags ?? [])
		d.tags.set(tagKey(b.id, t), { book_id: b.id, tag: t, present: true, clock: '' });
	for (const c of b.copies ?? []) {
		const cid = id('c');
		d.copies.set(cid, {
			id: cid,
			book_id: b.id,
			format: c.format,
			location: c.location,
			edition_id: 'e-' + b.id,
			deleted: false,
			clock: {}
		});
	}
	return d;
}

/** The sample library most tests use. */
export function sampleLibrary(): Data {
	const d = emptyData();
	addBook(d, {
		id: 'lhod',
		title: 'The Left Hand of Darkness',
		author: 'Ursula K. Le Guin',
		rank: 'B',
		active: true,
		pages: 304,
		year: 1969,
		reading: { pos: 140, total: 304, started: '2026-10-15' },
		events: [
			['2026-10-20', 0, 100],
			['2026-11-09', 100, 120],
			['2026-11-10', 120, 134],
			[TODAY, 134, 140]
		],
		tags: ['classics', 'sci-fi'],
		copies: [{ format: 'Paperback', location: 'Living room' }]
	});
	addBook(d, {
		id: 'mm',
		title: 'Middlemarch',
		author: 'George Eliot',
		rank: 'C',
		active: true,
		pages: 880,
		year: 1871,
		reading: { pos: 299, total: 880, started: '2026-09-01' },
		events: [['2026-11-10', 250, 299]],
		tags: ['classics']
	});
	addBook(d, {
		id: 'se',
		title: 'Station Eleven',
		author: 'Emily St. John Mandel',
		rank: 'D',
		pages: 333,
		tags: ['book club'],
		added: '2026-10-21'
	});
	addBook(d, {
		id: 'bk',
		title: 'The Brothers Karamazov',
		author: 'Fyodor Dostoevsky',
		rank: 'E',
		pages: 796,
		reading: { pos: 310, total: 796, started: '2026-06-01' },
		events: [['2026-08-01', 0, 310]]
	});
	addBook(d, {
		id: 'pir',
		title: 'Piranesi',
		author: 'Susanna Clarke',
		pages: 245,
		finished: ['2026-10-28'],
		rating: 5,
		tags: ['fantasy'],
		copies: [{ format: 'Hardcover', location: 'Bedroom' }]
	});
	return d;
}

/** Puts `d` on screen as the signed-in reader's library. */
export function useLibrary(d: Data = sampleLibrary()) {
	library.view = d;
	library.ready = true;
	library.online = true;
	library.pending = 0;
	return d;
}

/** A fetch stub answering by method and path; unmatched calls fail loudly. */
export function mockApi(routes: Record<string, unknown | ((body: unknown) => unknown)>) {
	const fn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = new URL(String(input), 'http://localhost');
		const key = `${(init?.method ?? 'GET').toUpperCase()} ${url.pathname.replace(/^\/api/, '')}`;
		const r = routes[key];
		if (r === undefined) throw new Error(`unexpected request ${key}`);
		const body = init?.body ? JSON.parse(String(init.body)) : undefined;
		const value = typeof r === 'function' ? (r as (b: unknown) => unknown)(body) : r;
		if (value instanceof Response) return value;
		return new Response(JSON.stringify(value), {
			status: 200,
			headers: { 'content-type': 'application/json' }
		});
	});
	vi.stubGlobal('fetch', fn);
	return fn;
}
