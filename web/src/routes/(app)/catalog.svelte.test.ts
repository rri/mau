// Catalog (spec §5): every book on the list, filtered, sorted, as a list or grid.
import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Catalog from './catalog/+page.svelte';
import { emptyData } from '#lib/sync/apply.ts';
import { freezeTime, signIn, useLibrary } from '../../test/fixtures.ts';

const titles = () =>
	screen
		.getAllByRole('link')
		.filter((a) => a.getAttribute('href')?.startsWith('/catalog/'))
		.map((a) => a.querySelector('.title, .t')?.textContent);

beforeEach(() => {
	freezeTime();
	signIn();
});

describe('Catalog', () => {
	it('CAT-01 lists every book, most recently added first, with state, tags, and ownership', () => {
		useLibrary();
		render(Catalog);
		expect(screen.getByText('5 books')).toBeInTheDocument();
		expect(titles()).toEqual([
			'Station Eleven',
			'The Left Hand of Darkness',
			'Middlemarch',
			'The Brothers Karamazov',
			'Piranesi'
		]);
		const item = (t: string) => screen.getByText(t, { selector: '.title' }).closest('a')!;
		expect(item('The Left Hand of Darkness')).toHaveTextContent('Active · page 140 of 304');
		expect(item('The Left Hand of Darkness')).toHaveTextContent('#classics#sci-fi');
		expect(
			within(item('The Left Hand of Darkness')).getByTitle('You own a copy')
		).toBeInTheDocument();
		expect(item('Piranesi')).toHaveTextContent('Finished · Oct 28, 2026');
		expect(item('The Brothers Karamazov')).toHaveTextContent('Inactive · page 310 of 796');
		expect(item('Station Eleven')).toHaveTextContent(/Inactive#book club$/);
		expect(within(item('Middlemarch')).queryByTitle('You own a copy')).toBeNull();
	});

	it('CAT-02 state chips, Owned?, and tags filter together; the count shows n of N', async () => {
		useLibrary();
		render(Catalog);
		const chip = (n: string) => screen.getByRole('button', { name: n });
		expect(chip('Active')).toHaveAttribute('aria-pressed', 'true');
		await userEvent.click(chip('Inactive'));
		await userEvent.click(chip('Finished'));
		expect(titles()).toEqual(['The Left Hand of Darkness', 'Middlemarch']);
		expect(screen.getByText('2 of 5 books')).toBeInTheDocument();
		await userEvent.click(screen.getByLabelText('Owned?'));
		expect(titles()).toEqual(['The Left Hand of Darkness']);
		await userEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
		expect(titles()).toHaveLength(5);
		await userEvent.click(
			within(screen.getByRole('group', { name: 'Tags' })).getByRole('button', { name: 'classics' })
		);
		expect(titles()).toEqual(['The Left Hand of Darkness', 'Middlemarch']);
		await userEvent.click(screen.getByRole('button', { name: 'sci-fi' }));
		expect(titles()).toEqual(['The Left Hand of Darkness']);
	});

	it('CAT-03 tags are listed alphabetically', () => {
		useLibrary();
		render(Catalog);
		const tags = within(screen.getByRole('group', { name: 'Tags' })).getAllByRole('button');
		expect(tags.map((t) => t.textContent)).toEqual(['book club', 'classics', 'fantasy', 'sci-fi']);
	});

	it('CAT-04 search matches title or author', async () => {
		useLibrary();
		render(Catalog);
		await userEvent.type(screen.getByRole('searchbox', { name: 'Search your catalog' }), 'clarke');
		expect(titles()).toEqual(['Piranesi']);
		expect(screen.getByText('1 of 5 books')).toBeInTheDocument();
	});

	it('CAT-05 with no state chosen, it asks for one; with no match, it says so', async () => {
		useLibrary();
		render(Catalog);
		for (const n of ['Active', 'Inactive', 'Finished'])
			await userEvent.click(screen.getByRole('button', { name: n }));
		expect(
			screen.getByText('Select Active, Inactive, or Finished to see books.')
		).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
		await userEvent.type(screen.getByRole('searchbox'), 'zzz');
		expect(screen.getByText('No books match these filters.')).toBeInTheDocument();
	});

	it('CAT-06 sorts by title (ignoring articles), author surname, or recently finished; remembered', async () => {
		useLibrary();
		render(Catalog);
		const sort = screen.getByLabelText('Sort');
		await userEvent.selectOptions(sort, 'title');
		expect(titles()).toEqual([
			'The Brothers Karamazov',
			'The Left Hand of Darkness',
			'Middlemarch',
			'Piranesi',
			'Station Eleven'
		]);
		await userEvent.selectOptions(sort, 'author');
		expect(titles()).toEqual([
			'Piranesi',
			'The Brothers Karamazov',
			'Middlemarch',
			'The Left Hand of Darkness',
			'Station Eleven'
		]);
		await userEvent.selectOptions(sort, 'finished');
		expect(titles()[0]).toBe('Piranesi');
		expect(localStorage.getItem('mau.catalog.sort')).toBe('finished');
	});

	it('CAT-07 the grid shows covers with state and owned badges, and a hidden description', async () => {
		useLibrary();
		render(Catalog);
		await userEvent.click(screen.getByRole('button', { name: 'Grid layout' }));
		expect(localStorage.getItem('mau.catalog.layout')).toBe('grid');
		const tile = (t: string) => screen.getByText(t, { selector: '.t' }).closest('a')!;
		expect(tile('Piranesi')).toHaveAccessibleName('Piranesi , Susanna Clarke, finished, owned');
		expect(within(tile('Piranesi')).getByTitle('Finished')).toBeInTheDocument();
		expect(within(tile('Middlemarch')).getByTitle('Active')).toBeInTheDocument();
		expect(tile('Station Eleven')).toHaveAccessibleName('Station Eleven , Emily St. John Mandel');
		// The progress strip is there for every tile (so titles line up), empty when not reading.
		expect(tile('Station Eleven').querySelector('.mini.none')).not.toBeNull();
		expect(tile('Middlemarch').querySelector('.mini span')).toHaveStyle('width: 34%');
	});

	it('CAT-08 an empty catalog offers to add a book; Add is always in the header', () => {
		useLibrary(emptyData());
		render(Catalog);
		expect(screen.getByText('Your catalog is empty.')).toBeInTheDocument();
		expect(
			screen.getAllByRole('link', { name: 'Add a book' }).map((a) => a.getAttribute('href'))
		).toEqual(['/add', '/add']);
	});
});
