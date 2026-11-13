// A book's page (spec §6): header, status, rating and notes, tags, copies,
// activity, editing, and removal.
import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Book from './catalog/[id]/+page.svelte';
import { bookInfo, eventsOf } from '#lib/derive.ts';
import { library } from '#lib/library.svelte.ts';
import { origin } from '#lib/origin.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { at, nav } from '../../test/app-mocks.ts';
import {
	addBook,
	freezeTime,
	mockApi,
	sampleLibrary,
	signIn,
	useLibrary
} from '../../test/fixtures.ts';

const info = (id: string) => bookInfo(library.view, library.view.books.get(id)!);
const open = (id: string) => {
	at(`/catalog/${id}`, { id });
	return render(Book);
};

beforeEach(() => {
	freezeTime();
	signIn();
	origin.visit('/catalog');
});
afterEach(() => origin.visit('/catalog'));

describe('Book page: header', () => {
	it('BOOK-01 shows the cover, title, authors, other people, and first publication', () => {
		const d = sampleLibrary();
		d.works.get('w-mm')!.people.push({ name: 'A. Translator', role: 'translator' });
		useLibrary(d);
		open('mm');
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Middlemarch');
		expect(screen.getByText('George Eliot')).toBeInTheDocument();
		expect(screen.getByText(/Translated by\s+A. Translator/)).toBeInTheDocument();
		expect(screen.getByText('First published 1871')).toBeInTheDocument();
	});

	it('BOOK-02 the trail starts at the section the reader came from', () => {
		useLibrary();
		origin.visit('/reading');
		open('mm');
		const crumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
		expect(within(crumbs).getAllByRole('link', { name: 'Reading' })[0]).toHaveAttribute(
			'href',
			'/reading'
		);
		expect(
			within(crumbs).getByText('Middlemarch', { selector: '[aria-current]' })
		).toBeInTheDocument();
	});

	it('BOOK-03 a book that isn’t on the list says so, with the way back', () => {
		useLibrary();
		open('nope');
		expect(screen.getByText("This book isn't on your list.")).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Back to Catalog' })).toHaveAttribute(
			'href',
			'/catalog'
		);
	});

	it('BOOK-04 a private entry says only you can see it, and can be suggested for the shared catalog', async () => {
		const d = sampleLibrary();
		d.works.get('w-se')!.private = true;
		useLibrary(d);
		const fetch = mockApi({ 'POST /catalog/works/w-se/suggest': { ok: true } });
		open('se');
		expect(screen.getByText('Only you can see this entry')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'More actions' }));
		await userEvent.click(screen.getByRole('menuitem', { name: 'Suggest for shared catalog' }));
		expect(fetch).toHaveBeenCalled();
		expect(toasts.current?.message).toBe(
			'Suggested for the shared catalog. An admin will review it.'
		);
	});

	it('BOOK-05 the menu closes on a click elsewhere', async () => {
		useLibrary();
		open('mm');
		await userEvent.click(screen.getByRole('button', { name: 'More actions' }));
		expect(screen.getByRole('menu')).toBeInTheDocument();
		await userEvent.click(document.body);
		expect(screen.queryByRole('menu')).toBeNull();
	});

	it('BOOK-06 Edit details changes the title and authors for this reader only; reset goes back', async () => {
		useLibrary();
		open('mm');
		await userEvent.click(screen.getByRole('button', { name: 'More actions' }));
		await userEvent.click(screen.getByRole('menuitem', { name: 'Edit details' }));
		const title = screen.getByLabelText('Title');
		await userEvent.clear(title);
		await userEvent.type(title, 'Middlemarch: A Study');
		const authors = screen.getByLabelText('Authors, separated by commas');
		await userEvent.clear(authors);
		await userEvent.type(authors, 'George Eliot, Mary Ann Evans');
		await userEvent.click(screen.getByRole('button', { name: 'Save' }));
		expect(info('mm').title).toBe('Middlemarch: A Study');
		expect(info('mm').authors).toEqual(['George Eliot', 'Mary Ann Evans']);
		expect(screen.getByText(/You've edited these details/)).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Reset to catalog' }));
		expect(info('mm').title).toBe('Middlemarch');
		expect(screen.queryByText(/You've edited these details/)).toBeNull();
	});
});

