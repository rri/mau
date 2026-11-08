<script lang="ts">
	import { goto } from '$app/navigation';
	import { post } from '#lib/api/client.ts';
	import type { RecoveryDone } from '#lib/api/generated/RecoveryDone.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import NewPassword from '#lib/components/NewPassword.svelte';

	let code = $state('');
	let username = $state('');
	let password = $state('');
	let passwordOk = $state(false);
	let busy = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const r = await post<RecoveryDone>('/auth/recovery', { username, code, password });
			await auth.refresh();
			goto('/', { replaceState: true });
			toasts.show(
				`New password set. ${r.codes_left} recovery ${r.codes_left === 1 ? 'code' : 'codes'} left.`
			);
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<h1>Set a new password</h1>
<p class="help">
	Use one of the recovery codes you saved when you signed up. Each code works once. Other devices
	will be signed out.
</p>
<form onsubmit={submit} style="display:grid;gap:14px">
	<label class="field" for="rc-user"
		>Username<input
			id="rc-user"
			bind:value={username}
			autocapitalize="none"
			autocomplete="username"
			spellcheck="false"
			required
		/></label
	>
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
	<NewPassword bind:password bind:valid={passwordOk} label="New password" id="rc-pw" />
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<button class="btn primary" type="submit" disabled={busy || !passwordOk}>Set password</button>
</form>
<div class="links">
	<a class="link" href="/signin">Back to sign in</a>
	<span class="help">Lost your codes too? An admin can give you a one-time link instead.</span>
</div>
