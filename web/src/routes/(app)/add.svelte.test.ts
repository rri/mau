// Adding books (spec §7): catalog search, the confirm sheet, and manual entry.
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Add from './add/+page.svelte';
import type { SearchResult } from '#lib/api/generated/SearchResult.ts';
import { liveBooks } from '#lib/derive.ts';
import { library } from '#lib/library.svelte.ts';
import { origin } from '#lib/origin.svelte.ts';
import { toasts } from '#lib/toast.svelte.ts';
import { freezeTime, mockApi, signIn, useLibrary, work } from '../../test/fixtures.ts';

const ISBN = '9780441478125';

const result = (over: Partial<SearchResult> = {}): SearchResult => ({
	book_id: null,
	provider: 'openlibrary',
	source_id: 'OL1W',
	title: 'Dune',
	authors: ['Frank Herbert'],
	first_year: 1965,
	cover_url: null,
	edition_count: 2,
	editions: [
		{
			source_id: 'OL1M',
			publisher: 'Ace',
			binding: 'Paperback',
			year: 1990,
			isbn13: ISBN,
			pages: 535,
			cover_url: null
		},
		{
			source_id: 'OL2M',
			publisher: 'Chilton',
			binding: 'Hardcover',
			year: 1965,
			isbn13: null,
			pages: 412,
			cover_url: null
		}
	],
	...over
});

const duneWork = () => {
	const w = work({ id: 'dune', title: 'Dune', author: 'Frank Herbert', year: 1965, pages: 535 });
	w.editions[0].isbn13 = ISBN;
	return w;
};

const search = async (q: string) => {
	const box = screen.getByRole('searchbox', { name: 'Search by title, author, or ISBN' });
	await userEvent.clear(box);
	await userEvent.type(box, q + '{Enter}');
};

beforeEach(() => {
	freezeTime();
	signIn();
	useLibrary();
	origin.visit('/catalog');
});

describe('Add a book: search', () => {
	it('ADD-25 without a Google Books key, a tip says adding one gets better matches', () => {
		const { unmount } = render(Add);
		const tip = screen.getByRole('complementary', { name: 'Tip' });
		expect(tip).toHaveTextContent('Get better matches with your own Google Books key.');
		expect(within(tip).getByRole('link', { name: 'Add a key in Settings' })).toHaveAttribute(
			'href',
			'/settings/google-books'
		);
		unmount();
		signIn({ google_books: true });
		render(Add);
		expect(screen.queryByRole('complementary', { name: 'Tip' })).toBeNull();
	});

	it('ADD-01 starts with help text, the trail, and a way to enter a book by hand', () => {
		origin.visit('/reading');
		render(Add);
		const crumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
		expect(within(crumbs).getAllByRole('link', { name: 'Reading' })[0]).toHaveAttribute(
			'href',
			'/reading'
		);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Add a book');
		expect(screen.getByText(/Type a title, author, or ISBN and press Enter/)).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Enter it manually' })).toBeInTheDocument();
	});

	it('ADD-02 results show cover, title, authors, year and editions; the status line names sources', async () => {
		mockApi({
			'GET /catalog/search': {
				isbn: null,
				results: [
					result(),
					result({ source_id: 'OL9W', title: 'Dune Messiah', first_year: null, edition_count: 1 })
				],
				sources: ['Open Library', 'Google Books'],
				errors: ['Google Books: quota exceeded']
			}
		});
		render(Add);
		await search('dune');
		expect(
			screen.getByText(
				'2 results from Open Library, then Google Books · Google Books: quota exceeded'
			)
		).toBeInTheDocument();
		const items = screen.getAllByRole('button', { name: /Dune/ });
		expect(items[0]).toHaveTextContent('Dune Frank Herbert 1965 · 2 editions');
		expect(items[1]).toHaveTextContent('Year unknown · 1 edition');
	});

	it('ADD-03 a mistyped ISBN is caught before searching', async () => {
		const fetch = mockApi({});
		render(Add);
		await search('9780441478126');
		expect(
			screen.getByText("That ISBN's check digit doesn't match. Check for a typo.")
		).toBeInTheDocument();
		expect(fetch).not.toHaveBeenCalled();
	});

	it('ADD-04 no results says so; a failed search shows the reason', async () => {
		mockApi({ 'GET /catalog/search': { isbn: null, results: [], sources: [], errors: [] } });
		render(Add);
		await search('zzz');
		expect(screen.getByText('No books found for "zzz".')).toBeInTheDocument();
		mockApi({
			'GET /catalog/search': new Response('{"error":"x","message":"Search failed."}', {
				status: 502
			})
		});
		await search('zzz');
		expect(screen.getByText('Search failed.')).toBeInTheDocument();
	});

	it('ADD-05 offline, it says search needs a connection but manual entry works', () => {
		library.online = false;
		render(Add);
		expect(
			screen.getByText(
				'Searching the catalog needs a connection. You can still enter a book by hand.'
			)
		).toBeInTheDocument();
	});

	it('ADD-06 a book already on the list is marked', async () => {
		mockApi({
			'GET /catalog/search': {
				isbn: null,
				results: [result({ book_id: 'lhod' })],
				sources: [],
				errors: []
			}
		});
		render(Add);
		await search('dune');
		expect(screen.getByText('On your list')).toBeInTheDocument();
	});
});

