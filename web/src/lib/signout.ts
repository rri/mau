// Signing out: the server session ends and this device's copy is deleted.

import { goto } from '$app/navigation';
import { post } from './api/client.ts';
import { auth } from './auth.svelte.ts';
import { library } from './library.svelte.ts';

export async function signOut() {
	try {
		await post('/auth/logout');
	} catch {
		// Offline: the cookie stays, but this device forgets everything anyway.
	}
	await library.wipe();
	auth.clear();
	goto('/signin', { replaceState: true });
}
