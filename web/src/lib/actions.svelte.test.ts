// The reading controls (spec §4 ordering rules): what each does to the
// library, the toast it shows, and its undo.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	finish,
	moveBook,
	pause,
	plusOne,
	resume,
	setPages,
	startLike,
	startReading,
	switchUnit
} from './actions.ts';
import { bookInfo, readingList } from './derive.ts';
import { cloneData } from './sync/apply.ts';
import { library } from './library.svelte.ts';
import { toasts } from './toast.svelte.ts';
import { freezeTime, signIn, TODAY, useLibrary } from '../test/fixtures.ts';

const info = (id: string) => bookInfo(library.view, library.view.books.get(id)!);
const order = () => readingList(library.view).map((b) => `${b.book.id}:${b.state}`);

beforeEach(() => {
	freezeTime();
	signIn();
	useLibrary();
});
afterEach(() => {
	vi.useRealTimers();
	toasts.dismiss();
});

describe('READ-ACT: reading actions', () => {
	it('READ-ACT-01 + adds one page today, with an undo toast', () => {
		plusOne(info('lhod'));
		expect(info('lhod').reading!.pos).toBe(141);
		expect(toasts.current?.message).toBe('+1 page on The Left Hand of Darkness');
		expect(toasts.current?.action?.label).toBe('Undo');
		const ev = [...library.view.events.values()].filter((e) => e.reading_id === '00r-lhod').at(-1)!;
		expect(ev).toMatchObject({ local_date: TODAY, from_page: 140, to_page: 141 });
		toasts.runAction();
		expect(info('lhod').reading!.pos).toBe(140);
	});

	it('READ-ACT-02 setting pages moves forward or back and can change the total', () => {
		setPages(info('lhod'), 100);
		expect(toasts.current?.message).toBe('Moved back to page 100');
		setPages(info('lhod'), 120, 320);
		expect(info('lhod').reading).toMatchObject({ pos: 120, total: 320 });
		expect(toasts.current?.message).toBe('Moved to page 120, total 304 → 320');
		toasts.runAction();
		expect(info('lhod').reading).toMatchObject({ pos: 100, total: 304 });
	});

	it('READ-ACT-03 positions are clamped to 0…total', () => {
		setPages(info('lhod'), -5);
		expect(info('lhod').reading!.pos).toBe(0);
	});

	it('READ-ACT-04 reaching the last page finishes the book and deactivates it; undo restores', () => {
		setPages(info('lhod'), 304);
		const b = info('lhod');
		expect(b.state).toBe('finished');
		expect(b.reading!.finished_on).toBe(TODAY);
		expect(b.book.active).toBe(false);
		expect(toasts.current?.message).toBe('Finished The Left Hand of Darkness');
		toasts.runAction();
		expect(info('lhod').state).toBe('active');
		expect(info('lhod').reading!.pos).toBe(140);
	});

	it('READ-ACT-05 an unchanged position does nothing', () => {
		setPages(info('lhod'), 140);
		expect(toasts.current).toBeNull();
	});

	it('READ-ACT-06 pause moves the book to the top of the inactive books', () => {
		pause(info('mm'));
		expect(order()).toEqual(['lhod:active', 'mm:inactive', 'se:inactive', 'bk:inactive']);
		expect(toasts.current?.message).toBe('Paused Middlemarch');
		toasts.runAction();
		expect(order()).toEqual(['lhod:active', 'mm:active', 'se:inactive', 'bk:inactive']);
	});

	it('READ-ACT-07 resume puts the book at the end of the active books, at its page', () => {
		resume(info('bk'));
		expect(order()).toEqual(['lhod:active', 'mm:active', 'bk:active', 'se:inactive']);
		expect(toasts.current?.message).toBe('Resumed The Brothers Karamazov at page 310');
		toasts.runAction();
		expect(order()).toEqual(['lhod:active', 'mm:active', 'se:inactive', 'bk:inactive']);
	});

	it('READ-ACT-08 starting a never-read book creates a reading at page 0 and activates it', () => {
		startReading(info('se'), { unit: 'pages', total: 333 });
		const b = info('se');
		expect(b.state).toBe('active');
		expect(b.reading).toMatchObject({ pos: 0, total: 333, started_on: TODAY });
		expect(toasts.current?.message).toBe('Started Station Eleven');
		toasts.runAction();
		expect(info('se').reading).toBeNull();
		expect(info('se').state).toBe('inactive');
	});

	it('READ-ACT-09 a re-read starts a new reading; the finished one is kept', () => {
		startReading(info('pir'), startLike(info('pir')));
		const b = info('pir');
		expect(b.state).toBe('active');
		expect(b.readings).toHaveLength(2);
		expect(toasts.current?.message).toBe('Started re-reading Piranesi at page 0');
	});
});

