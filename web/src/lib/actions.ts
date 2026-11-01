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

/** Moves the current reading to page `to` (and optionally a new total).
 *  Reaching the total finishes the book. Shows an undo toast. */
export function setPages(info: BookInfo, to: number, total?: number, message?: string) {
	const r = info.reading;
	if (!r || r.finished_on) return;
	const newTotal = total ?? r.total;
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
			local_date: today()
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
				to: r.pos
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
		if (to !== r.pos) parts.push(`${to < r.pos ? 'Moved back to' : 'Moved to'} page ${to}`);
		if (newTotal !== r.total) parts.push(`total ${r.total} → ${newTotal}`);
		text = parts.join(', ');
	}
	toasts.undo(text, undo);
}

export function plusOne(info: BookInfo) {
	const r = info.reading;
	if (!r) return;
	setPages(info, r.pos + 1, undefined, `+1 page on ${info.title}`);
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
	const at = info.reading ? `at page ${info.reading.pos}` : '';
	toasts.undo(`Resumed ${info.title} ${at}`.trim(), () =>
		library.commit([{ type: 'book.set', id: info.book.id, fields: before }])
	);
}

/** Starts a first reading (after setup) or a re-read, and activates the book. */
export function startReading(info: BookInfo, total: number, rank?: string) {
	const readingId = uuidv7();
	const before = { active: info.book.active, rank: info.book.rank };
	const reread = info.state === 'finished';
	library.commit([
		{ type: 'reading.create', id: readingId, book_id: info.book.id, started_on: today(), total },
		{
			type: 'book.set',
			id: info.book.id,
			fields: { active: true, rank: rank ?? endOfActive(info.book.id) }
		}
	]);
	toasts.undo(reread ? `Started re-reading ${info.title} at page 0` : `Started ${info.title}`, () =>
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
