import { describe, expect, it } from 'vitest';
import { between } from './rank.ts';

describe('rank', () => {
	it('matches the Rust implementation', () => {
		// Same cases as crates/mau-core/src/rank.rs.
		expect(between(null, null)).toBe('V');
		expect(between('V', null)).toBe('l');
		expect(between(null, 'V')).toBe('G');
		expect(between('V', 'W')).toBe('VV');
		expect(between('z', null)).toBe('zV');
		expect(between(null, '1')).toBe('0V');
	});

	it('always fits a new rank between two', () => {
		const ranks = [between(null, null)];
		for (let i = 0; i < 300; i++) {
			const r =
				i % 3 === 0
					? between(null, ranks[0])
					: i % 3 === 1
						? between(ranks[ranks.length - 1], null)
						: between(ranks[(ranks.length >> 1) - 1], ranks[ranks.length >> 1]);
			expect(r.endsWith('0')).toBe(false);
			ranks.push(r);
			ranks.sort();
			expect(new Set(ranks).size).toBe(ranks.length);
		}
	});
});
