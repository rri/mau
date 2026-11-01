<script lang="ts">
	import { onMount } from 'svelte';
	import { del, get, post } from '#lib/api/client.ts';
	import type { SessionInfo } from '#lib/api/generated/SessionInfo.ts';
	import { ago, dateOfMs } from '#lib/dates.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let sessions = $state<SessionInfo[]>([]);
	let error = $state('');
	const tz = auth.me?.time_zone ?? 'UTC';
	const today = dateOfMs(Date.now(), tz);

	async function load() {
		try {
			sessions = await get<SessionInfo[]>('/sessions');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	function lastActive(s: SessionInfo) {
		if (s.current) return 'Active now';
		const mins = (Date.now() - s.last_seen_at) / 60000;
		if (mins < 60) return 'Active in the last hour';
		return `Last active ${ago(dateOfMs(s.last_seen_at, tz), today)}`;
	}

	async function revoke(s: SessionInfo) {
		await del(`/sessions/${s.id}`);
		toasts.show(`Signed out ${s.device}`);
		load();
	}
	async function revokeOthers() {
		await post('/sessions/revoke-others');
		toasts.show('Signed out all other sessions');
		load();
	}
</script>

<PageHead title="Sessions" />

<div class="sec">
	<p class="help">
		Browsers signed in to your account. Each stays signed in for 30 days after its last visit.
	</p>
	<div class="cards">
		{#each sessions as s (s.id)}
			<div class="card">
				<div class="top">
					<span class="name">{s.device}</span>{#if s.current}<span class="pill acc"
							>This device</span
						>{/if}
				</div>
				<div class="meta">{lastActive(s)}</div>
				{#if !s.current}<div class="acts">
						<button class="btn sm danger-ghost" type="button" onclick={() => revoke(s)}
							>Sign out</button
						>
					</div>{/if}
			</div>
		{/each}
	</div>
	{#if sessions.length > 1}
		<div class="row-end">
			<button class="btn" type="button" onclick={revokeOthers}>Sign out all other sessions</button>
		</div>
	{/if}
	{#if error}<p class="error">{error}</p>{/if}
</div>
