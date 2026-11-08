<script lang="ts">
	import { put } from '#lib/api/client.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { withReauth } from '#lib/confirm.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';
	import NewPassword from '#lib/components/NewPassword.svelte';

	let password = $state('');
	let valid = $state(false);
	let busy = $state(false);
	let error = $state('');
	// Remounts the fields after a change, which clears them.
	let round = $state(0);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			await withReauth(() => put('/account/password', { password }));
			toasts.show('Password changed');
			password = '';
			round++;
		} catch (err) {
			if ((err as { code?: string }).code !== 'cancelled') error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<PageHead title="Password" />

<div class="sec">
	<p class="help">
		You'll confirm with your current password first. Devices that are signed in stay signed in; sign
		them out under Sessions if you need to.
	</p>
	<form onsubmit={submit} style="display:grid;gap:14px">
		<input type="text" name="username" autocomplete="username" value={auth.me?.username} hidden />
		{#key round}
			<NewPassword bind:password bind:valid label="New password" id="pw" />
		{/key}
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="row-end">
			<button class="btn primary" type="submit" disabled={busy || !valid}>Change password</button>
		</div>
	</form>
</div>
