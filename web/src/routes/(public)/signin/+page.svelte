<script lang="ts">
	import { goto } from '$app/navigation';
	import { post } from '#lib/api/client.ts';
	import type { LoginResult } from '#lib/api/generated/LoginResult.ts';
	import { auth } from '#lib/auth.svelte.ts';

	let username = $state('');
	let password = $state('');
	let code = $state('');
	/** Set after a right password when an authenticator code is needed. */
	let ticket = $state<string | null>(null);
	let busy = $state(false);
	let error = $state('');

	$effect(() => {
		if (auth.me) goto('/', { replaceState: true });
	});

	async function done() {
		await auth.refresh();
		goto('/', { replaceState: true });
	}

	async function signIn(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const r = await post<LoginResult>('/auth/login', { username, password });
			if (r.code_ticket) {
				ticket = r.code_ticket;
				password = '';
			} else await done();
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}

	async function sendCode(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			await post('/auth/login/code', { ticket, code });
			await done();
		} catch (err) {
			error = (err as Error).message;
			if ((err as { code?: string }).code === 'ticket_expired') {
				ticket = null;
				code = '';
			}
		} finally {
			busy = false;
		}
	}
</script>

<h1 class="tagline">your personal library and reading log</h1>

{#if auth.status?.setup_required}
	<p class="notice">
		This instance has no admin yet. Open the setup link printed in the server log to create the
		admin account.
	</p>
{/if}

{#if ticket}
	<form onsubmit={sendCode} style="display:grid;gap:14px">
		<label class="field" for="si-code"
			>Code from your authenticator app<input
				id="si-code"
				class="mono"
				bind:value={code}
				inputmode="numeric"
				autocomplete="one-time-code"
				placeholder="123456"
				required
			/></label
		>
		<p class="help">Lost your phone? Enter one of your recovery codes instead.</p>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn primary" type="submit" disabled={busy}>Sign in</button>
	</form>
	<div class="links">
		<button
			class="link"
			type="button"
			onclick={() => {
				ticket = null;
				error = '';
			}}>Back</button
		>
	</div>
{:else}
	<form onsubmit={signIn} style="display:grid;gap:14px">
		<label class="field" for="si-user"
			>Username<input
				id="si-user"
				bind:value={username}
				autocapitalize="none"
				autocomplete="username"
				spellcheck="false"
				required
			/></label
		>
		<label class="field" for="si-pw"
			>Password<input
				id="si-pw"
				type="password"
				bind:value={password}
				autocomplete="current-password"
				required
			/></label
		>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn primary big" type="submit" disabled={busy}
			>{busy ? 'Signing in…' : 'Sign in'}</button
		>
	</form>

	<div class="links">
		<a class="link" href="/signin/recovery">Forgot your password?</a>
		{#if auth.status?.signup_mode === 'open' && !auth.status?.setup_required}
			<a class="link" href="/signup">Create an account</a>
		{:else}
			<span class="help">New here? Ask for an invite link.</span>
		{/if}
	</div>
{/if}
