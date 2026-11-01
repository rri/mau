<script lang="ts">
	import { auth } from '#lib/auth.svelte.ts';
	import SignupFlow from '#lib/components/SignupFlow.svelte';
</script>

{#if auth.status?.signup_mode === 'open' && !auth.status.setup_required}
	<SignupFlow intro="Create your mau account." />
{:else}
	<h1>Signup isn't open</h1>
	<p>You need an invite link to create an account on this instance.</p>
	<div class="links"><a class="link" href="/signin">Back to sign in</a></div>
{/if}
