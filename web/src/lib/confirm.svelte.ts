// Sensitive actions need a fresh password confirmation. `withReauth` runs the
// action; if the server asks for a confirmation, it opens the dialog and, once
// confirmed, runs the action again.

import { ApiError } from './api/client.ts';

class Confirmation {
	open = $state(false);
	private waiting: { resolve: () => void; reject: (e: Error) => void } | null = null;

	/** Opens the dialog; settles when it's confirmed or cancelled. */
	ask(): Promise<void> {
		this.waiting?.reject(new ApiError(400, 'cancelled', 'Cancelled.'));
		this.open = true;
		return new Promise((resolve, reject) => (this.waiting = { resolve, reject }));
	}

	done() {
		this.open = false;
		this.waiting?.resolve();
		this.waiting = null;
	}

	cancel() {
		this.open = false;
		this.waiting?.reject(new ApiError(400, 'cancelled', 'Cancelled. Nothing was changed.'));
		this.waiting = null;
	}
}

export const confirmation = new Confirmation();

export async function withReauth<T>(action: () => Promise<T>): Promise<T> {
	try {
		return await action();
	} catch (e) {
		if (e instanceof ApiError && e.code === 'reauth_required') {
			await confirmation.ask();
			return action();
		}
		throw e;
	}
}
