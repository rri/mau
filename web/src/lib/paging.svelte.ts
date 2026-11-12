// Long lists draw a page of rows at a time and add the next page as the
// reader scrolls near the end. Search, filters, sorting, and counts still
// cover the whole library (it's all on the device); only drawing is
// incremental, so a library of thousands opens as fast as one of fifty.

export const PAGE_SIZE = 60;

export class Pager {
	limit = $state(PAGE_SIZE);

	/** Back to the first page (the list changed: a new search, filter, or sort). */
	reset() {
		this.limit = PAGE_SIZE;
	}

	more() {
		this.limit += PAGE_SIZE;
	}

	/** Keeps the rows drawn and the scroll position when the reader comes back
	 *  to the list (a SvelteKit snapshot). SvelteKit scrolls before restoring
	 *  snapshots, when only the first page is drawn, so this scrolls again. */
	snapshot = {
		capture: () => ({ limit: this.limit, y: window.scrollY }),
		restore: (s: { limit: number; y: number }) => {
			this.limit = Math.max(PAGE_SIZE, s.limit);
			requestAnimationFrame(() => window.scrollTo(0, s.y));
		}
	};
}
