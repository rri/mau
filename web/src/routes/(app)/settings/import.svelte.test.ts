// Import & export (spec §8): read a file, match, review, import; export links.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Import from './import/+page.svelte';
import { library } from '#lib/library.svelte.ts';
import { mockApi, signIn, useLibrary } from '../../../test/fixtures.ts';

const row = (index: number, kind: string, title: string, over: object = {}) => ({
	index,
	title,
	authors: ['A. Author'],
	summary: 'read 2026-10-20',
	tags: [],
	kind,
	candidates:
		kind === 'uncertain'
			? [
					{ title: `${title} (Ace)`, authors: ['A. Author'], first_year: 1990 },
					{ title: `${title} (Tor)`, authors: ['B'], first_year: null }
				]
			: [],
	...over
});
const ROWS = [
	row(0, 'matched', 'Dune', { tags: ['sf', 'classic'] }),
	row(1, 'uncertain', 'Emma'),
	row(2, 'unmatched', 'My Zine', { summary: 'not finished → inactive' }),
	row(3, 'dupe', 'Middlemarch')
];
const file = (name = 'goodreads.csv') =>
	new File(['Title,Author\nDune,Frank Herbert\n'], name, { type: 'text/csv' });
/** Chooses a file the way a browser's file input does (settable, then change). */
async function upload(input: HTMLElement, f: File) {
	let list: FileList = Object.assign([f], {
		item: (i: number) => (i === 0 ? f : null)
	}) as unknown as FileList;
	Object.defineProperty(input, 'files', {
		configurable: true,
		get: () => list,
		set: (v: FileList | null) => {
			if (v) list = v;
		}
	});
	await fireEvent.change(input);
}
const sentTo = (fetch: ReturnType<typeof mockApi>, path: string) =>
	fetch.mock.calls
		.filter(([u, i]) => String(u) === '/api' + path && i?.body)
		.map(([, i]) => JSON.parse(String(i!.body)));

beforeEach(() => {
	signIn();
	useLibrary();
});

