// Applies operations to local data with the same rules as the server
// (crates/mau-server/src/library.rs): field-level last-writer-wins, and
// chain-based progress conflicts (crates/mau-core/src/progress.rs). The
// server stays authoritative; this keeps the screen right until it answers.

import type { BookRow } from '../api/generated/BookRow.ts';
import type { CopyRow } from '../api/generated/CopyRow.ts';
import type { EventRow } from '../api/generated/EventRow.ts';
import type { Op } from '../api/generated/Op.ts';
import type { ReadingRow } from '../api/generated/ReadingRow.ts';
import type { TagRow } from '../api/generated/TagRow.ts';
import type { Unit } from '../api/generated/Unit.ts';
import type { WorkRow } from '../api/generated/WorkRow.ts';

export type Data = {
	books: Map<string, BookRow>;
	tags: Map<string, TagRow>;
	copies: Map<string, CopyRow>;
	readings: Map<string, ReadingRow>;
	events: Map<string, EventRow>;
	works: Map<string, WorkRow>;
};

export const tagKey = (bookId: string, tag: string) => `${bookId}\u0000${tag}`;

export function emptyData(): Data {
	return {
		books: new Map(),
		tags: new Map(),
		copies: new Map(),
		readings: new Map(),
		events: new Map(),
		works: new Map()
	};
}

/** A copy whose maps can be changed without touching the original. */
export function cloneData(d: Data): Data {
	return {
		books: new Map(d.books),
		tags: new Map(d.tags),
		copies: new Map(d.copies),
		readings: new Map(d.readings),
		events: new Map(d.events),
		works: new Map(d.works)
	};
}

type Clock = { [k: string]: string | undefined };

/** Applies `fields` whose clocks `hlc` beats. Returns the new row or null if nothing changed. */
function lww<T extends { clock: Clock }>(
	row: T,
	fields: Partial<Record<keyof T, unknown>>,
	hlc: string,
	map?: (k: string, v: unknown) => unknown
): T | null {
	let changed = false;
	const next = { ...row, clock: { ...row.clock } } as T;
	for (const [k, v] of Object.entries(fields)) {
		if (v === undefined || v === null) continue;
		if ((next.clock[k] ?? '') >= hlc) continue;
		next.clock[k] = hlc;
		(next as Record<string, unknown>)[k] = map ? map(k, v) : v;
		changed = true;
	}
	return changed ? next : null;
}

type Chain = { root: string; tip: string; first: boolean; chain: number };

/** Mirrors mau_core::progress::chains. */
function chains(ops: Op[]): Map<string, Chain> {
	const out = new Map<string, Chain>();
	const open = new Map<string, { idx: number; last: string }>();
	const roots: string[] = [];
	const tips: string[] = [];
	const order: { opId: string; idx: number; first: boolean }[] = [];
	for (const op of ops) {
		const k = op.op;
		if (k.type !== 'progress' && k.type !== 'progress.undo' && k.type !== 'progress.unit') continue;
		const o = open.get(k.reading_id);
		let idx: number;
		let first = false;
		if (o && o.last === k.base) idx = o.idx;
		else {
			roots.push(k.base);
			tips.push('');
			idx = roots.length - 1;
			first = true;
		}
		if (op.hlc > tips[idx]) tips[idx] = op.hlc;
		open.set(k.reading_id, { idx, last: op.hlc });
		order.push({ opId: op.id, idx, first });
	}
	for (const o of order)
		out.set(o.opId, { root: roots[o.idx], tip: tips[o.idx], first: o.first, chain: o.idx });
	return out;
}

