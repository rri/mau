// What the reading controls do, as sync operations, with their undo.
// Ordering rules (spec §4): play → end of active books; pause → top of
// inactive books; drag rules in `moveBook`.

import type { OpKind } from './api/generated/OpKind.ts';
import { auth } from './auth.svelte.ts';
import { todayIn } from './dates.ts';
import { bookInfo, readingList, type BookInfo } from './derive.ts';
import { library } from './library.svelte.ts';
import { between } from './sync/rank.ts';
import { uuidv7 } from './sync/ids.ts';
import { toasts } from './toast.svelte.ts';

export const today = () => todayIn(auth.me?.time_zone ?? 'UTC');

/** The current state of a book (after other changes may have happened). */
function fresh(bookId: string): BookInfo | null {
	const b = library.view.books.get(bookId);
	return b && !b.deleted ? bookInfo(library.view, b) : null;
}

/** Rank for the end of the active books. */
function endOfActive(exceptId: string): string {
	const active = readingList(library.view).filter(
		(b) => b.state === 'active' && b.book.id !== exceptId
	);
	return between(active.at(-1)?.book.rank ?? null, null);
}

/** Rank for the top of the inactive books. */
export function topOfInactive(exceptId?: string): string {
	const inactive = readingList(library.view).filter(
		(b) => b.state === 'inactive' && b.book.id !== exceptId
	);
	return between(null, inactive[0]?.book.rank ?? null);
}

/** Moves the current reading to `to` (a page, or a percent for a reading
 *  tracked by percent), and optionally a new page total (pages readings
 *  only). Reaching the total finishes the book. Shows an undo toast. */
export function setPages(info: BookInfo, to: number, total?: number, message?: string) {
	const r = info.reading;
	if (!r || r.finished_on) return;
	const percent = r.unit === 'percent';
	const newTotal = percent ? 100 : (total ?? r.total);
	to = Math.max(0, Math.min(to, newTotal));
	const ops: OpKind[] = [];
	const eventId = uuidv7();
	if (newTotal !== r.total)
		ops.push({ type: 'reading.set', id: r.id, fields: { total: newTotal } });
	if (to !== r.pos)
		ops.push({
			type: 'progress',
			event_id: eventId,
			reading_id: r.id,
			base: r.pos_clock,
			from: r.pos,
			to,
			local_date: today(),
			unit: r.unit
		});
	const finishing = to >= newTotal;
	if (finishing) {
		ops.push({ type: 'reading.set', id: r.id, fields: { finished_on: today() } });
		ops.push({ type: 'book.set', id: info.book.id, fields: { active: false } });
	}
	if (!ops.length) return;
	const wasActive = info.book.active;
	library.commit(ops);

	const undo = () => {
		const now = fresh(info.book.id);
		const nr = now?.readings.find((x) => x.id === r.id);
		if (!now || !nr) return;
		const back: OpKind[] = [];
		if (to !== r.pos)
			back.push({
				type: 'progress.undo',
				reading_id: r.id,
				event_id: eventId,
				base: nr.pos_clock,
				to: r.pos,
				unit: r.unit
			});
		if (newTotal !== r.total)
			back.push({ type: 'reading.set', id: r.id, fields: { total: r.total } });
		if (finishing) {
			back.push({ type: 'reading.set', id: r.id, fields: { finished_on: '' } });
			back.push({ type: 'book.set', id: info.book.id, fields: { active: wasActive } });
		}
		library.commit(back);
	};

	let text = message;
	if (finishing) text = `Finished ${info.title}`;
	else if (!text) {
		const parts: string[] = [];
		const where = percent ? `${to}%` : `page ${to}`;
		if (to !== r.pos) parts.push(`${to < r.pos ? 'Moved back to' : 'Moved to'} ${where}`);
		if (newTotal !== r.total) parts.push(`total ${r.total} → ${newTotal}`);
		text = parts.join(', ');
	}
	toasts.undo(text, undo);
}

/** One step up: a page, or 1% for a reading tracked by percent. */
export function plusOne(info: BookInfo) {
	const r = info.reading;
	if (!r) return;
	const step = r.unit === 'percent' ? '1%' : '1 page';
	setPages(info, r.pos + 1, undefined, `+${step} on ${info.title}`);
}

