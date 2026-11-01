// Fractional-index ranks for manual ordering; same algorithm as
// crates/mau-core/src/rank.rs.

const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
const BASE = 62;

function midpoint(a: string, b: string | null): string {
	if (b !== null) {
		let n = 0;
		while (n < b.length && (a[n] ?? '0') === b[n]) n++;
		if (n > 0) return b.slice(0, n) + midpoint(a.slice(n), b.slice(n));
	}
	const da = a ? DIGITS.indexOf(a[0]) : 0;
	const db = b !== null ? (b ? DIGITS.indexOf(b[0]) : BASE) : BASE;
	if (db - da > 1) return DIGITS[Math.ceil((da + db) / 2)];
	if (b !== null && b.length > 1) return b[0];
	return DIGITS[da] + midpoint(a.slice(1), null);
}

/** A rank strictly between `a` and `b` (`null` = unbounded). */
export function between(a: string | null, b: string | null): string {
	if (a !== null && b !== null && !(a < b))
		throw new Error(`rank.between(${a}, ${b}): out of order`);
	return midpoint(a ?? '', b);
}
