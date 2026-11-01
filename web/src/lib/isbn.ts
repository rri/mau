// ISBN handling, matching crates/mau-core/src/isbn.rs.

const strip = (s: string) => s.replace(/[\s-]/g, '').toUpperCase();

export function looksLikeIsbn(s: string): boolean {
	const d = strip(s);
	return /^\d{13}$/.test(d) || /^\d{9}[\dX]$/.test(d);
}

function valid13(d: string): boolean {
	let sum = 0;
	for (let i = 0; i < 12; i++) sum += Number(d[i]) * (i % 2 ? 3 : 1);
	return (10 - (sum % 10)) % 10 === Number(d[12]);
}

function valid10(d: string): boolean {
	let sum = 0;
	for (let i = 0; i < 10; i++) sum += (d[i] === 'X' ? 10 : Number(d[i])) * (10 - i);
	return sum % 11 === 0;
}

function to13(d10: string): string {
	const base = '978' + d10.slice(0, 9);
	let sum = 0;
	for (let i = 0; i < 12; i++) sum += Number(base[i]) * (i % 2 ? 3 : 1);
	return base + ((10 - (sum % 10)) % 10);
}

/** The ISBN-13 for a valid ISBN-10 or ISBN-13, else null. */
export function normalizeIsbn(s: string): string | null {
	const d = strip(s);
	if (/^\d{13}$/.test(d)) return valid13(d) ? d : null;
	if (/^\d{9}[\dX]$/.test(d)) return valid10(d) ? to13(d) : null;
	return null;
}
