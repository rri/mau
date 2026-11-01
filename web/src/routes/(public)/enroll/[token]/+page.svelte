<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { get, post } from '#lib/api/client.ts';
	import type { EnrollInfo } from '#lib/api/generated/EnrollInfo.ts';
	import { auth } from '#lib/auth.svelte.ts';

	const token = page.params.token ?? '';
	let info = $state<EnrollInfo | null>(null);
	let busy = $state(false);
	let error = $state('');

	get<EnrollInfo>(`/auth/enroll/${encodeURIComponent(token)}`)
		.then((i) => (info = i))
		.catch((e) => (error = e.message));

	async function use() {
		busy = true;
		try {
			await post(`/auth/enroll/${encodeURIComponent(token)}`);
			await auth.refresh();
			goto('/add-passkey', { replaceState: true });
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

{#if error}
	<p class="error">{error}</p>
{:else if !info}
	<p class="help">Checking the link…</p>
{:else if !info.valid}
	<h1>This link doesn't work</h1>
	<p>It has expired or has already been used. Links from an admin work once, within 24 hours.</p>
{:else}
	<h1>Add a passkey for @{info.username}</h1>
	<p>
		An admin sent you this one-time link because you lost access. You'll be asked to create a new
		passkey.
	</p>
	<p class="help">You'll see a note about this under Recovery codes in Settings.</p>
	<button class="btn primary" type="button" onclick={use} disabled={busy}>Continue</button>
{/if}
