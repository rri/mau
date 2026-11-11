// Which main section the reader came from (Home, Reading, or Catalog), so a
// book or the Add page can start its breadcrumb trail there. Kept for the tab
// (sessionStorage) so a reload doesn't lose it.

export type Section = { label: string; href: string };

const SECTIONS: Record<string, string> = {
	'/': 'Home',
	'/reading': 'Reading',
	'/catalog': 'Catalog'
};
const KEY = 'mau.origin';
const DEFAULT: Section = { label: 'Catalog', href: '/catalog' };

function load(): Section {
	try {
		const href = sessionStorage.getItem(KEY);
		if (href && SECTIONS[href]) return { label: SECTIONS[href], href };
	} catch {
		// storage blocked: fall back to the catalog
	}
	return DEFAULT;
}

class Origin {
	section = $state<Section>(load());

	/** Call on every navigation; only the main sections count. */
	visit(pathname: string) {
		const label = SECTIONS[pathname];
		if (!label) return;
		this.section = { label, href: pathname };
		try {
			sessionStorage.setItem(KEY, pathname);
		} catch {
			// not remembered across reloads; fine
		}
	}
}

export const origin = new Origin();