/** Switches the current reading between pages and percent. To percent, the
 *  position becomes the share read (the page count is kept); to pages, it's
 *  `page` of `total`. Logs no reading. Shows an undo toast. */
export function switchUnit(info: BookInfo, to: 'percent'): void;
export function switchUnit(info: BookInfo, to: 'pages', total: number, page: number): void;
export function switchUnit(info: BookInfo, to: 'pages' | 'percent', total = 0, page = 0) {
	const r = info.reading;
	if (!r || r.finished_on || r.unit === to) return;
	const before = { unit: r.unit, total: r.total, to: r.pos };
	const next =
		to === 'percent'
			? { unit: to, total: 100, to: Math.min(100, Math.round((100 * r.pos) / r.total)) }
			: { unit: to, total, to: Math.max(0, Math.min(page, total)) };
	library.commit([{ type: 'progress.unit', reading_id: r.id, base: r.pos_clock, ...next }]);
	toasts.undo(
		to === 'percent'
			? `${info.title} now counts by percent, at ${next.to}%`
			: `${info.title} now counts by pages, at page ${next.to} of ${total}`,
		() => {
			const nr = fresh(info.book.id)?.readings.find((x) => x.id === r.id);
			if (!nr) return;
			library.commit([{ type: 'progress.unit', reading_id: r.id, base: nr.pos_clock, ...before }]);
			// Back to percent from pages, the page count is the one entered.
			if (before.unit === 'percent')
				library.commit([{ type: 'reading.set', id: r.id, fields: { pages: r.pages ?? 0 } }]);
		}
	);
}

/** A percent reading's page count (0: unknown). */
export function setPageCount(info: BookInfo, pages: number) {
	const r = info.reading;
	if (!r || r.unit !== 'percent' || (r.pages ?? 0) === pages) return;
	const before = r.pages ?? 0;
	library.commit([{ type: 'reading.set', id: r.id, fields: { pages } }]);
	toasts.undo(pages ? `Your copy has ${pages} pages` : 'Page count removed', () =>
		library.commit([{ type: 'reading.set', id: r.id, fields: { pages: before } }])
	);
}

/** Marks a book finished today, from Active or Inactive. A book never
 *  started gets a reading that's finished today (in pages when the catalog
 *  knows the count, else in percent). The position stays where it was, so
 *  nothing counts as read today. Shows an undo toast. */
export function finish(info: BookInfo) {
	if (info.state === 'finished') return;
	const wasActive = info.book.active;
	const r = info.reading;
	const day = today();
	const ops: OpKind[] = [];
	let readingId: string;
	if (r) {
		readingId = r.id;
	} else {
		readingId = uuidv7();
		ops.push(
			info.catalogPages
				? {
						type: 'reading.create',
						id: readingId,
						book_id: info.book.id,
						started_on: day,
						total: info.catalogPages
					}
				: {
						type: 'reading.create',
						id: readingId,
						book_id: info.book.id,
						started_on: day,
						total: 100,
						unit: 'percent'
					}
		);
	}
	ops.push({ type: 'reading.set', id: readingId, fields: { finished_on: day } });
	if (wasActive) ops.push({ type: 'book.set', id: info.book.id, fields: { active: false } });
	library.commit(ops);
	toasts.undo(`Finished ${info.title}`, () => {
		const back: OpKind[] = r
			? [{ type: 'reading.set', id: readingId, fields: { finished_on: '' } }]
			: [{ type: 'reading.delete', id: readingId }];
		if (wasActive) back.push({ type: 'book.set', id: info.book.id, fields: { active: true } });
		library.commit(back);
	});
}

export function pause(info: BookInfo) {
	const before = { active: info.book.active, rank: info.book.rank };
	library.commit([
		{
			type: 'book.set',
			id: info.book.id,
			fields: { active: false, rank: topOfInactive(info.book.id) }
		}
	]);
	toasts.undo(`Paused ${info.title}`, () =>
		library.commit([{ type: 'book.set', id: info.book.id, fields: before }])
	);
}

