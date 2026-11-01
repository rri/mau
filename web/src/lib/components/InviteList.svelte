<script lang="ts">
	// Pending invites with create, copy, and revoke.
	import { onMount } from 'svelte';
	import { del, get, post } from '../api/client.ts';
	import type { Invites } from '../api/generated/Invites.ts';
	import type { Link } from '../api/generated/Link.ts';
	import { dateOfMs, daysBetween } from '../dates.ts';
	import { auth } from '../auth.svelte.ts';
	import { toasts } from '../toast.svelte.ts';

	let data = $state<Invites | null>(null);
	let created = $state<string | null>(null);
	let error = $state('');
	const tz = auth.me?.time_zone ?? 'UTC';

	async function load() {
		try {
			data = await get<Invites>('/invites');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	async function create() {
		error = '';
		try {
			const l = await post<Link>('/invites');
			created = l.url;
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			toasts.show('Link copied');
		} catch {
			toasts.show(text);
		}
	}
	async function revoke(id: string) {
		await del(`/invites/${id}`);
		toasts.show('Invite revoked');
		load();
	}
	const expires = (ms: number) => {
		const d = daysBetween(dateOfMs(Date.now(), tz), dateOfMs(ms, tz));
		return d <= 0 ? 'today' : d === 1 ? 'tomorrow' : `in ${d} days`;
	};
</script>

<p class="help">
	Each link creates one account and expires after 7 days. The link is shown only once, so copy it
	now.
</p>
{#if created}
	<div class="card">
		<div class="mono link-text">{created}</div>
		<div class="acts">
			<button class="btn sm primary" type="button" onclick={() => copy(created!)}>Copy link</button>
		</div>
	</div>
{/if}
<div class="cards">
	{#each data?.pending ?? [] as inv (inv.id)}
		<div class="card">
			<div class="top">
				<span class="name">Invite from @{inv.created_by ?? 'deleted user'}</span>
			</div>
			<div class="meta">Expires {expires(inv.expires_at)}</div>
			<div class="acts">
				<button class="btn sm danger-ghost" type="button" onclick={() => revoke(inv.id)}
					>Revoke</button
				>
			</div>
		</div>
	{:else}
		<p class="help">No pending invites.</p>
	{/each}
</div>
{#if data?.can_create}
	<div class="row-end">
		<button class="btn primary" type="button" onclick={create}>Create invite link</button>
	</div>
{:else if data}
	<p class="help">You can't create more invites right now.</p>
{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
	.link-text {
		font-size: 14px;
		overflow-wrap: anywhere;
	}
</style>