describe('Import', () => {
	it('IMP-08 without a Google Books key, a tip says imports match better with one', async () => {
		mockApi({ 'GET /import/current': { job: null } });
		const { unmount } = render(Import);
		const tip = await screen.findByRole('complementary', { name: 'Tip' });
		expect(tip).toHaveTextContent('Imports match your books on Open Library until you add');
		unmount();
		signIn({ google_books: true });
		render(Import);
		await screen.findByLabelText('Import from');
		expect(screen.queryByRole('complementary', { name: 'Tip' })).toBeNull();
	});

	it('IMP-01 Read file needs a file', async () => {
		mockApi({ 'GET /import/current': { job: null } });
		render(Import);
		await userEvent.click(screen.getByRole('button', { name: 'Read file' }));
		expect(screen.getByRole('alert')).toHaveTextContent('Choose the file you exported.');
	});

	it('IMP-02 a file is matched, then reviewed by tab; the import button counts what will come in', async () => {
		let committed = false;
		const fetch = mockApi({
			'GET /import/current': { job: null },
			'POST /import': { job: 'j1', total: 4 },
			'GET /import/j1': () =>
				committed
					? {
							total: 4,
							done: 4,
							rows: null,
							refused: 0,
							refused_reason: null,
							saving: null,
							result: { imported: 2, skipped: 2 },
							error: null
						}
					: {
							total: 4,
							done: 4,
							rows: ROWS,
							refused: 1,
							refused_reason: 'Quota exceeded.',
							saving: null,
							result: null,
							error: null
						},
			'POST /import/j1/commit': () => ((committed = true), { done: 0, total: 2 }),
			'POST /sync': {
				changes: { books: [], tags: [], copies: [], readings: [], events: [], works: [], seq: 0 },
				rejected: []
			}
		});
		vi.spyOn(library, 'sync').mockResolvedValue();
		render(Import);
		await userEvent.selectOptions(screen.getByLabelText('Import from'), 'goodreads');
		await upload(screen.getByLabelText('File'), file());
		await userEvent.click(screen.getByRole('button', { name: 'Read file' }));
		expect(sentTo(fetch, '/import')[0]).toMatchObject({ source: 'goodreads', mapping: null });
		expect(await screen.findByText(/4 books in the file/)).toBeInTheDocument();
		expect(
			screen.getByText(
				/Google Books refused 1\s+search, so that book was looked up on Open Library instead. Quota exceeded./
			)
		).toBeInTheDocument();

		const tabs = screen.getAllByRole('button', {
			name: /^(Matched|Check match|Not found|Already on list) \d/
		});
		expect(tabs.map((t) => t.textContent)).toEqual([
			'Matched 1',
			'Check match 1',
			'Not found 1',
			'Already on list 1'
		]);
		expect(screen.getByText('Dune').closest('li')).toHaveTextContent(
			'A. Author · read 2026-10-20 · #sf #classic'
		);
		const go = () => screen.getByRole('button', { name: /^Import \d+ books?$/ });
		expect(go()).toHaveTextContent('Import 3 books');

		await userEvent.click(screen.getByRole('button', { name: 'Check match 1' }));
		const pick = screen.getByRole('combobox', { name: 'Choose a match' });
		expect(
			within(pick)
				.getAllByRole('option')
				.map((o) => o.textContent)
		).toEqual([
			'Emma (Ace) — A. Author (1990)',
			'Emma (Tor) — B',
			'Not this book: add as entered',
			'Skip'
		]);
		await userEvent.selectOptions(pick, 'Emma (Tor) — B');
		await userEvent.click(screen.getByRole('button', { name: 'Not found 1' }));
		await userEvent.selectOptions(screen.getByRole('combobox', { name: 'What to do' }), 'Skip');
		expect(go()).toHaveTextContent('Import 2 books');
		await userEvent.click(screen.getByRole('button', { name: 'Already on list 1' }));
		expect(screen.getByText('Middlemarch').closest('li')).toHaveTextContent('Skipped');

		await userEvent.click(go());
		expect(sentTo(fetch, '/import/j1/commit')[0]).toEqual({
			choices: [
				{ index: 1, action: 'match', candidate: 1 },
				{ index: 2, action: 'skip', candidate: null }
			]
		});
		expect(
			await screen.findByText(/Imported 2\s+books, skipped 2/, {}, { timeout: 3000 })
		).toBeInTheDocument();
		expect(library.sync).toHaveBeenCalled();
	});

	it('IMP-03 a row can be corrected and searched again; it stays in view on its tab', async () => {
		const fetch = mockApi({
			'GET /import/current': { job: 'j2' },
			'GET /import/j2': {
				total: 1,
				done: 1,
				rows: [row(0, 'unmatched', 'Dnue')],
				refused: 0,
				refused_reason: null,
				saving: null,
				result: null,
				error: null
			},
			'POST /import/j2/rows/0': row(0, 'matched', 'Dune')
		});
		render(Import);
		// An import that was in progress is picked up again.
		await userEvent.click(await screen.findByRole('button', { name: 'Edit' }));
		const title = screen.getByLabelText('Title');
		await userEvent.clear(title);
		await userEvent.type(title, 'Dune');
		await userEvent.clear(screen.getByLabelText('Authors, separated by ;'));
		await userEvent.type(
			screen.getByLabelText('Authors, separated by ;'),
			'Frank Herbert; Brian Herbert'
		);
		await userEvent.click(screen.getByRole('button', { name: 'Search' }));
		expect(sentTo(fetch, '/import/j2/rows/0')[0]).toEqual({
			title: 'Dune',
			authors: ['Frank Herbert', ' Brian Herbert']
		});
		expect(await screen.findByText('Dune')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Matched 1' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Not found 0' })).toHaveAttribute(
			'aria-pressed',
			'true'
		);
	});

	it('IMP-04 Other CSV maps columns first, guessing from the headers', async () => {
		const fetch = mockApi({
			'GET /import/current': { job: null },
			'POST /import/preview': {
				headers: ['Book Title', 'Writer', 'ISBN13', 'Date Read', 'Notes'],
				fields: [
					'title',
					'authors',
					'isbn',
					'state',
					'finished',
					'rating',
					'tags',
					'owned',
					'pages'
				],
				samples: [['Dune', 'Frank Herbert', '9780441478125', '2026-01-01', 'x']]
			},
			'POST /import': { job: 'j3', total: 1 },
			'GET /import/j3': {
				total: 1,
				done: 0,
				rows: null,
				refused: 0,
				refused_reason: null,
				saving: null,
				result: null,
				error: null
			}
		});
		render(Import);
		await userEvent.selectOptions(screen.getByLabelText('Import from'), 'csv');
		await upload(screen.getByLabelText('File'), file('mine.csv'));
		await userEvent.click(screen.getByRole('button', { name: 'Read file' }));
		expect(await screen.findByLabelText('mau field for Book Title')).toHaveDisplayValue('Title');
		expect(screen.getByLabelText('mau field for Writer')).toHaveDisplayValue('Authors');
		expect(screen.getByLabelText('mau field for ISBN13')).toHaveDisplayValue('ISBN');
		expect(screen.getByLabelText('mau field for Date Read')).toHaveDisplayValue('Finished date');
		expect(screen.getByLabelText('mau field for Notes')).toHaveDisplayValue('(ignore)');
		await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
		expect(sentTo(fetch, '/import')[0].mapping).toEqual({
			title: 'Book Title',
			authors: 'Writer',
			isbn: 'ISBN13',
			finished: 'Date Read'
		});
		expect(await screen.findByText(/Matching books with the catalog… 0 of 1/)).toBeInTheDocument();
	});

	it('IMP-05 offline, importing is unavailable; export links are always there', () => {
		mockApi({ 'GET /import/current': { job: null } });
		library.online = false;
		render(Import);
		expect(screen.getByText('Importing needs a connection.')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Spreadsheet (CSV)' })).toHaveAttribute(
			'href',
			'/api/export.csv'
		);
		expect(screen.getByRole('link', { name: 'Full export (JSON)' })).toHaveAttribute(
			'href',
			'/api/export.json'
		);
	});

	it('IMP-06 a failed save reports why', async () => {
		mockApi({
			'GET /import/current': { job: 'j4' },
			'GET /import/j4': {
				total: 1,
				done: 1,
				rows: null,
				refused: 0,
				refused_reason: null,
				saving: null,
				result: null,
				error: 'disk full'
			}
		});
		render(Import);
		expect(await screen.findByRole('alert')).toHaveTextContent('Saving stopped: disk full');
		expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
	});
});
