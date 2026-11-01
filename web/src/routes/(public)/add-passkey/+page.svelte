<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { post } from '#lib/api/client.ts';
	import { createPasskey, deviceName } from '#lib/api/webauthn.ts';
	import type { CeremonyStart } from '#lib/api/generated/CeremonyStart.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import PasskeyIcon from '#lib/components/PasskeyIcon.svelte';

	let name = $state(deviceName());
	let busy = $state(false);
	let error = $state('');
	const left = page.url.searchParams.get('left');

	$effect(() => {
		if (!auth.me) goto('/signin', { replaceState: true });
		else if (!auth.restricted) goto('/', { replaceState: true });
	});

	async function add() {
		busy = true;
		error = '';
		try {
			await createPasskey(
				() => post<CeremonyStart>('/passkeys/start'),
				(challenge, credential) => post('/passkeys/finish', { challenge, credential, name })
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

{#if left !== null}
	<p class="notice">You signed in with a recovery code. {left} codes left.</p>
{/if}
<h1>Add a new passkey</h1>
<p class="help">You need a passkey before you can continue.</p>
<label class="field" for="pk-name"
	>Name for this passkey<input id="pk-name" bind:value={name} /></label
>
<button class="btn primary big" type="button" onclick={add} disabled={busy}>
	<PasskeyIcon />{busy ? 'Waiting for your passkey…' : 'Create passkey'}
</button>
{#if error}<p class="error" role="alert">{error}</p>{/if}