describe('Add a book: confirm sheet', () => {
	it('ADD-10 an ISBN with one match opens the sheet with that edition and its binding chosen', async () => {
		mockApi({
			'GET /catalog/search': {
				isbn: ISBN,
				results: [result()],
				sources: ['Open Library'],
				errors: []
			},
			'GET /catalog/editions': result().editions
		});
		render(Add);
		await search('978-0-441-47812-5');
		const sheet = await screen.findByRole('dialog', { name: 'Add Dune' });
		expect(screen.getByText(`ISBN ${ISBN} · 1 result from Open Library`)).toBeInTheDocument();
		expect(within(sheet).getByLabelText('Edition')).toHaveDisplayValue('Ace, Paperback, 1990');
		await userEvent.click(within(sheet).getByLabelText('I own a copy'));
		expect(within(sheet).getByLabelText('Format')).toHaveDisplayValue('Paperback');
	});

	it('ADD-11 adding puts the book at the top of the inactive books, with tags and a copy', async () => {
		const fetch = mockApi({
			'GET /catalog/search': { isbn: null, results: [result()], sources: [], errors: [] },
			'GET /catalog/editions': result().editions,
			'POST /catalog/ensure': duneWork()
		});
		render(Add);
		await search('dune');
		await userEvent.click(screen.getByRole('button', { name: /Dune/ }));
		const sheet = await screen.findByRole('dialog', { name: 'Add Dune' });
		await userEvent.selectOptions(within(sheet).getByLabelText('Edition'), 'Ace, Paperback, 1990');
		await userEvent.click(within(sheet).getByLabelText('I own a copy'));
		await userEvent.selectOptions(within(sheet).getByLabelText('Format'), 'Hardcover');
		await userEvent.type(within(sheet).getByLabelText('Location'), 'Study');
		await userEvent.type(within(sheet).getByRole('combobox', { name: 'Add tag' }), 'sci-fi{Enter}');
		await userEvent.click(within(sheet).getByRole('button', { name: 'Add to my list' }));

		const ensure = fetch.mock.calls.find(([u]) => String(u).startsWith('/api/catalog/ensure'))!;
		expect(JSON.parse(String(ensure[1]!.body))).toEqual({
			provider: 'openlibrary',
			source_id: 'OL1W',
			isbn: ISBN
		});
		const dune = liveBooks(library.view).find((b) => b.title === 'Dune')!;
		expect(dune.state).toBe('inactive');
		expect(dune.tags).toEqual(['sci-fi']);
		expect(dune.owned).toBe(true);
		const copy = [...library.view.copies.values()].find((c) => c.book_id === dune.book.id)!;
		expect(copy).toMatchObject({ format: 'Hardcover', location: 'Study', edition_id: 'e-dune' });
		// Top of the inactive books: before Station Eleven (rank D).
		expect(dune.book.rank < 'D').toBe(true);
		expect(toasts.current?.message).toBe(
			'Added Dune to the top of your inactive books, with a copy'
		);
		expect(toasts.current?.action?.label).toBe('View');
		expect(screen.queryByRole('dialog')).toBeNull();
	});

	it('ADD-12 for a book already on the list, the sheet adds another copy', async () => {
		mockApi({
			'GET /catalog/search': {
				isbn: null,
				results: [result({ book_id: 'lhod', title: 'The Left Hand of Darkness' })],
				sources: [],
				errors: []
			},
			'GET /catalog/editions': [],
			'POST /catalog/ensure': work({
				id: 'lhod',
				title: 'The Left Hand of Darkness',
				author: 'Ursula K. Le Guin'
			})
		});
		render(Add);
		await search('left hand');
		await userEvent.click(screen.getByRole('button', { name: /Left Hand/ }));
		const sheet = await screen.findByRole('dialog');
		expect(sheet).toHaveTextContent(
			'This book is already on your list. Add another copy, or open its page.'
		);
		expect(within(sheet).getByRole('link', { name: 'open its page' })).toHaveAttribute(
			'href',
			'/catalog/lhod'
		);
		expect(within(sheet).queryByLabelText('I own a copy')).toBeNull();
		expect(within(sheet).queryByRole('combobox', { name: 'Add tag' })).toBeNull();
		await userEvent.click(within(sheet).getByRole('button', { name: 'Add copy' }));
		expect([...library.view.copies.values()].filter((c) => c.book_id === 'lhod')).toHaveLength(2);
		expect(toasts.current?.message).toBe('Added another copy of The Left Hand of Darkness');
	});

	it('ADD-13 the sheet closes with Cancel, Escape, or a tap outside it', async () => {
		mockApi({
			'GET /catalog/search': { isbn: null, results: [result()], sources: [], errors: [] },
			'GET /catalog/editions': []
		});
		render(Add);
		await search('dune');
		const openSheet = async () => {
			await userEvent.click(screen.getByRole('button', { name: /Dune/ }));
			return screen.findByRole('dialog');
		};
		await userEvent.click(within(await openSheet()).getByRole('button', { name: 'Cancel' }));
		expect(screen.queryByRole('dialog')).toBeNull();
		await openSheet();
		await userEvent.keyboard('{Escape}');
		expect(screen.queryByRole('dialog')).toBeNull();
		const sheet = await openSheet();
		await userEvent.click(sheet.parentElement!);
		expect(screen.queryByRole('dialog')).toBeNull();
	});

	it('ADD-14 adding offline explains that the catalog needs a connection', async () => {
		mockApi({
			'GET /catalog/search': { isbn: null, results: [result()], sources: [], errors: [] },
			'GET /catalog/editions': [],
			'POST /catalog/ensure': () => {
				throw new TypeError('Failed to fetch');
			}
		});
		render(Add);
		await search('dune');
		await userEvent.click(screen.getByRole('button', { name: /Dune/ }));
		await userEvent.click(await screen.findByRole('button', { name: 'Add to my list' }));
		expect(screen.getByRole('alert')).toHaveTextContent(
			'Adding from the catalog needs a connection.'
		);
	});
});