/** Applies operations in order, mutating `d` (pass a clone). */
export function applyOps(d: Data, ops: Op[]): Data {
	const chainOf = chains(ops);
	const outcome = new Map<number, boolean>();

	const liveBook = (id: string) => {
		const b = d.books.get(id);
		return b && !b.deleted ? b : null;
	};

	/** The position rule (mau_core::progress). `unit`: the unit the device
	 *  saw; a move in the other unit loses, and the rest of its chain. */
	function move(op: Op, readingId: string, to: number, unit?: Unit): boolean {
		const r = d.readings.get(readingId);
		const c = chainOf.get(op.id);
		if (!r || r.deleted || !c) return false;
		if (unit && unit !== r.unit) {
			if (c.first) outcome.set(c.chain, false);
			return false;
		}
		if (!c.first) {
			if (outcome.get(c.chain) !== true) return false;
		} else {
			const current = r.pos_clock;
			if (c.tip <= current) {
				outcome.set(c.chain, false);
				return false;
			}
			outcome.set(c.chain, true);
			if (c.root < current) {
				for (const [id, e] of d.events)
					if (e.reading_id === readingId && e.hlc > c.root && !e.deleted)
						d.events.set(id, { ...e, deleted: true });
			}
		}
		d.readings.set(readingId, { ...d.readings.get(readingId)!, pos: to, pos_clock: op.hlc });
		return true;
	}

	for (const op of ops) {
		const k = op.op;
		const hlc = op.hlc;
		switch (k.type) {
			case 'work.create': {
				const w = k.work;
				if (d.works.has(w.id)) break;
				d.works.set(w.id, {
					id: w.id,
					title: w.title,
					first_year: w.first_year,
					cover: null,
					private: true,
					suggested: false,
					people: w.people,
					editions: w.edition
						? [
								{
									...w.edition,
									work_id: w.id,
									cover: null,
									private: true,
									binding: w.edition.binding ?? null
								}
							]
						: []
				});
				break;
			}
			case 'book.create': {
				if (d.books.has(k.id) || !d.works.has(k.work_id)) break;
				const fields = [
					'rank',
					'active',
					'rating',
					'notes',
					'title_override',
					'authors_override',
					'cover_override'
				];
				d.books.set(k.id, {
					id: k.id,
					work_id: k.work_id,
					rank: k.rank,
					active: false,
					rating: 0,
					notes: '',
					title_override: null,
					authors_override: null,
					cover_override: null,
					added_at: k.added_at,
					deleted: false,
					clock: Object.fromEntries(fields.map((f) => [f, hlc]))
				});
				break;
			}
			case 'book.set': {
				const b = liveBook(k.id);
				if (!b) break;
				const next = lww(b, k.fields as Partial<Record<keyof BookRow, unknown>>, hlc, (f, v) => {
					if (f === 'title_override' || f === 'cover_override') return v === '' ? null : v;
					if (f === 'authors_override') return (v as string[]).length ? v : null;
					return v;
				});
				if (next) d.books.set(k.id, next);
				break;
			}
			case 'book.delete': {
				const b = liveBook(k.id);
				if (!b) break;
				d.books.set(k.id, { ...b, deleted: true, notes: '', rating: 0 });
				for (const [id, r] of d.readings) {
					if (r.book_id !== k.id) continue;
					d.readings.set(id, { ...r, deleted: true });
					for (const [eid, e] of d.events)
						if (e.reading_id === id) d.events.set(eid, { ...e, deleted: true });
				}
				for (const [id, c] of d.copies)
					if (c.book_id === k.id) d.copies.set(id, { ...c, deleted: true });
				for (const [key, t] of d.tags)
					if (t.book_id === k.id) d.tags.set(key, { ...t, present: false });
				break;
			}
			case 'tag.set': {
				if (!liveBook(k.book_id)) break;
				const key = tagKey(k.book_id, k.tag);
				const t = d.tags.get(key);
				if (t && t.clock >= hlc) break;
				d.tags.set(key, { book_id: k.book_id, tag: k.tag, present: k.present, clock: hlc });
				break;
			}
			case 'copy.create': {
				if (d.copies.has(k.id) || !liveBook(k.book_id)) break;
				d.copies.set(k.id, {
					id: k.id,
					book_id: k.book_id,
					format: k.format,
					location: k.location,
					edition_id: k.edition_id,
					deleted: false,
					clock: { format: hlc, location: hlc, edition_id: hlc }
				});
				break;
			}
			case 'copy.set': {
				const c = d.copies.get(k.id);
				if (!c || c.deleted) break;
				const next = lww(c, k.fields as Partial<Record<keyof CopyRow, unknown>>, hlc, (f, v) =>
					f === 'edition_id' && v === '' ? null : v
				);
				if (next) d.copies.set(k.id, next);
				break;
			}
			case 'copy.delete': {
				const c = d.copies.get(k.id);
				if (c && !c.deleted) d.copies.set(k.id, { ...c, deleted: true });
				break;
			}
			case 'reading.create': {
				if (d.readings.has(k.id) || !liveBook(k.book_id)) break;
				const unit = k.unit ?? 'pages';
				d.readings.set(k.id, {
					id: k.id,
					book_id: k.book_id,
					started_on: k.started_on,
					finished_on: null,
					unit,
					total: k.total,
					pos: 0,
					pages: unit === 'percent' ? (k.pages ?? null) : null,
					pos_clock: hlc,
					deleted: false,
					clock: { total: hlc, finished_on: hlc, pages: hlc }
				});
				break;
			}
			case 'reading.set': {
				const r = d.readings.get(k.id);
				if (!r || r.deleted) break;
				// A total is for pages readings, a page count for percent ones.
				const { total, pages, ...rest } = k.fields;
				const fields: Record<string, unknown> = { ...rest };
				if (r.unit === 'pages') fields.total = total;
				else fields.pages = pages;
				const next = lww(r, fields as Partial<Record<keyof ReadingRow, unknown>>, hlc, (f, v) =>
					(f === 'finished_on' && v === '') || (f === 'pages' && v === 0) ? null : v
				);
				if (next) d.readings.set(k.id, next);
				break;
			}
			case 'reading.delete': {
				const r = d.readings.get(k.id);
				if (!r || r.deleted) break;
				d.readings.set(k.id, { ...r, deleted: true });
				for (const [eid, e] of d.events)
					if (e.reading_id === k.id) d.events.set(eid, { ...e, deleted: true });
				break;
			}
			case 'progress': {
				if (!move(op, k.reading_id, k.to, k.unit ?? 'pages')) break;
				d.events.set(k.event_id, {
					id: k.event_id,
					reading_id: k.reading_id,
					hlc,
					local_date: k.local_date,
					from_page: k.from,
					to_page: k.to,
					unit: k.unit ?? 'pages',
					deleted: false
				});
				break;
			}
			case 'progress.undo': {
				if (!move(op, k.reading_id, k.to, k.unit ?? 'pages')) break;
				const e = d.events.get(k.event_id);
				if (e) d.events.set(k.event_id, { ...e, deleted: true });
				break;
			}
			case 'progress.unit': {
				const r = d.readings.get(k.reading_id);
				if (!r || !move(op, k.reading_id, k.to)) break;
				// To percent keeps a known page count; to pages makes it the total.
				const pages = k.unit === 'pages' ? null : r.unit === 'pages' ? r.total : r.pages;
				d.readings.set(k.reading_id, {
					...d.readings.get(k.reading_id)!,
					unit: k.unit,
					total: k.total,
					pages,
					clock: { ...r.clock, total: hlc, pages: hlc }
				});
				break;
			}
		}
	}
	return d;
}
