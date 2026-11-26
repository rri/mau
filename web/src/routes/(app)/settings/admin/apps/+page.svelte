<script lang="ts">
	// Admin › Apps: every app someone on this instance has connected, by
	// address, with how many people use it (never who). Nothing is approved
	// in advance; an admin can block an address that turns out to be bad.
	import { onMount } from 'svelte';
	import { get, post } from '#lib/api/client.ts';
	import type { AppRow } from '#lib/api/generated/AppRow.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { ago, dateOfMs, fmtDate } from '#lib/dates.ts';
	import { modal } from '#lib/modal.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	const tz = auth.me?.time_zone ?? 'UTC';
	const today = dateOfMs(Date.now(), tz);

	let apps = $state<AppRow[] | null>(null);
	let error = $state('');
	let blocking = $state<AppRow | null>(null);

	async function load() {
		try {
			apps = await get<AppRow[]>('/admin/apps');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	const people = (n: number) => (n === 1 ? '1 person' : `${n} people`);
	/** Parts joined with "·", lowercase after the dots and capitalized at the
	 *  start, as status lines are everywhere in the app. */
	const line = (parts: string[]) => {
		const s = parts.join(' · ');
		return s.charAt(0).toUpperCase() + s.slice(1);
	};

	async function setBlocked(a: AppRow, blocked: boolean) {
		error = '';
		try {
			await post('/admin/apps/block', { client_id: a.client_id, blocked });
			toasts.show(blocked ? `Blocked ${a.host}` : `Unblocked ${a.host}`);
			blocking = null;
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
</script>

<PageHead title="Apps" />

<div class="sec">
	<p class="help">
		Apps people have connected to their accounts with "Sign in with mau". Apps don't need approval;
		each is known by its address. Block one that misleads people or misuses access.
	</p>
	{#if apps?.length}
		<div class="cards">
			{#each apps as a (a.client_id)}
				<div class="card">
					<div class="top">
						<span class="name host">{a.host}</span>
						{#if a.blocked}<span class="pill bad">Blocked</span>{/if}
					</div>
					{#if a.name}<div class="meta">Calls itself “{a.name}”</div>{/if}
					{#if a.client_id !== a.origin + '/'}<div class="meta mono">{a.client_id}</div>{/if}
					<div class="meta">
						{line([
							...(a.blocked ? [] : [`connected by ${people(a.people)}`]),
							`first connected ${fmtDate(dateOfMs(a.first_seen, tz))}`,
							...(a.last_used_at === null
								? []
								: [`last used ${ago(dateOfMs(a.last_used_at, tz), today)}`])
						])}
					</div>
					<div class="acts">
						{#if a.blocked}
							<button class="btn sm" type="button" onclick={() => setBlocked(a, false)}
								>Unblock</button
							>
						{:else}
							<button class="btn sm danger-ghost" type="button" onclick={() => (blocking = a)}
								>Block</button
							>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{:else if apps}
		<p class="help">No apps connected yet.</p>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

{#if blocking}
	<div class="dialog-back" role="presentation">
		<div
			class="dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="bl-h"
			use:modal={() => (blocking = null)}
		>
			<h3 id="bl-h">Block {blocking.host}?</h3>
			<p>
				{blocking.people
					? `${people(blocking.people)} will be disconnected from it at once.`
					: 'Nobody is connected to it now.'}
				Nobody on this instance can connect any app at {blocking.origin} until you unblock it, and unblocking
				doesn't reconnect anyone.
			</p>
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (blocking = null)}>Cancel</button>
				<button class="btn danger" type="button" onclick={() => setBlocked(blocking!, true)}
					>Block</button
				>
			</div>
		</div>
	</div>
{/if}

<style>
	.host {
		overflow-wrap: anywhere;
	}
</style>
