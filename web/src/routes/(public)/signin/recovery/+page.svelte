<script lang="ts">
	import { goto } from '$app/navigation';
	import { post } from '#lib/api/client.ts';
	import type { RecoverySignedIn } from '#lib/api/generated/RecoverySignedIn.ts';
	import { auth } from '#lib/auth.svelte.ts';

	let code = $state('');
	let username = $state('');
	let busy = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const r = await post<RecoverySignedIn>('/auth/recovery', { username, code });
			await auth.refresh();
			goto(`/add-passkey?left=${r.codes_left}`, { replaceState: true });
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<h1>Use a recovery code</h1>
<p class="help">Each code works once. After signing in you'll add a new passkey.</p>
<form onsubmit={submit} style="display:grid;gap:14px">
	<label class="field" for="rc-code"
		>Recovery code<input
			id="rc-code"
			class="mono"
			bind:value={code}
			placeholder="xxxxx-xxxxx"
			autocapitalize="none"
			autocomplete="off"
			spellcheck="false"
			required
		/></label
	>
	<label class="field" for="rc-user"
		>Username<input
			id="rc-user"
			bind:value={username}
			autocapitalize="none"
			autocomplete="username"
			required
		/></label
	>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<button class="btn primary" type="submit" disabled={busy}>Continue</button>
</form>
<div class="links">
	<a class="link" href="/signin">Back to sign in</a>
	<span class="help"
		>Lost your codes too? An admin can send you a one-time link to add a passkey.</span
	>
</div>