describe('Book page: status', () => {
	it('BOOK-10 an active book shows its pill, progress, today’s pages, and the reading buttons', async () => {
		useLibrary();
		open('lhod');
		const T = 'The Left Hand of Darkness';
		expect(screen.getByText('Active', { selector: '.pill' })).toBeInTheDocument();
		expect(screen.getByText('+6 pages today')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: `Add 1 page to ${T}` }));
		expect(screen.getByText('+7 pages today')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: `Pause ${T}` }));
		expect(screen.getByText('Inactive', { selector: '.pill' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: `Resume ${T}` })).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: `Finish ${T}` }));
		expect(screen.getByText('Finished', { selector: '.pill' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Re-read' })).toBeInTheDocument();
	});

	it('BOOK-11 one page today is singular', async () => {
		useLibrary();
		open('mm');
		await userEvent.click(screen.getByRole('button', { name: 'Add 1 page to Middlemarch' }));
		expect(screen.getByText('+1 page today')).toBeInTheDocument();
	});

	it('BOOK-12 the page count opens an editor for page and total', async () => {
		useLibrary();
		open('mm');
		await userEvent.click(screen.getByRole('button', { name: 'page 299 of 880' }));
		const total = screen.getByRole('spinbutton', { name: 'Total pages' });
		await userEvent.clear(total);
		await userEvent.type(total, '900');
		await userEvent.click(screen.getByRole('button', { name: 'Save' }));
		expect(info('mm').reading).toMatchObject({ pos: 299, total: 900 });
	});

	it('BOOK-13 starting a never-read book asks for its pages', async () => {
		useLibrary();
		open('se');
		expect(screen.getByText('Not started')).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Start Station Eleven' }));
		await userEvent.click(screen.getByRole('button', { name: 'Start reading' }));
		expect(
			screen.getByText('Enter the number of pages in your copy, or track by percent.')
		).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Catalog lists 333 pages' }));
		await userEvent.click(screen.getByRole('button', { name: 'Start reading' }));
		expect(info('se').state).toBe('active');
	});

	it('BOOK-14 a finished book shows its date and offers a re-read', async () => {
		useLibrary();
		open('pir');
		// The pill says Finished; the status line just the date.
		expect(screen.getAllByText('Finished', { exact: false })).toHaveLength(1);
		expect(document.querySelector('.meta')).toHaveTextContent(/^\s*Oct 28, 2026\s*$/);
		await userEvent.click(screen.getByRole('button', { name: 'Re-read' }));
		expect(info('pir').state).toBe('active');
		expect(info('pir').readings).toHaveLength(2);
	});
});

describe('Book page: percent', () => {
	it('BOOK-16 a percent reading shows percent, adds 1%, and its history reads in percent', async () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'pc',
			title: 'Percent Book',
			author: 'P. Author',
			active: true,
			reading: { pos: 40, total: 100, started: '2026-11-01', unit: 'percent' },
			events: [['2026-11-10', 30, 40]]
		});
		useLibrary(d);
		open('pc');
		expect(screen.getByRole('button', { name: '40%' })).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Add 1% to Percent Book' }));
		expect(screen.getByText('+1% today')).toBeInTheDocument();
		expect(screen.getByText('30% → 40%')).toBeInTheDocument();
		const stats = document.querySelector('.stats')!;
		expect(stats).toHaveTextContent('2 days read');
		expect(stats).not.toHaveTextContent('pages');
	});
});

describe('Book page: readings without dates', () => {
	it('BOOK-17 an imported finished read without dates: no zero stats, no empty calendar, plain wording', () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'imp',
			title: 'Imported Book',
			author: 'I. Author',
			pages: 300,
			finished: ['0000-01-01']
		});
		const r = [...d.readings.values()].find((x) => x.book_id === 'imp')!;
		d.readings.set(r.id, { ...r, started_on: '0000-01-01' });
		useLibrary(d);
		open('imp');
		expect(document.body).not.toHaveTextContent(/date unknown/i);
		expect(document.querySelector('.meta')).toHaveTextContent(/^\s*$/);
		const stats = document.querySelector('.stats')!;
		expect(stats).toHaveTextContent(/^\s*1 reading\s*$/);
		expect(document.querySelector('.legend')).toBeNull();
		expect(screen.getByText('Reading 1')).toBeInTheDocument();
		expect(document.querySelector('summary')).toHaveTextContent(/Reading 1\s*Finished/);
		expect(screen.getByText('No pages logged')).toHaveClass('empty');
	});

	it('BOOK-18 a finished reading without its date takes one inline, logging the whole book that day', async () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'imp',
			title: 'Imported Book',
			author: 'I. Author',
			pages: 300,
			finished: ['0000-01-01']
		});
		useLibrary(d);
		open('imp');
		const save = screen.getByRole('button', { name: 'Save date' });
		expect(save).toBeDisabled();
		await userEvent.type(screen.getByLabelText('Finished on'), '2026-09-14');
		await userEvent.click(save);
		const r = info('imp').reading!;
		expect(r.finished_on).toBe('2026-09-14');
		expect(r.pos).toBe(300);
		expect(screen.getByText('page 0 → 300')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Save date' })).toBeNull();
		expect(document.querySelector('.meta')).toHaveTextContent('Sep 14, 2026');
		// Undo puts it back as it was.
		toasts.current?.action?.run();
		expect(info('imp').reading!.finished_on).toBe('0000-01-01');
		expect(eventsOf(library.view, info('imp').reading!.id)).toHaveLength(0);
		flushSync();
		expect(screen.getByText('No pages logged')).toBeInTheDocument();
	});

	it('BOOK-18 a percent reading is logged to 100%', async () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'imp',
			title: 'Imported Book',
			author: 'I. Author',
			finished: ['0000-01-01']
		});
		const r = [...d.readings.values()].find((x) => x.book_id === 'imp')!;
		d.readings.set(r.id, { ...r, unit: 'percent', total: 100, pos: 100 });
		useLibrary(d);
		open('imp');
		await userEvent.type(screen.getByLabelText('Finished on'), '2026-09-14');
		await userEvent.click(screen.getByRole('button', { name: 'Save date' }));
		expect(screen.getByText('0% → 100%')).toBeInTheDocument();
	});
});

