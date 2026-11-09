<script lang="ts">
	import { page } from '$app/state';
	import { post } from '#lib/api/client.ts';
	import type { Valid } from '#lib/api/generated/Valid.ts';
	import SignupFlow from '#lib/components/SignupFlow.svelte';

	const token = page.params.token ?? '';
	let valid = $state<boolean | null>(null);
	let started = $state(false);

	post<Valid>('/auth/setup-check', { token })
		.then((v) => (valid = v.valid))
		.catch(() => (valid = false));
</script>

{#if valid === null}
	<p class="help">Checking the setup link…</p>
{:else if !valid}
	<h1>This setup link doesn't work</h1>
	<p>
		The admin account already exists, or the server has restarted since this link was printed. If
		there's still no admin, restart mau and use the new link from its log.
	</p>
	<div class="links"><a class="link" href="/signin">Go to sign in</a></div>
{:else if !started}
	<h1 class="brand">mau</h1>
	<h1>Set up this instance</h1>
	<p>
		You opened the one-time setup link from the server log. The account you create next will be the
		admin.
	</p>
	<p class="help">This link stops working once the admin account exists.</p>
	<button class="btn primary" type="button" onclick={() => (started = true)}
		>Create the admin account</button
	>
{:else}
	<SignupFlow
		setup={token}
		intro="This is the first account on this instance. It will be the admin."
	/>
{/if}