/** Activates a book that already has an unfinished reading, at `rank` (default: end of active). */
export function resume(info: BookInfo, rank?: string) {
	const before = { active: info.book.active, rank: info.book.rank };
	library.commit([
		{
			type: 'book.set',
			id: info.book.id,
			fields: { active: true, rank: rank ?? endOfActive(info.book.id) }
		}
	]);
	const r = info.reading;
	const at = r ? `at ${r.unit === 'percent' ? `${r.pos}%` : `page ${r.pos}`}` : '';
	toasts.undo(`Resumed ${info.title} ${at}`.trim(), () =>
		library.commit([{ type: 'book.set', id: info.book.id, fields: before }])
	);
}

/** How a reading counts: pages of a total, or percent (with the copy's page
 *  count if known). */
export type Start = { unit: 'pages'; total: number } | { unit: 'percent'; pages?: number | null };

/** The same way as the book's last reading (for a re-read). */
export function startLike(info: BookInfo): Start {
	const r = info.reading;
	if (r?.unit === 'percent') return { unit: 'percent', pages: r.pages };
	return { unit: 'pages', total: r?.total ?? info.catalogPages ?? 1 };
}

/** Starts a first reading (after setup) or a re-read, and activates the book. */
export function startReading(info: BookInfo, start: Start, rank?: string) {
	const readingId = uuidv7();
	const before = { active: info.book.active, rank: info.book.rank };
	const reread = info.state === 'finished';
	const create: OpKind =
		start.unit === 'pages'
			? {
					type: 'reading.create',
					id: readingId,
					book_id: info.book.id,
					started_on: today(),
					total: start.total
				}
			: {
					type: 'reading.create',
					id: readingId,
					book_id: info.book.id,
					started_on: today(),
					total: 100,
					unit: 'percent',
					pages: start.pages ?? undefined
				};
	library.commit([
		create,
		{
			type: 'book.set',
			id: info.book.id,
			fields: { active: true, rank: rank ?? endOfActive(info.book.id) }
		}
	]);
	const at = start.unit === 'percent' ? '0%' : 'page 0';
	toasts.undo(reread ? `Started re-reading ${info.title} at ${at}` : `Started ${info.title}`, () =>
		library.commit([
			{ type: 'reading.delete', id: readingId },
			{ type: 'book.set', id: info.book.id, fields: before }
		])
	);
}

/** Moves a book in the Reading list. `index` is the drop position among the
 *  other visible books (0 = top). Applies the drag rules:
 *  - inactive book dropped between active books: activates there;
 *  - active book dropped among inactive books: stays active, goes to end of active;
 *  - the edge between the groups changes nobody's state. */
export function moveBook(info: BookInfo, index: number, onNeedsSetup: (rank: string) => void) {
	const others = readingList(library.view).filter((b) => b.book.id !== info.book.id);
	const active = others.filter((b) => b.state === 'active');
	const nActive = active.length;
	if (info.state === 'active') {
		const at = Math.min(index, nActive);
		const rank = between(active[at - 1]?.book.rank ?? null, active[at]?.book.rank ?? null);
		if (rank === info.book.rank) return;
		const before = info.book.rank;
		library.commit([{ type: 'book.set', id: info.book.id, fields: { rank } }]);
		toasts.undo(
			index > nActive
				? `${info.title} is still active, moved to the end of the active list`
				: `Moved ${info.title}`,
			() => library.commit([{ type: 'book.set', id: info.book.id, fields: { rank: before } }])
		);
		return;
	}
	if (index < nActive) {
		const rank = between(active[index - 1]?.book.rank ?? null, active[index].book.rank);
		if (!info.reading) onNeedsSetup(rank);
		else resume(info, rank);
		return;
	}
	const inactive = others.filter((b) => b.state === 'inactive');
	const j = index - nActive;
	const rank = between(inactive[j - 1]?.book.rank ?? null, inactive[j]?.book.rank ?? null);
	const before = info.book.rank;
	library.commit([{ type: 'book.set', id: info.book.id, fields: { rank } }]);
	toasts.undo(`Moved ${info.title}`, () =>
		library.commit([{ type: 'book.set', id: info.book.id, fields: { rank: before } }])
	);
}
