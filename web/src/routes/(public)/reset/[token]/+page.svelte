<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { get, post } from '#lib/api/client.ts';
	import type { ResetInfo } from '#lib/api/generated/ResetInfo.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import NewPassword from '#lib/components/NewPassword.svelte';

	const token = page.params.token ?? '';
	const path = `/auth/reset/${encodeURIComponent(token)}`;
	let info = $state<ResetInfo | null>(null);
	let password = $state('');
	let passwordOk = $state(false);
	let busy = $state(false);
	let error = $state('');
	let loadError = $state('');

	get<ResetInfo>(path)
		.then((i) => (info = i))
		.catch((e) => (loadError = e.message));

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			await post(path, { password });
			await auth.refresh();
			goto('/', { replaceState: true });
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

{#if loadError}
	<p class="error">{loadError}</p>
{:else if !info}
	<p class="help">Checking the link…</p>
{:else if !info.valid}
	<h1>This link doesn't work</h1>
	<p>
		It has expired or has already been used. Reset links work once, within 24 hours. Ask your admin
		for a new one.
	</p>
{:else}
	<h1>New password for @{info.username}</h1>
	<p class="help">
		This one-time link lets you set a new password. Other devices will be signed out.
		{#if info.two_factor}Two-factor will be turned off; you can turn it on again in Settings.{/if}
		You'll see a note about this under Recovery codes in Settings.
	</p>
	<form onsubmit={submit} style="display:grid;gap:14px">
		<input type="text" name="username" autocomplete="username" value={info.username} hidden />
		<NewPassword bind:password bind:valid={passwordOk} label="New password" id="rs-pw" />
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn primary" type="submit" disabled={busy || !passwordOk}>Set password</button>
	</form>
{/if}
