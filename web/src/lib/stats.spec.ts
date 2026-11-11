import { describe, expect, it } from 'vitest';
import type { Data } from './sync/apply.ts';
import { byMonth, daysLeft, lastDays, pagesPerDay, streak, yearTotals } from './stats.ts';

// A tiny library: one book, two readings (one finished), a few progress events.
function library(): Data {
	const d = {
		books: new Map(),
		tags: new Map(),
		copies: new Map(),
		readings: new Map(),
		events: new Map(),
		works: new Map()
	} as unknown as Data;
	d.books.set('b1', { id: 'b1', deleted: false } as never);
	d.books.set('b2', { id: 'b2', deleted: true } as never);
	d.readings.set('r1', {
		id: 'r1',
		book_id: 'b1',
		deleted: false,
		finished_on: '2026-03-04',
		pos: 300,
		total: 300
	} as never);
	d.readings.set('r2', {
		id: 'r2',
		book_id: 'b1',
		deleted: false,
		finished_on: null,
		pos: 100,
		total: 400
	} as never);
	d.readings.set('r3', {
		id: 'r3',
		book_id: 'b2',
		deleted: false,
		finished_on: '2026-03-05',
		pos: 0,
		total: 9
	} as never);
	const ev = (id: string, reading: string, date: string, from: number, to: number) =>
		d.events.set(id, {
			id,
			reading_id: reading,
			local_date: date,
			from_page: from,
			to_page: to,
			deleted: false
		} as never);
	ev('e1', 'r2', '2026-11-08', 0, 40);
	ev('e2', 'r2', '2026-11-09', 40, 70);
	ev('e3', 'r2', '2026-11-09', 70, 60); // a rewind doesn't count
	ev('e4', 'r2', '2026-11-10', 60, 100);
	ev('e5', 'r3', '2026-11-10', 0, 9); // deleted book doesn't count
	ev('e6', 'r1', '2026-03-01', 0, 300);
	return d;
}

describe('stats', () => {
	it('counts forward pages per day for live books only', () => {
		const p = pagesPerDay(library());
		expect(p.get('2026-11-09')).toBe(30);
		expect(p.get('2026-11-10')).toBe(40);
		expect(p.get('2026-03-01')).toBe(300);
	});

	it('keeps a streak alive until the day is over', () => {
		const p = pagesPerDay(library());
		expect(streak(p, '2026-11-10')).toEqual({ days: 3, readToday: true });
		expect(streak(p, '2026-11-11')).toEqual({ days: 3, readToday: false });
		expect(streak(p, '2026-11-12')).toEqual({ days: 0, readToday: false });
	});

	it('lists recent days oldest first', () => {
		const days = lastDays(pagesPerDay(library()), '2026-11-10', 3);
		expect(days).toEqual([
			{ date: '2026-11-08', pages: 40 },
			{ date: '2026-11-09', pages: 30 },
			{ date: '2026-11-10', pages: 40 }
		]);
	});

	it('totals a year and splits it by month', () => {
		const d = library();
		const p = pagesPerDay(d);
		expect(yearTotals(d, p, 2026)).toEqual({
			finished: 1,
			pages: 410,
			days: 4,
			perReadingDay: 103
		});
		const m = byMonth(d, p, 2026);
		expect(m[2]).toEqual({ month: 2, pages: 300, finished: 1 });
		expect(m[10]).toEqual({ month: 10, pages: 110, finished: 0 });
	});

	it('estimates days left from the last four weeks', () => {
		const d = library();
		const r2 = { id: 'r2', pos: 100, total: 400 };
		// 110 pages in 28 days ≈ 3.9 a day; 300 left → 77 days.
		expect(daysLeft(d, r2, '2026-11-10')).toBe(77);
		expect(daysLeft(d, r2, '2027-02-01')).toBeNull();
	});
});
