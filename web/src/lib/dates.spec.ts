import { describe, expect, it } from 'vitest';
import {
	addDays,
	ago,
	daysBetween,
	readingSpan,
	fmtDate,
	fmtMonthYear,
	todayIn,
	UNKNOWN_DATE
} from './dates.ts';

describe('dates', () => {
	it('DATE-01 today is the calendar date in the reader’s time zone', () => {
		const at = new Date('2026-11-11T23:30:00Z');
		expect(todayIn('UTC', at)).toBe('2026-11-11');
		expect(todayIn('Europe/Berlin', at)).toBe('2026-11-12');
		expect(todayIn('America/Los_Angeles', at)).toBe('2026-11-11');
		expect(todayIn('Not/AZone', at)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
	});

	it('DATE-02 day arithmetic crosses months and years', () => {
		expect(addDays('2026-12-31', 1)).toBe('2027-01-01');
		expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
		expect(daysBetween('2026-10-31', '2026-11-11')).toBe(11);
	});

	it('DATE-03 relative days read naturally', () => {
		const t = '2026-11-11';
		expect(ago(t, t)).toBe('today');
		expect(ago('2026-11-10', t)).toBe('yesterday');
		expect(ago('2026-11-01', t)).toBe('10 days ago');
		expect(ago('2026-10-14', t)).toBe('4 weeks ago');
		expect(ago('2026-05-11', t)).toBe('6 months ago');
		expect(ago('2023-11-11', t)).toBe('3 years ago');
		expect(ago(UNKNOWN_DATE, t)).toBe('a while ago');
	});

	it('DATE-04 imported reads without a date say so', () => {
		expect(fmtDate('2026-10-28')).toBe('Oct 28, 2026');
		expect(fmtDate(UNKNOWN_DATE)).toBe('date unknown');
		expect(fmtMonthYear(UNKNOWN_DATE)).toBe('date unknown');
	});

	it('BOOK-17 a reading’s span says what’s known', () => {
		const U = '0000-01-01';
		expect(readingSpan('2026-10-01', '2026-10-28')).toBe('Oct 1, 2026 – Oct 28, 2026');
		expect(readingSpan('2026-10-01', null)).toBe('Oct 1, 2026 – now');
		expect(readingSpan(U, '2026-10-28')).toBe('Finished Oct 28, 2026');
		expect(readingSpan('2026-10-01', U)).toBe('Started Oct 1, 2026');
		expect(readingSpan(U, U)).toBe('Finished');
		expect(readingSpan(U, null)).toBe('Started, date unknown');
	});
});
