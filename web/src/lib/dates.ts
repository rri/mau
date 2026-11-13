// Calendar dates in the user's time zone, as YYYY-MM-DD strings.

export function todayIn(timeZone: string, at: Date = new Date()): string {
	try {
		return new Intl.DateTimeFormat('en-CA', {
			timeZone,
			year: 'numeric',
			month: '2-digit',
			day: '2-digit'
		}).format(at);
	} catch {
		return new Intl.DateTimeFormat('en-CA').format(at);
	}
}

/** Days between two YYYY-MM-DD dates (b − a). */
export function daysBetween(a: string, b: string): number {
	return Math.round((Date.parse(b + 'T00:00:00Z') - Date.parse(a + 'T00:00:00Z')) / 86_400_000);
}

export function addDays(d: string, n: number): string {
	return new Date(Date.parse(d + 'T00:00:00Z') + n * 86_400_000).toISOString().slice(0, 10);
}

/** "today", "yesterday", "3 days ago", "2 weeks ago", "4 months ago". */
export function ago(date: string, today: string): string {
	if (date === UNKNOWN_DATE) return 'a while ago';
	const d = daysBetween(date, today);
	if (d <= 0) return 'today';
	if (d === 1) return 'yesterday';
	if (d < 14) return `${d} days ago`;
	if (d < 60) return `${Math.round(d / 7)} weeks ago`;
	if (d < 730) return `${Math.round(d / 30)} months ago`;
	return `${Math.round(d / 365)} years ago`;
}

const monthDay = new Intl.DateTimeFormat('en-US', {
	month: 'short',
	day: 'numeric',
	timeZone: 'UTC'
});
const full = new Intl.DateTimeFormat('en-US', {
	month: 'short',
	day: 'numeric',
	year: 'numeric',
	timeZone: 'UTC'
});
const monthYear = new Intl.DateTimeFormat('en-US', {
	month: 'short',
	year: 'numeric',
	timeZone: 'UTC'
});

/** Imported reads without a date are stored with this placeholder. */
export const UNKNOWN_DATE = '0000-01-01';

export const fmtDate = (d: string) =>
	d === UNKNOWN_DATE ? 'date unknown' : full.format(new Date(d + 'T00:00:00Z'));
/** When a reading happened, for its heading on the book page: "Oct 1, 2026 –
 *  Oct 28, 2026", "Oct 1, 2026 – now", "Finished Oct 28, 2026" (start
 *  unknown), "Started Oct 1, 2026" (finished, date unknown), or just
 *  "Finished" (finished, neither date known). Imports often know neither. */
export function readingSpan(started: string, finished: string | null): string {
	const s = started === UNKNOWN_DATE ? null : fmtDate(started);
	if (finished === null) return s ? `${s} – now` : 'Started, date unknown';
	const f = finished === UNKNOWN_DATE ? null : fmtDate(finished);
	if (s && f) return `${s} – ${f}`;
	if (f) return `Finished ${f}`;
	if (s) return `Started ${s}`;
	return 'Finished';
}

export const fmtMonthDay = (d: string) => monthDay.format(new Date(d + 'T00:00:00Z'));
export const fmtMonthYear = (d: string) =>
	d === UNKNOWN_DATE ? 'date unknown' : monthYear.format(new Date(d + 'T00:00:00Z'));

/** The local date of an epoch-ms instant in a time zone. */
export function dateOfMs(ms: number, timeZone: string): string {
	return todayIn(timeZone, new Date(ms));
}
