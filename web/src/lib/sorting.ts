// Sort keys for the catalog.

const ARTICLE = /^(the|a|an)\s+/i;
const PARTICLES = new Set([
	'le',
	'la',
	'de',
	'del',
	'della',
	'des',
	'du',
	'da',
	'di',
	'van',
	'von',
	'der',
	'den',
	'ten',
	'ter',
	'st.',
	'st',
	'mac',
	'bin',
	'al'
]);

/** Title without a leading "The", "A", or "An", for sorting. */
export function titleKey(title: string): string {
	return title.replace(ARTICLE, '').toLocaleLowerCase();
}

/** Surname for sorting: the last word, plus particles before it
 *  ("Ursula K. Le Guin" → "Le Guin", "Ludwig van Beethoven" → "van Beethoven"). */
export function surname(name: string): string {
	const words = name.trim().split(/\s+/);
	let i = words.length - 1;
	while (i > 0 && PARTICLES.has(words[i - 1].toLowerCase())) i--;
	return words.slice(i).join(' ');
}

export function authorKey(name: string | undefined): string {
	return name ? surname(name).toLocaleLowerCase() : '￿';
}