describe('Book page: deleting readings', () => {
	it('BOOK-19 deleting the only reading of a finished book makes it Inactive, after asking', async () => {
		useLibrary();
		open('pir');
		await userEvent.click(screen.getByRole('button', { name: 'Delete reading' }));
		const dialog = screen.getByRole('dialog', { name: 'Delete reading 1?' });
		expect(dialog).toHaveTextContent('the book is Inactive again');
		await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
		expect(info('pir').readings).toHaveLength(1);
		await userEvent.click(screen.getByRole('button', { name: 'Delete reading' }));
		await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
		expect(info('pir').readings).toHaveLength(0);
		expect(info('pir').state).toBe('inactive');
		expect(screen.getByText('No reading yet.')).toBeInTheDocument();
	});

	it('BOOK-19 deleting the current reading of an active book leaves the earlier one', async () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'rr',
			title: 'Reread Book',
			author: 'R. Author',
			active: true,
			finished: ['2026-05-01'],
			reading: { pos: 40, total: 300, started: '2026-11-01' },
			events: [['2026-11-10', 0, 40]]
		});
		useLibrary(d);
		open('rr');
		await userEvent.click(screen.getAllByRole('button', { name: 'Delete reading' })[0]);
		const dialog = screen.getByRole('dialog', { name: 'Delete reading 2?' });
		expect(dialog).toHaveTextContent('with 1 day of activity');
		await userEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
		expect(info('rr').readings).toHaveLength(1);
		expect(info('rr').state).toBe('finished');
		expect(info('rr').book.active).toBe(false);
	});
});

describe('Book page: rating, notes, tags, copies', () => {
	it('BOOK-20 stars set the rating; the same star again clears it', async () => {
		useLibrary();
		open('mm');
		const stars = within(screen.getByRole('radiogroup', { name: 'Rating' })).getAllByRole('radio');
		expect(stars).toHaveLength(5);
		await userEvent.click(screen.getByRole('radio', { name: '4 stars' }));
		expect(info('mm').book.rating).toBe(4);
		expect(screen.getByRole('radio', { name: '4 stars' })).toHaveAttribute('aria-checked', 'true');
		await userEvent.click(screen.getByRole('radio', { name: '4 stars' }));
		expect(info('mm').book.rating).toBe(0);
	});

	it('BOOK-21 notes save on their own shortly after typing stops', async () => {
		useLibrary();
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		open('mm');
		await user.type(screen.getByRole('textbox', { name: 'Notes' }), 'Dorothea!');
		expect(info('mm').book.notes).toBe('');
		vi.advanceTimersByTime(700);
		expect(info('mm').book.notes).toBe('Dorothea!');
		expect(await screen.findByText('Saved')).toBeInTheDocument();
	});

	it('BOOK-22 tags: add from suggestions or create new; × removes', async () => {
		useLibrary();
		open('mm');
		const input = screen.getByRole('combobox', { name: 'Add tag' });
		await userEvent.type(input, 'sci');
		expect(screen.getByRole('option', { name: '#sci-fi' })).toBeInTheDocument();
		expect(screen.getByRole('option', { name: 'Create new tag #sci' })).toBeInTheDocument();
		await userEvent.keyboard('{Enter}');
		expect(info('mm').tags).toEqual(['classics', 'sci-fi']);
		await userEvent.type(input, 'Victorian{Enter}');
		expect(info('mm').tags).toEqual(['classics', 'sci-fi', 'Victorian']);
		await userEvent.click(screen.getByRole('button', { name: 'Remove tag classics' }));
		expect(info('mm').tags).toEqual(['sci-fi', 'Victorian']);
	});

	it('BOOK-23 copies: listed with format, location, and edition; added and removed', async () => {
		useLibrary();
		open('mm');
		expect(
			screen.getByText("You don't own a copy. Add one to mark this book as owned.")
		).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: '+ Add copy' }));
		await userEvent.selectOptions(screen.getByLabelText('Format'), 'Ebook');
		// The edition starts as unknown; the work's editions are listed too.
		const edition = screen.getByLabelText('Edition');
		expect(edition).toHaveDisplayValue('Unknown edition');
		await userEvent.selectOptions(edition, 'Ace, Paperback, 1871');
		await userEvent.type(screen.getByLabelText('Location'), 'Kindle');
		await userEvent.click(screen.getByRole('button', { name: 'Add copy' }));
		expect(info('mm').owned).toBe(true);
		expect(screen.getByText('Copies · owned')).toBeInTheDocument();
		const li = screen.getByText('Ebook').closest('li')!;
		expect(li).toHaveTextContent('Ebook · Kindle');
		expect(li).toHaveTextContent('Ace, Paperback, 1871');
		await userEvent.click(within(li).getByRole('button', { name: 'Remove this copy' }));
		expect(info('mm').owned).toBe(false);
		expect(toasts.current?.message).toBe('Copy removed');
	});

	it('BOOK-24 a copy without a location says so', () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'x',
			title: 'X',
			author: 'A',
			copies: [{ format: 'Hardcover', location: '' }]
		});
		useLibrary(d);
		open('x');
		expect(screen.getByText('No location')).toBeInTheDocument();
	});
});

