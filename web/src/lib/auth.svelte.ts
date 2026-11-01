// Who is signed in. Cached in localStorage so the app opens offline.

import { get } from './api/client.ts';
import type { AuthStatus } from './api/generated/AuthStatus.ts';
import type { Me } from './api/generated/Me.ts';

const CACHE_KEY = 'mau.me';

class Auth {
	me = $state<Me | null>(null);
	restricted = $state(false);
	status = $state<AuthStatus | null>(null);
	/** Set once the first status check (or cache read) has finished. */
	ready = $state(false);
	offline = $state(false);

	async refresh(): Promise<void> {
		try {
			const s = await get<AuthStatus>('/auth/status');
			this.status = s;
			this.me = s.me;
			this.restricted = s.restricted;
			this.offline = false;
			if (s.me && !s.restricted) localStorage.setItem(CACHE_KEY, JSON.stringify(s.me));
			else localStorage.removeItem(CACHE_KEY);
		} catch {
			// Offline: trust the cached identity until the server says otherwise.
			this.offline = true;
			const cached = localStorage.getItem(CACHE_KEY);
			this.me = cached ? (JSON.parse(cached) as Me) : null;
		}
		this.ready = true;
	}

	setMe(me: Me) {
		this.me = me;
		localStorage.setItem(CACHE_KEY, JSON.stringify(me));
	}

	clear() {
		this.me = null;
		this.restricted = false;
		localStorage.removeItem(CACHE_KEY);
	}
}

export const auth = new Auth();
