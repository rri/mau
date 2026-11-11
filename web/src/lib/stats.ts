// Reading statistics for the home page, all derived from the synced library
// (so they work offline). Pages count forward moves only, as everywhere else.

import type { Data } from './sync/apply.ts';
import { addDays, daysBetween, UNKNOWN_DATE } from './dates.ts';

/** Pages read per local date, across every live book. */
export function pagesPerDay(d: Data): Map<string, number> {
	const liveReadings = new Set<string>();
	for (const r of d.readings.values()) {
		const book = d.books.get(r.book_id);
		if (!r.deleted && book && !book.deleted) liveReadings.add(r.id);
	}
	const out = new Map<string, number>();
	for (const e of d.events.values()) {
		if (e.deleted || !liveReadings.has(e.reading_id) || e.to_page <= e.from_page) continue;
		out.set(e.local_date, (out.get(e.local_date) ?? 0) + (e.to_page - e.from_page));
	}
	return out;
}

/** Consecutive reading days ending today, or ending yesterday if today has
 *  no reading yet (the streak isn't broken until the day is over). */
export function streak(perDay: Map<string, number>, today: string) {
	const readToday = (perDay.get(today) ?? 0) > 0;
	let day = readToday ? today : addDays(today, -1);
	let days = 0;
	while ((perDay.get(day) ?? 0) > 0) {
		days++;
		day = addDays(day, -1);
	}
	return { days, readToday };
}

/** The last `n` days, oldest first. */
export function lastDays(perDay: Map<string, number>, today: string, n = 14) {
	return Array.from({ length: n }, (_, i) => {
		const date = addDays(today, i - (n - 1));
		return { date, pages: perDay.get(date) ?? 0 };
	});
}

/** Readings finished in `year` (unknown finish dates excluded). */
function finishedIn(d: Data, year: number): string[] {
	const out: string[] = [];
	for (const r of d.readings.values()) {
		const book = d.books.get(r.book_id);
		if (r.deleted || !book || book.deleted || !r.finished_on || r.finished_on === UNKNOWN_DATE)
			continue;
		if (r.finished_on.startsWith(`${year}-`)) out.push(r.finished_on);
	}
	return out;
}

/** Totals for one calendar year. */
export function yearTotals(d: Data, perDay: Map<string, number>, year: number) {
	let pages = 0;
	let days = 0;
	for (const [date, p] of perDay)
		if (date.startsWith(`${year}-`) && p > 0) {
			pages += p;
			days++;
		}
	return {
		finished: finishedIn(d, year).length,
		pages,
		days,
		perReadingDay: days ? Math.round(pages / days) : 0
	};
}

/** Pages and finished books for each month of `year`. */
export function byMonth(d: Data, perDay: Map<string, number>, year: number) {
	const months = Array.from({ length: 12 }, (_, m) => ({ month: m, pages: 0, finished: 0 }));
	for (const [date, p] of perDay)
		if (date.startsWith(`${year}-`)) months[Number(date.slice(5, 7)) - 1].pages += p;
	for (const f of finishedIn(d, year)) months[Number(f.slice(5, 7)) - 1].finished++;
	return months;
}

/** Days left for a reading at its pace over the last four weeks, or null when
 *  there's no recent reading to go by. */
export function daysLeft(
	d: Data,
	reading: { id: string; pos: number; total: number },
	today: string,
	window = 28
): number | null {
	const left = reading.total - reading.pos;
	if (left <= 0) return 0;
	let pages = 0;
	for (const e of d.events.values()) {
		if (e.deleted || e.reading_id !== reading.id || e.to_page <= e.from_page) continue;
		const age = daysBetween(e.local_date, today);
		if (age >= 0 && age < window) pages += e.to_page - e.from_page;
	}
	if (pages === 0) return null;
	return Math.max(1, Math.ceil(left / (pages / window)));
}