describe('READ-DRAG: drag to reorder', () => {
	const setup = vi.fn();
	// Drop positions count the other books: lhod, mm | se, bk without the dragged one.

	it('READ-DRAG-01 an active book moves within the active books', () => {
		moveBook(info('mm'), 0, setup);
		expect(order().slice(0, 2)).toEqual(['mm:active', 'lhod:active']);
		expect(toasts.current?.message).toBe('Moved Middlemarch');
	});

	it('READ-DRAG-02 an active book dropped among inactive books stays active, at the end of active', () => {
		moveBook(info('lhod'), 3, setup);
		expect(order()).toEqual(['mm:active', 'lhod:active', 'se:inactive', 'bk:inactive']);
		expect(toasts.current?.message).toBe(
			'The Left Hand of Darkness is still active, moved to the end of the active list'
		);
	});

	it('READ-DRAG-03 an inactive book with a reading dropped between active books resumes there', () => {
		moveBook(info('bk'), 1, setup);
		expect(order()).toEqual(['lhod:active', 'bk:active', 'mm:active', 'se:inactive']);
	});

	it('READ-DRAG-04 a never-started book dropped between active books asks for its pages first', () => {
		moveBook(info('se'), 0, setup);
		expect(setup).toHaveBeenCalledWith(expect.any(String));
		expect(info('se').state).toBe('inactive');
		const rank = setup.mock.calls[0][0] as string;
		expect(rank < info('lhod').book.rank).toBe(true);
	});

	it('READ-DRAG-05 the edge between the groups changes nobody’s state', () => {
		moveBook(info('bk'), 2, setup);
		expect(order()).toEqual(['lhod:active', 'mm:active', 'bk:inactive', 'se:inactive']);
	});

	it('READ-ACT-10 Finish marks an active book finished today, where it was; undo takes it back', () => {
		const events = () => [...library.view.events.values()].filter((e) => !e.deleted).length;
		const before = events();
		finish(info('lhod'));
		const b = info('lhod');
		expect(b.state).toBe('finished');
		expect(b.reading).toMatchObject({ finished_on: TODAY, pos: 140 });
		expect(b.book.active).toBe(false);
		// Nothing counts as read today.
		expect(events()).toBe(before);
		expect(toasts.current?.message).toBe('Finished The Left Hand of Darkness');
		toasts.runAction();
		expect(info('lhod').state).toBe('active');
		expect(info('lhod').reading!.finished_on).toBeNull();
	});

	it('READ-ACT-11 Finish on a never-started book adds a reading finished today (percent without a page count)', () => {
		finish(info('se'));
		expect(info('se').reading).toMatchObject({ unit: 'pages', total: 333, finished_on: TODAY });
		toasts.runAction();
		expect(info('se').reading).toBeNull();
		expect(info('se').state).toBe('inactive');

		const d = cloneData(library.view);
		const w = d.works.get(d.books.get('se')!.work_id)!;
		d.works.set(w.id, { ...w, editions: [] });
		useLibrary(d);
		finish(info('se'));
		expect(info('se').reading).toMatchObject({ unit: 'percent', total: 100, finished_on: TODAY });
	});

	it('READ-ACT-12 percent: the up arrow adds 1%, and 100% finishes the book', () => {
		startReading(info('se'), { unit: 'percent' });
		expect(info('se').reading).toMatchObject({ unit: 'percent', total: 100, pos: 0, pages: null });
		plusOne(info('se'));
		expect(info('se').reading!.pos).toBe(1);
		expect(toasts.current?.message).toBe('+1% on Station Eleven');
		const ev = [...library.view.events.values()].find(
			(e) => e.reading_id === info('se').reading!.id
		)!;
		expect(ev).toMatchObject({ unit: 'percent', from_page: 0, to_page: 1 });
		setPages(info('se'), 100);
		expect(info('se').state).toBe('finished');
	});

	it('READ-ACT-13 switching units moves the position and keeps the page count; undo switches back', () => {
		switchUnit(info('lhod'), 'percent');
		expect(info('lhod').reading).toMatchObject({
			unit: 'percent',
			pos: 46,
			total: 100,
			pages: 304
		});
		expect(toasts.current?.message).toBe('The Left Hand of Darkness now counts by percent, at 46%');
		toasts.runAction();
		expect(info('lhod').reading).toMatchObject({ unit: 'pages', pos: 140, total: 304 });

		switchUnit(info('lhod'), 'percent');
		switchUnit(info('lhod'), 'pages', 320, 150);
		expect(info('lhod').reading).toMatchObject({
			unit: 'pages',
			pos: 150,
			total: 320,
			pages: null
		});
	});
});
