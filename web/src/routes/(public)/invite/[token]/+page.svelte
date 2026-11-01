<script lang="ts">
	import { page } from '$app/state';
	import { get } from '#lib/api/client.ts';
	import type { InviteInfo } from '#lib/api/generated/InviteInfo.ts';
	import SignupFlow from '#lib/components/SignupFlow.svelte';

	const token = page.params.token ?? '';
	let info = $state<InviteInfo | null>(null);
	let error = $state('');

	get<InviteInfo>(`/auth/invite/${encodeURIComponent(token)}`)
		.then((i) => (info = i))
		.catch((e) => (error = e.message));
</script>

{#if error}
	<p class="error">{error}</p>
{:else if !info}
	<p class="help">Checking your invite…</p>
{:else if !info.valid}
	<h1>This invite link doesn't work</h1>
	<p>It has expired or has already been used. Invite links work once, within 7 days.</p>
	<p>Ask whoever invited you for a new link.</p>
	<div class="links"><a class="link" href="/signin">I already have an account</a></div>
{:else}
	<SignupFlow
		invite={token}
		intro={info.inviter ? `${info.inviter} invited you to mau.` : "You've been invited to mau."}
	/>
{/if}
