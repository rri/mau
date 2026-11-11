// Home (spec §3b): greeting, books in progress, and reading trends.
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Home from './+page.svelte';
import { library } from '#lib/library.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { emptyData } from '#lib/sync/apply.ts';
import { addBook, freezeTime, sampleLibrary, signIn, TODAY, useLibrary } from '../../test/fixtures.ts';

beforeEach(() => signIn());

describe('Home', () => {
	it('HOME-01 the header shows the date and greets the reader by the time of day', () => {
		freezeTime('2026-11-11T08:00:00Z');
		useLibrary();
		render(Home);
		expect(screen.getByText('Wednesday 11 November')).toBeInTheDocument();
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Good morning, R. Reader!');
	});

	it.each([
		['2026-11-11T03:00:00Z', 'Good night'],
		['2026-11-11T13:00:00Z', 'Good afternoon'],
		['2026-11-11T19:00:00Z', 'Good evening']
	])('HOME-01 at %s it says %s', (at, hello) => {
		freezeTime(at);
		useLibrary();
		render(Home);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(`${hello}, R. Reader!`);
	});

	it('HOME-02 while the library loads, it says so', () => {
		render(Home);
		expect(screen.getByText('Loading your library…')).toBeInTheDocument();
	});

	it('HOME-03 an empty library gets a welcome and the two ways to start', () => {
		freezeTime();
		useLibrary(emptyData());
		render(Home);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Welcome, R. Reader!');
		expect(screen.getByRole('link', { name: 'Add your first book' })).toHaveAttribute('href', '/add');
		expect(screen.getByRole('link', { name: /Import from Goodreads/ })).toHaveAttribute(
			'href',
			'/settings/import'
		);
		expect(screen.queryByText('Pick up where you left off')).toBeNull();
	});

	it('HOME-04 Pick up where you left off lists active books with progress and pace', () => {
		freezeTime();
		useLibrary();
		render(Home);
		const sec = screen.getByRole('region', { name: 'Pick up where you left off' });
		const items = within(sec).getAllByRole('listitem');
		expect(items.map((li) => within(li).getAllByRole('link')[0].textContent)).toEqual([
			'The Left Hand of Darkness',
			'Middlemarch'
		]);
		// 140 pages in the last 28 days → 5 a day; 164 left → 33 days.
		expect(items[0]).toHaveTextContent('page 140 of 304 · about 33 days left at your pace');
		// Middlemarch: 49 pages in 28 days → 581 left at 1.75 a day = 332 days.
		expect(items[1]).toHaveTextContent('page 299 of 880 · about 332 days left at your pace');
		expect(within(sec).getByRole('link', { name: 'All 4 books' })).toHaveAttribute('href', '/reading');
	});

	it('HOME-05 no pace is shown without reading in the last four weeks', () => {
		freezeTime();
		const d = emptyData();
		addBook(d, {
			id: 'a',
			title: 'Old',
			author: 'A',
			active: true,
			reading: { pos: 10, total: 100, started: '2026-01-01' },
			events: [['2026-01-02', 0, 10]]
		});
		useLibrary(d);
		render(Home);
		expect(screen.getByText('page 10 of 100')).toBeInTheDocument();
		expect(screen.queryByText(/left at your pace/)).toBeNull();
	});

	it('HOME-06 at most five books are listed', () => {
		freezeTime();
		const d = emptyData();
		for (let i = 0; i < 7; i++)
			addBook(d, {
				id: `b${i}`,
				title: `Book ${i}`,
				author: 'A',
				rank: `B${i}`,
				active: true,
				reading: { pos: 1, total: 100, started: TODAY }
			});
		useLibrary(d);
		render(Home);
		const sec = screen.getByRole('region', { name: 'Pick up where you left off' });
		expect(within(sec).getAllByRole('listitem')).toHaveLength(5);
		expect(within(sec).getByRole('link', { name: 'All 7 books' })).toBeInTheDocument();
	});

	it('HOME-07 with nothing active, it points to the Reading list', () => {
		freezeTime();
		const d = sampleLibrary();
		for (const id of ['lhod', 'mm']) d.books.set(id, { ...d.books.get(id)!, active: false });
		useLibrary(d);
		render(Home);
		expect(screen.getByText(/Nothing in progress right now/)).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Choose a book' })).toHaveAttribute('href', '/reading');
	});

	it('HOME-08 + adds a page to that book, with an undo toast', async () => {
		freezeTime();
		useLibrary();
		render(Home);
		await userEvent.click(screen.getByRole('button', { name: 'Add 1 page to Middlemarch' }));
		expect(await screen.findByText(/page 300 of 880/)).toBeInTheDocument();
		expect(toasts.current?.message).toBe('+1 page on Middlemarch');
	});

	it('HOME-09 the covers and titles link to the book', () => {
		freezeTime();
		useLibrary();
		render(Home);
		expect(screen.getByRole('link', { name: 'Middlemarch' })).toHaveAttribute('href', '/catalog/mm');
	});

	it('HOME-10 the last 14 days: one bar per day, today labelled, numbers in a table', () => {
		freezeTime();
		useLibrary();
		const { container } = render(Home);
		expect(screen.getByRole('heading', { level: 2, name: 'Your reading trends' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { level: 3, name: 'Last 14 days' })).toBeInTheDocument();
		expect(container.querySelectorAll('.plot .bar')).toHaveLength(14);
		const table = screen.getByRole('table', { name: 'Pages read each of the last 14 days' });
		const rows = within(table).getAllByRole('row').map((r) => r.textContent);
		expect(rows).toHaveLength(14);
		expect(rows[0]).toBe('Oct 29: no reading');
		expect(rows.slice(-3)).toEqual(['Nov 9: 20 pages', 'Nov 10: 63 pages', 'Nov 11: 6 pages']);
		expect(container.querySelector('.labels span:last-child')).toHaveTextContent('today');
	});

	it('HOME-11 this year so far: books finished, pages read, days with reading', () => {
		freezeTime();
		useLibrary();
		render(Home);
		expect(screen.getByRole('heading', { level: 3, name: '2026 (so far)' })).toBeInTheDocument();
		const tile = (label: string) => screen.getByText(label).parentElement!;
		expect(tile('books finished')).toHaveTextContent('1');
		// 100 + 20 + 14 + 6 (lhod) + 49 (mm) + 310 (bk) + nothing for Piranesi's undated pages
		expect(tile('pages read')).toHaveTextContent('499');
		expect(tile('days with reading')).toHaveTextContent('5');
	});

	it('HOME-12 the numbers follow the library as it changes', async () => {
		freezeTime();
		useLibrary();
		render(Home);
		library.commit([
			{
				type: 'progress',
				event_id: 'e-new',
				reading_id: '00r-mm',
				base: '',
				from: 299,
				to: 309,
				local_date: TODAY
			}
		]);
		expect(await screen.findByText('Nov 11: 16 pages')).toBeInTheDocument();
	});
});