describe('Book page: activity and removal', () => {
	it('BOOK-30 totals, a calendar per year, and each reading’s days (newest first)', () => {
		useLibrary();
		open('lhod');
		const sec = screen.getByRole('heading', { name: 'Reading activity' }).closest('section')!;
		expect(sec).toHaveTextContent('4 days read');
		expect(sec).toHaveTextContent('140 pages');
		expect(sec).toHaveTextContent('1 reading');
		expect(
			within(sec).getByRole('img', { name: 'Pages read per day in 2026' })
		).toBeInTheDocument();
		const days = within(
			within(sec).getByRole('region', { name: 'Days read in reading 1' })
		).getAllByRole('listitem');
		expect(days.map((d) => d.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
			'Nov 11 page 134 → 140+6',
			'Nov 10 page 120 → 134+14',
			'Nov 9 page 100 → 120+20',
			'Oct 20 page 0 → 100+100'
		]);
	});

	it('BOOK-31 singular totals; a reading with no progress says so', () => {
		const d = sampleLibrary();
		addBook(d, {
			id: 'x',
			title: 'X',
			author: 'A',
			active: true,
			reading: { pos: 1, total: 10, started: '2026-11-11' },
			events: [['2026-11-11', 0, 1]]
		});
		addBook(d, {
			id: 'y',
			title: 'Y',
			author: 'A',
			active: true,
			reading: { pos: 0, total: 10, started: '2026-11-11' }
		});
		useLibrary(d);
		const { unmount } = open('x');
		const sec = () => screen.getByRole('heading', { name: 'Reading activity' }).closest('section')!;
		expect(sec()).toHaveTextContent('1 day read');
		expect(sec()).toHaveTextContent('1 page');
		expect(sec()).toHaveTextContent('2026 1 page');
		unmount();
		open('y');
		expect(screen.getByText('No progress yet')).toBeInTheDocument();
	});

	it('BOOK-32 a never-read book has no activity yet', () => {
		useLibrary();
		open('se');
		expect(screen.getByText('No reading yet.')).toBeInTheDocument();
	});

	it('BOOK-33 removing asks first, lists what goes, then deletes and returns to the catalog', async () => {
		useLibrary();
		open('lhod');
		await userEvent.click(screen.getByRole('button', { name: 'More actions' }));
		await userEvent.click(screen.getByRole('menuitem', { name: 'Remove from list' }));
		const dialog = screen.getByRole('dialog', { name: 'Remove The Left Hand of Darkness?' });
		expect(dialog).toHaveTextContent(
			"This permanently deletes 1 reading with their activity, your rating, notes, 1 copy and 2 tags. This can't be undone."
		);
		await userEvent.keyboard('{Escape}');
		expect(dialog).not.toBeInTheDocument();
		expect(library.view.books.get('lhod')!.deleted).toBe(false);
		await userEvent.click(screen.getByRole('button', { name: 'More actions' }));
		await userEvent.click(screen.getByRole('menuitem', { name: 'Remove from list' }));
		await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
		expect(library.view.books.get('lhod')!.deleted).toBe(true);
		expect(toasts.current?.message).toBe('Removed The Left Hand of Darkness from your list');
		expect(nav.goto).toHaveBeenCalledWith('/catalog', { replaceState: true });
	});
});
