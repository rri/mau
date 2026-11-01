import { describe, expect, it } from 'vitest';
import type { Op } from '../api/generated/Op.ts';
import { applyOps, cloneData, emptyData } from './apply.ts';

const h = (ms: number, node: string) => `${String(ms).padStart(13, '0')}-0000-${node}`;
const T0 = h(1000, 'srv');

function base() {
	const d = emptyData();
	d.works.set('w', { id: 'w', title: 'W', first_year: null, cover: null, private: false, suggested: false, people: [], editions: [] });
	return applyOps(d, [
		{ id: 'c1', hlc: T0, op: { type: 'book.create', id: 'b', work_id: 'w', rank: 'V', added_at: 0 } },
		{ id: 'c2', hlc: T0, op: { type: 'reading.create', id: 'r', book_id: 'b', started_on: '2026-11-01', total: 100 } }
	]);
}

const tap = (id: string, hlc: string, baseClock: string, from: number, to: number): Op => ({
	id,
	hlc,
	op: { type: 'progress', event_id: `e-${id}`, reading_id: 'r', base: baseClock, from, to, local_date: '2026-11-01' }
});

const live = (d: ReturnType<typeof base>) =>
	[...d.events.values()].filter((e) => !e.deleted).map((e) => e.id).sort();

describe('applyOps', () => {
	it('a newer offline chain replaces the other branch (same rule as the server)', () => {
		const a1 = h(2000, 'aaa');
		const a2 = h(3000, 'aaa');
		const b1 = h(2500, 'bbb');
		const b2 = h(4000, 'bbb');
		let d = applyOps(cloneData(base()), [tap('a1', a1, T0, 0, 1), tap('a2', a2, a1, 1, 2)]);
		d = applyOps(cloneData(d), [tap('b1', b1, T0, 0, 5), tap('b2', b2, b1, 5, 6)]);
		expect(d.readings.get('r')!.pos).toBe(6);
		expect(live(d)).toEqual(['e-b1', 'e-b2']);
	});

	it('an older offline chain loses entirely', () => {
		let d = applyOps(cloneData(base()), [tap('b1', h(5000, 'bbb'), T0, 0, 5)]);
		d = applyOps(cloneData(d), [tap('a1', h(2000, 'aaa'), T0, 0, 1)]);
		expect(d.readings.get('r')!.pos).toBe(5);
		expect(live(d)).toEqual(['e-b1']);
	});

	it('fields are last-writer-wins', () => {
		let d = applyOps(cloneData(base()), [
			{ id: 's1', hlc: h(3000, 'a'), op: { type: 'book.set', id: 'b', fields: { rating: 4 } } }
		]);
		d = applyOps(cloneData(d), [
			{ id: 's2', hlc: h(2000, 'b'), op: { type: 'book.set', id: 'b', fields: { rating: 2, rank: 'k' } } }
		]);
		expect(d.books.get('b')!.rating).toBe(4);
		expect(d.books.get('b')!.rank).toBe('k');
	});

	it('does not change the input data', () => {
		const d = base();
		applyOps(cloneData(d), [tap('a1', h(2000, 'a'), T0, 0, 1)]);
		expect(d.readings.get('r')!.pos).toBe(0);
	});
});
