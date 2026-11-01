import { describe, expect, it } from 'vitest';
import { surname, titleKey } from './sorting.ts';

describe('sorting', () => {
	it('ignores leading articles in titles', () => {
		expect(titleKey('The Dispossessed')).toBe('dispossessed');
		expect(titleKey('A Wizard of Earthsea')).toBe('wizard of earthsea');
		expect(titleKey('Anathem')).toBe('anathem');
	});
	it('finds surnames with particles', () => {
		expect(surname('Ursula K. Le Guin')).toBe('Le Guin');
		expect(surname('Emily St. John Mandel')).toBe('Mandel');
		expect(surname('Ludwig van Beethoven')).toBe('van Beethoven');
		expect(surname('Plato')).toBe('Plato');
	});
});
