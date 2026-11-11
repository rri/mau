// Reading (spec §4): the list and grid of unfinished books and their controls.
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Reading from './reading/+page.svelte';
import { bookInfo } from '#lib/derive.ts';
import { library } from '#lib/library.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { emptyData } from '#lib/sync/apply.ts';
import { freezeTime, signIn, useLibrary } from '../../test/fixtures.ts';

const info = (id: string) => bookInfo(library.view, library.view.books.get(id)!);
const rows = () => screen.getAllByRole('listitem').filter((li) => li.classList.contains('row'));
const row = (title: string) => rows().find((r) => r.textContent?.includes(title))!;

beforeEach(() => {
	freezeTime();
	signIn();
});

describe('Reading list', () => {
	it('READ-01 lists unfinished books: active first, then inactive, each in your order', () => {
		useLibrary();
		render(Reading);
		expect(rows().map((r) => within(r).getByRole('link').textContent)).toEqual([
			'The Left Hand of Darkness',
			'Middlemarch',
			'Station Eleven',
			'The Brothers Karamazov'
		]);
		expect(rows()[0]).toHaveClass('active');
		expect(rows()[2]).toHaveClass('idle');
	});

	it('READ-02 each row shows pages and when it was last read', () => {
		useLibrary();
		render(Reading);
		expect(row('Left Hand')).toHaveTextContent('page 140 of 304 +6 pages today');
		expect(row('Middlemarch')).toHaveTextContent('page 299 of 880 last read yesterday');
		expect(row('Karamazov')).toHaveTextContent('page 310 of 796 last read 3 months ago');
		expect(row('Station Eleven')).toHaveTextContent('Not started added 3 weeks ago');
	});

	it('READ-02 one page today is singular; a started book with no progress says so', () => {
		const d = emptyData();
		useLibrary(d);
		library.commit([
			{
				type: 'work.create',
				work: {
					id: 'w',
					title: 'New',
					first_year: null,
					people: [{ name: 'A', role: 'author' }],
					edition: null
				}
			},
			{ type: 'book.create', id: 'b', work_id: 'w', rank: 'M', added_at: Date.now() },
			{ type: 'reading.create', id: 'r', book_id: 'b', started_on: '2026-11-11', total: 50 },
			{ type: 'book.set', id: 'b', fields: { active: true } }
		]);
		render(Reading);
		expect(row('New')).toHaveTextContent('no progress yet');
		library.commit([
			{
				type: 'progress',
				event_id: 'e',
				reading_id: 'r',
				base: '',
				from: 0,
				to: 1,
				local_date: '2026-11-11'
			}
		]);
		return screen.findByText('+1 page today');
	});

	it('READ-03 + adds a page; pause moves the book down; play resumes it', async () => {
		useLibrary();
		render(Reading);
		await userEvent.click(screen.getByRole('button', { name: 'Add 1 page to Middlemarch' }));
		expect(info('mm').reading!.pos).toBe(300);
		await userEvent.click(screen.getByRole('button', { name: 'Pause Middlemarch' }));
		expect(info('mm').state).toBe('inactive');
		expect(toasts.current?.message).toBe('Paused Middlemarch');
		await userEvent.click(screen.getByRole('button', { name: 'Resume The Brothers Karamazov' }));
		expect(info('bk').state).toBe('active');
	});

	it('READ-04 the page count opens an inline editor; Enter saves, Escape cancels', async () => {
		useLibrary();
		render(Reading);
		await userEvent.click(screen.getByRole('button', { name: 'Edit pages, now page 299 of 880' }));
		const cur = screen.getByRole('spinbutton', { name: 'Current page' });
		expect(cur).toHaveFocus();
		await userEvent.clear(cur);
		await userEvent.type(cur, '350{Enter}');
		expect(info('mm').reading!.pos).toBe(350);
		expect(toasts.current?.message).toBe('Moved to page 350');

		await userEvent.click(screen.getByRole('button', { name: 'Edit pages, now page 350 of 880' }));
		await userEvent.type(screen.getByRole('spinbutton', { name: 'Current page' }), '9{Escape}');
		expect(info('mm').reading!.pos).toBe(350);
		expect(screen.queryByRole('spinbutton', { name: 'Current page' })).toBeNull();
	});

	it('READ-05 starting a never-read book asks for its pages inline, with the catalog’s count as a hint', async () => {
		useLibrary();
		render(Reading);
		await userEvent.click(screen.getByRole('button', { name: 'Start Station Eleven' }));
		const total = screen.getByLabelText('How many pages does your copy have?');
		expect(total).toHaveFocus();
		await userEvent.click(screen.getByRole('button', { name: 'Start reading' }));
		expect(screen.getByRole('alert')).toHaveTextContent('Enter the number of pages in your copy.');
		await userEvent.click(screen.getByRole('button', { name: 'Catalog lists 333 pages' }));
		expect(total).toHaveValue(333);
		await userEvent.click(screen.getByRole('button', { name: 'Start reading' }));
		expect(info('se').state).toBe('active');
		expect(info('se').reading).toMatchObject({ pos: 0, total: 333 });
	});

	it('READ-06 search filters by title or author, counts matches, and turns off dragging', async () => {
		useLibrary();
		render(Reading);
		expect(screen.getAllByRole('button', { name: /Drag to reorder/ })).toHaveLength(4);
		await userEvent.type(
			screen.getByRole('searchbox', { name: "Search books you're reading" }),
			'eliot'
		);
		expect(rows()).toHaveLength(1);
		expect(screen.getByText(/1 of 4\s+books/)).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /Drag to reorder/ })).toBeNull();
		await userEvent.click(screen.getByRole('button', { name: 'Clear search' }));
		expect(rows()).toHaveLength(4);
	});

	it('READ-07 a search with no matches says so', async () => {
		useLibrary();
		render(Reading);
		await userEvent.type(screen.getByRole('searchbox'), 'dune');
		expect(
			screen.getByText('No books you’re reading match “dune”.'.replace('’', "'"))
		).toBeInTheDocument();
	});

	it('READ-08 the grid shows tiles; starting a never-read book opens a dialog', async () => {
		useLibrary();
		render(Reading);
		await userEvent.click(screen.getByRole('button', { name: 'Grid layout' }));
		expect(localStorage.getItem('mau.reading.layout')).toBe('grid');
		const tiles = screen.getAllByRole('listitem').filter((li) => li.classList.contains('tile'));
		expect(tiles).toHaveLength(4);
		expect(tiles[0]).toHaveTextContent('p. 140/304');
		expect(tiles[2]).toHaveTextContent('Not started');
		await userEvent.click(screen.getByRole('button', { name: 'Start Station Eleven' }));
		const dialog = screen.getByRole('dialog', { name: 'Station Eleven' });
		await userEvent.keyboard('{Escape}');
		expect(dialog).not.toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Start Station Eleven' }));
		await userEvent.type(screen.getByLabelText('How many pages does your copy have?'), '320');
		await userEvent.click(screen.getByRole('button', { name: 'Start reading' }));
		expect(info('se').reading).toMatchObject({ total: 320 });
		expect(screen.queryByRole('dialog')).toBeNull();
	});

	it('READ-09 the layout choice is remembered on this device', () => {
		localStorage.setItem('mau.reading.layout', 'grid');
		useLibrary();
		render(Reading);
		expect(screen.getByRole('button', { name: 'Grid layout' })).toHaveAttribute(
			'aria-pressed',
			'true'
		);
	});

	it('READ-10 offline and unsynced changes show next to the title', () => {
		useLibrary();
		library.online = false;
		library.pending = 2;
		const { unmount } = render(Reading);
		expect(screen.getByText('Offline · 2 unsynced')).toBeInTheDocument();
		unmount();
		library.online = true;
		render(Reading);
		expect(screen.getByText('Syncing…')).toBeInTheDocument();
	});

	it('READ-11 an empty list welcomes the reader with ways to add books', () => {
		useLibrary(emptyData());
		render(Reading);
		expect(screen.getByText('Welcome, R. Reader.')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Add your first book' })).toHaveAttribute(
			'href',
			'/add'
		);
		expect(screen.queryByRole('searchbox')).toBeNull();
		expect(screen.queryByRole('button', { name: 'Grid layout' })).toBeNull();
	});

	it('READ-13 arrow keys on the handle move a book one place within its group', async () => {
		useLibrary();
		render(Reading);
		const order = () => rows().map((r) => within(r).getByRole('link').textContent);
		const handle = (t: string) =>
			screen.getByRole('button', { name: new RegExp(`Drag to reorder ${t}`) });
		handle('Middlemarch').focus();
		await userEvent.keyboard('{ArrowUp}');
		expect(order().slice(0, 2)).toEqual(['Middlemarch', 'The Left Hand of Darkness']);
		expect(toasts.current?.message).toBe('Moved Middlemarch');
		expect(handle('Middlemarch')).toHaveFocus();
		// Not across into the inactive books (pause does that).
		handle('The Left Hand of Darkness').focus();
		await userEvent.keyboard('{ArrowDown}');
		expect(info('lhod').state).toBe('active');
		expect(order()[1]).toBe('The Left Hand of Darkness');
		// Within the inactive books.
		handle('The Brothers Karamazov').focus();
		await userEvent.keyboard('{ArrowUp}');
		expect(order().slice(2)).toEqual(['The Brothers Karamazov', 'Station Eleven']);
		expect(info('bk').state).toBe('inactive');
	});

	it('READ-12 while loading, it says so', () => {
		render(Reading);
		expect(screen.getByText('Loading your library…')).toBeInTheDocument();
	});
});
