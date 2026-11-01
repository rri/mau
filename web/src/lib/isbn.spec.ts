import { describe, expect, it } from 'vitest';
import { looksLikeIsbn, normalizeIsbn } from './isbn.ts';

describe('isbn', () => {
	it('matches the Rust implementation', () => {
		expect(normalizeIsbn('978-0-306-40615-7')).toBe('9780306406157');
		expect(normalizeIsbn('9780306406158')).toBeNull();
		expect(normalizeIsbn('0-306-40615-2')).toBe('9780306406157');
		expect(normalizeIsbn('080442957X')).toBe('9780804429573');
		expect(looksLikeIsbn('earthsea')).toBe(false);
		expect(looksLikeIsbn('080442957x')).toBe(true);
	});
});
