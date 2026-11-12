// Read-only views over the library: names, states, the Reading order, search.
import { describe, expect, it } from 'vitest';
import {
	bookInfo,
	lastReadDate,
	liveBooks,
	matchesSearch,
	pagesOn,
	readingList
} from './derive.ts';
import { addBook, sampleLibrary, TODAY } from '../test/fixtures.ts';
import { emptyData } from './sync/apply.ts';

describe('derive', () => {
	it('DATA-01 a book is finished when its latest reading is, else active or inactive by its flag', () => {
		const d = sampleLibrary();
		const state = (id: string) => bookInfo(d, d.books.get(id)!).state;
		expect([state('lhod'), state('se'), state('bk'), state('pir')]).toEqual([
			'active',
			'inactive',
			'inactive',
			'finished'
		]);
	});

	it('DATA-02 your own title, authors, and cover win over the catalog’s', () => {
		const d = sampleLibrary();
		expect(bookInfo(d, d.books.get('se')!).title).toBe('Station Eleven');
		// Each change makes a new snapshot (as library.commit does).
		const e = sampleLibrary();
		const b = e.books.get('se')!;
		e.books.set('se', {
			...b,
			title_override: 'S11',
			authors_override: ['E. M.'],
			cover_override: 'abc'
		});
		const i = bookInfo(e, e.books.get('se')!);
		expect([i.title, i.authors, i.cover]).toEqual(['S11', ['E. M.'], 'abc']);
	});

	it('DATA-03 owned means at least one live copy; tags are sorted', () => {
		const d = sampleLibrary();
		const lhod = bookInfo(d, d.books.get('lhod')!);
		expect(lhod.owned).toBe(true);
		expect(lhod.tags).toEqual(['classics', 'sci-fi']);
		expect(bookInfo(d, d.books.get('mm')!).owned).toBe(false);
	});

	it('DATA-04 deleted books and books whose work is missing are left out', () => {
		const d = sampleLibrary();
		d.books.set('se', { ...d.books.get('se')!, deleted: true });
		d.works.delete('w-mm');
		expect(
			liveBooks(d)
				.map((b) => b.book.id)
				.sort()
		).toEqual(['bk', 'lhod', 'pir']);
	});

	it('DATA-05 the Reading list: active by rank, then inactive by rank; finished left out', () => {
		const d = sampleLibrary();
		addBook(d, { id: 'x', title: 'X', author: 'A', rank: 'A', active: false });
		expect(readingList(d).map((b) => b.book.id)).toEqual(['lhod', 'mm', 'x', 'se', 'bk']);
	});

	it('DATA-06 pages today count forward moves only; last read is the latest event', () => {
		const d = sampleLibrary();
		expect(pagesOn(d, d.readings.get('00r-lhod')!, TODAY)).toBe(6);
		expect(lastReadDate(d, '00r-lhod')).toBe(TODAY);
		expect(lastReadDate(d, 'nope')).toBeNull();
	});

	it('DATA-07 search matches title or any author, ignoring case', () => {
		const d = emptyData();
		addBook(d, { id: 'a', title: 'Piranesi', author: 'Susanna Clarke' });
		const b = bookInfo(d, d.books.get('a')!);
		expect(matchesSearch(b, '')).toBe(true);
		expect(matchesSearch(b, 'pira')).toBe(true);
		expect(matchesSearch(b, 'clarke')).toBe(true);
		expect(matchesSearch(b, 'dune')).toBe(false);
	});
});
