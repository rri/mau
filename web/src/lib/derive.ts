// Read-only views over library data: what a book is called, its current
// reading, its state, and page counts.

import type { BookRow } from './api/generated/BookRow.ts';
import type { EventRow } from './api/generated/EventRow.ts';
import type { ReadingRow } from './api/generated/ReadingRow.ts';
import type { WorkRow } from './api/generated/WorkRow.ts';
import type { Data } from './sync/apply.ts';

export type BookState = 'active' | 'inactive' | 'finished';

export type BookInfo = {
	book: BookRow;
	work: WorkRow | undefined;
	title: string;
	authors: string[];
	cover: string | null;
	reading: ReadingRow | null;
	readings: ReadingRow[];
	state: BookState;
	owned: boolean;
	tags: string[];
	/** The catalog's page count for the book (a hint for setup). */
	catalogPages: number | null;
};

type Index = {
	readingsByBook: Map<string, ReadingRow[]>;
	eventsByReading: Map<string, EventRow[]>;
	tagsByBook: Map<string, string[]>;
	copiesByBook: Map<string, number>;
	infos: Map<string, BookInfo>;
};

const cache = new WeakMap<Data, Index>();

function index(d: Data): Index {
	let ix = cache.get(d);
	if (ix) return ix;
	const readingsByBook = new Map<string, ReadingRow[]>();
	for (const r of d.readings.values()) {
		if (r.deleted) continue;
		const list = readingsByBook.get(r.book_id) ?? [];
		list.push(r);
		readingsByBook.set(r.book_id, list);
	}
	for (const list of readingsByBook.values()) list.sort((a, b) => (a.id < b.id ? -1 : 1));
	const eventsByReading = new Map<string, EventRow[]>();
	for (const e of d.events.values()) {
		if (e.deleted) continue;
		const list = eventsByReading.get(e.reading_id) ?? [];
		list.push(e);
		eventsByReading.set(e.reading_id, list);
	}
	for (const list of eventsByReading.values()) list.sort((a, b) => (a.hlc < b.hlc ? -1 : 1));
	const tagsByBook = new Map<string, string[]>();
	for (const t of d.tags.values()) {
		if (!t.present) continue;
		const list = tagsByBook.get(t.book_id) ?? [];
		list.push(t.tag);
		tagsByBook.set(t.book_id, list);
	}
	const copiesByBook = new Map<string, number>();
	for (const c of d.copies.values())
		if (!c.deleted) copiesByBook.set(c.book_id, (copiesByBook.get(c.book_id) ?? 0) + 1);
	ix = { readingsByBook, eventsByReading, tagsByBook, copiesByBook, infos: new Map() };
	cache.set(d, ix);
	return ix;
}

export function authorsOf(work: WorkRow | undefined): string[] {
	return work?.people.filter((p) => p.role === 'author').map((p) => p.name) ?? [];
}

export function bookInfo(d: Data, book: BookRow): BookInfo {
	const ix = index(d);
	const cached = ix.infos.get(book.id);
	if (cached) return cached;
	const work = d.works.get(book.work_id);
	const readings = ix.readingsByBook.get(book.id) ?? [];
	const reading = readings.at(-1) ?? null;
	const state: BookState = reading?.finished_on ? 'finished' : book.active ? 'active' : 'inactive';
	const pages = work?.editions.map((e) => e.pages).find((p) => p != null) ?? null;
	const info: BookInfo = {
		book,
		work,
		title: book.title_override ?? work?.title ?? 'Unknown book',
		authors: book.authors_override ?? authorsOf(work),
		cover: book.cover_override ?? work?.cover ?? work?.editions.find((e) => e.cover)?.cover ?? null,
		reading,
		readings,
		state,
		owned: (ix.copiesByBook.get(book.id) ?? 0) > 0,
		tags: (ix.tagsByBook.get(book.id) ?? []).sort((a, b) => a.localeCompare(b)),
		catalogPages: pages
	};
	ix.infos.set(book.id, info);
	return info;
}

export function liveBooks(d: Data): BookInfo[] {
	const out: BookInfo[] = [];
	for (const b of d.books.values())
		if (!b.deleted && d.works.has(b.work_id)) out.push(bookInfo(d, b));
	return out;
}

export function eventsOf(d: Data, readingId: string): EventRow[] {
	return index(d).eventsByReading.get(readingId) ?? [];
}

/** Pages (or percent, for a reading tracked by percent) read on `date` in
 *  this reading: forward moves in its current unit only. */
export function pagesOn(d: Data, reading: ReadingRow, date: string): number {
	let n = 0;
	for (const e of eventsOf(d, reading.id))
		if (e.local_date === date && e.unit === reading.unit && e.to_page > e.from_page)
			n += e.to_page - e.from_page;
	return n;
}

/** "page 140 of 304", or "42%" for a reading tracked by percent. */
export function progressText(r: ReadingRow): string {
	return r.unit === 'percent'
		? `${r.pos}%`
		: `page ${r.pos.toLocaleString()} of ${r.total.toLocaleString()}`;
}

/** The short form for tiles: "p. 140/304" or "42%". */
export function progressShort(r: ReadingRow): string {
	return r.unit === 'percent' ? `${r.pos}%` : `p. ${r.pos}/${r.total}`;
}

/** The last date anything was read in this reading. */
export function lastReadDate(d: Data, readingId: string): string | null {
	return eventsOf(d, readingId).at(-1)?.local_date ?? null;
}

/** The Reading view's list: active books by rank, then inactive by rank. */
export function readingList(d: Data): BookInfo[] {
	const books = liveBooks(d).filter((b) => b.state !== 'finished');
	const byRank = (a: BookInfo, b: BookInfo) =>
		a.book.rank < b.book.rank ? -1 : a.book.rank > b.book.rank ? 1 : 0;
	return [
		...books.filter((b) => b.state === 'active').sort(byRank),
		...books.filter((b) => b.state === 'inactive').sort(byRank)
	];
}

/** Whether `needle` (already lowercased and trimmed) is in the title or an author. */
export function matchesSearch(b: BookInfo, needle: string): boolean {
	return (
		!needle ||
		b.title.toLocaleLowerCase().includes(needle) ||
		b.authors.some((a) => a.toLocaleLowerCase().includes(needle))
	);
}
