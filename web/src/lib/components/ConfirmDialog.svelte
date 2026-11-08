<script lang="ts">
	// Asks for the password (and an authenticator code when two-factor is on)
	// before a sensitive action. Opened by `withReauth`.
	import { post } from '#lib/api/client.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { confirmation } from '#lib/confirm.svelte.ts';

	let password = $state('');
	let code = $state('');
	let busy = $state(false);
	let error = $state('');

	$effect(() => {
		if (confirmation.open) {
			password = '';
			code = '';
			error = '';
		}
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			await post('/auth/confirm', { password, code: auth.me?.two_factor ? code : null });
			confirmation.done();
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

{#if confirmation.open}
	<div class="dialog-back" role="presentation">
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="cf-h">
			<form onsubmit={submit}>
				<h3 id="cf-h">Confirm it's you</h3>
				<p>Enter your password to continue. You won't be asked again for 10 minutes.</p>
				<input
					type="text"
					name="username"
					autocomplete="username"
					value={auth.me?.username ?? ''}
					hidden
				/>
				<label class="field" for="cf-pw"
					>Password<input
						id="cf-pw"
						type="password"
						autocomplete="current-password"
						bind:value={password}
						required
					/></label
				>
				{#if auth.me?.two_factor}
					<label class="field" for="cf-code"
						>Authenticator code<input
							id="cf-code"
							class="mono"
							bind:value={code}
							inputmode="numeric"
							autocomplete="one-time-code"
							placeholder="123456"
							required
						/></label
					>
					<p class="help">Lost your phone? A recovery code works here too.</p>
				{/if}
				{#if error}<p class="error" role="alert">{error}</p>{/if}
				<div class="row-end">
					<button class="btn" type="button" onclick={() => confirmation.cancel()}>Cancel</button>
					<button class="btn primary" type="submit" disabled={busy}>Confirm</button>
				</div>
			</form>
		</div>
	</div>
{/if}

<style>
	form {
		display: grid;
		gap: 12px;
	}
</style>