describe('Add a book: manual entry', () => {
	it('ADD-20 Enter it manually carries the search over as the title (or the ISBN)', async () => {
		mockApi({ 'GET /catalog/search': { isbn: null, results: [], sources: [], errors: [] } });
		render(Add);
		await search('My Notebook');
		await userEvent.click(screen.getByRole('button', { name: 'Enter it manually' }));
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Enter a book');
		expect(screen.getByLabelText('Title (required)')).toHaveValue('My Notebook');
		const crumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
		await userEvent.click(within(crumbs).getAllByRole('button', { name: 'Add a book' })[0]);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Add a book');
	});

	it('ADD-21 needs a title and an author; checks the ISBN and year', async () => {
		render(Add);
		await userEvent.click(screen.getByRole('button', { name: 'Enter it manually' }));
		await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
		expect(screen.getByRole('alert')).toHaveTextContent('Enter at least a title and an author.');
		await userEvent.type(screen.getByLabelText('Title (required)'), 'Zine');
		await userEvent.type(screen.getByLabelText('Author (required)'), 'Me');
		await userEvent.type(screen.getByLabelText('First published year'), 'soon');
		await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
		expect(screen.getByRole('alert')).toHaveTextContent('Enter the year as a number, like 1998.');
		await userEvent.clear(screen.getByLabelText('First published year'));
		await userEvent.click(screen.getByText('Edition details (optional)'));
		await userEvent.type(screen.getByLabelText('ISBN'), '123');
		await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
		expect(screen.getByRole('alert')).toHaveTextContent("That ISBN doesn't look right.");
	});

	it('ADD-22 a new entry is private, goes to the top of the inactive books, and can have more people', async () => {
		mockApi({ 'GET /catalog/similar': [] });
		render(Add);
		await userEvent.click(screen.getByRole('button', { name: 'Enter it manually' }));
		await userEvent.type(screen.getByLabelText('Title (required)'), 'Zine');
		await userEvent.type(screen.getByLabelText('Author (required)'), 'Me');
		await userEvent.click(screen.getByRole('button', { name: '+ Add another person' }));
		await userEvent.type(screen.getByLabelText('Name'), 'Friend');
		await userEvent.selectOptions(screen.getByLabelText('Role'), 'illustrator');
		await userEvent.type(screen.getByLabelText('First published year'), '2024');
		await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
		const zine = liveBooks(library.view).find((b) => b.title === 'Zine')!;
		expect(zine.work!.people).toEqual([
			{ name: 'Me', role: 'author' },
			{ name: 'Friend', role: 'illustrator' }
		]);
		expect(zine.work!.first_year).toBe(2024);
		expect(zine.state).toBe('inactive');
		expect(toasts.current?.message).toBe(
			'Added Zine (private to you) to the top of your inactive books'
		);
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Add a book');
	});

	it('ADD-23 a likely match in the shared catalog is offered first', async () => {
		mockApi({
			'GET /catalog/similar': [
				{ id: 'w-dune', title: 'Dune', authors: ['Frank Herbert'], first_year: 1965, cover: null }
			],
			'GET /catalog/works/w-dune': duneWork()
		});
		render(Add);
		await userEvent.click(screen.getByRole('button', { name: 'Enter it manually' }));
		await userEvent.type(screen.getByLabelText('Title (required)'), 'Dune');
		await userEvent.type(screen.getByLabelText('Author (required)'), 'Frank Herbert');
		await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
		expect(screen.getByText('Is it one of these?')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'No, add my book' })).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: /Frank Herbert · 1965/ }));
		expect(await screen.findByRole('dialog', { name: 'Add Dune' })).toBeInTheDocument();
	});

	it('ADD-24 Cancel goes back to the search', async () => {
		render(Add);
		await userEvent.click(screen.getByRole('button', { name: 'Enter it manually' }));
		await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
		expect(screen.getByRole('searchbox')).toBeInTheDocument();
	});
});
