<script lang="ts">
	import { goto } from '$app/navigation';
	import { post } from '#lib/api/client.ts';
	import { passkeysSupported, usePasskey } from '#lib/api/webauthn.ts';
	import type { CeremonyStart } from '#lib/api/generated/CeremonyStart.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import PasskeyIcon from '#lib/components/PasskeyIcon.svelte';

	let busy = $state(false);
	let error = $state('');

	$effect(() => {
		if (auth.me && !auth.restricted) goto('/', { replaceState: true });
		else if (auth.status?.setup_required) error = '';
	});

	async function signIn() {
		busy = true;
		error = '';
		try {
			await usePasskey(
				() => post<CeremonyStart>('/auth/login/start'),
				(challenge, credential) => post('/auth/login/finish', { challenge, credential })
			);
			await auth.refresh();
			goto('/', { replaceState: true });
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<p class="brand">mau</p>
<p class="tagline">Your library and reading log</p>

{#if auth.status?.setup_required}
	<p class="notice">
		This instance has no admin yet. Open the setup link printed in the server log to create the
		admin account.
	</p>
{/if}

{#if !passkeysSupported()}
	<p class="error">This browser doesn't support passkeys.</p>
{/if}

<button
	class="btn primary big"
	type="button"
	onclick={signIn}
	disabled={busy || !passkeysSupported()}
>
	<PasskeyIcon />{busy ? 'Waiting for your passkey…' : 'Sign in with a passkey'}
</button>
{#if error}<p class="error" role="alert">{error}</p>{/if}

<div class="links">
	<a class="link" href="/signin/recovery">Use a recovery code</a>
	{#if auth.status?.signup_mode === 'open' && !auth.status?.setup_required}
		<a class="link" href="/signup">Create an account</a>
	{:else}
		<span class="help">New here? Ask for an invite link.</span>
	{/if}
</div>
