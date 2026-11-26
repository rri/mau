import { describe, expect, it } from 'vitest';
import { safeNext } from './next.ts';

describe('safeNext', () => {
	it('AUTH-08 keeps paths on this site and sends anything else Home', () => {
		expect(safeNext('/authorize/abc')).toBe('/authorize/abc');
		expect(safeNext('/catalog?x=1#y')).toBe('/catalog?x=1#y');
		for (const bad of [
			null,
			'',
			'https://evil.example/',
			'//evil.example/',
			'/\\evil.example',
			'javascript:alert(1)',
			'/\t/evil.example'
		])
			expect(safeNext(bad)).toBe('/');
	});
});
