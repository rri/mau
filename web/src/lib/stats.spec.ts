import { describe, expect, it } from 'vitest';
import type { Data } from './sync/apply.ts';
import { daysLeft, lastDays, pagesPerDay, yearTotals } from './stats.ts';

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
	const ev = (
		id: string,
		reading: string,
		date: string,
		from: number,
		to: number,
		unit: 'pages' | 'percent' = 'pages'
	) =>
		d.events.set(id, {
			id,
			reading_id: reading,
			local_date: date,
			from_page: from,
			to_page: to,
			unit,
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

	it('lists recent days oldest first', () => {
		const days = lastDays(pagesPerDay(library()), '2026-11-10', 3);
		expect(days).toEqual([
			{ date: '2026-11-08', pages: 40 },
			{ date: '2026-11-09', pages: 30 },
			{ date: '2026-11-10', pages: 40 }
		]);
	});

	it('totals a year', () => {
		const d = library();
		const p = pagesPerDay(d);
		expect(yearTotals(d, p, 2026)).toEqual({ finished: 1, pages: 410, days: 4 });
	});

	it('estimates days left from the last four weeks', () => {
		const d = library();
		const r2 = { id: 'r2', pos: 100, total: 400, unit: 'pages' as const };
		// 110 pages in 28 days ≈ 3.9 a day; 300 left → 77 days.
		expect(daysLeft(d, r2, '2026-11-10')).toBe(77);
		expect(daysLeft(d, r2, '2027-02-01')).toBeNull();
	});

	it('percent readings count as days read and pace in percent, but add no pages', () => {
		const d = library();
		d.readings.set('r4', {
			id: 'r4',
			book_id: 'b1',
			deleted: false,
			finished_on: null,
			unit: 'percent',
			pos: 20,
			total: 100
		} as never);
		d.events.set('p1', {
			id: 'p1',
			reading_id: 'r4',
			local_date: '2026-11-11',
			from_page: 0,
			to_page: 20,
			unit: 'percent',
			deleted: false
		} as never);
		const p = pagesPerDay(d);
		expect(p.get('2026-11-11')).toBeUndefined();
		expect(yearTotals(d, p, 2026)).toEqual({ finished: 1, pages: 410, days: 5 });
		// 20% in 28 days: 80% left → 112 days.
		const r4 = { id: 'r4', pos: 20, total: 100, unit: 'percent' as const };
		expect(daysLeft(d, r4, '2026-11-11')).toBe(112);
	});
});
