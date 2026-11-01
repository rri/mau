<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from '#lib/api/client.ts';
	import type { Stats } from '#lib/api/generated/Stats.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let s = $state<Stats | null>(null);
	let error = $state('');
	onMount(async () => {
		try {
			s = await get<Stats>('/admin/stats');
		} catch (e) {
			error = (e as Error).message;
		}
	});
	const mb = (b: number) =>
		b < 1024 * 1024
			? `${Math.max(1, Math.round(b / 1024))} KB`
			: `${(b / 1024 / 1024).toFixed(1)} MB`;
	const n = (x: number) => x.toLocaleString('en-US');
</script>

<PageHead title="Instance stats" />
<div class="sec">
	{#if s}
		<div class="stats">
			<div class="stat"><b>{n(s.users)}</b><span>Users</span></div>
			<div class="stat"><b>{n(s.books)}</b><span>Books on lists</span></div>
			<div class="stat"><b>{n(s.shared_works)}</b><span>Shared catalog works</span></div>
			<div class="stat"><b>{n(s.private_works)}</b><span>Private manual entries</span></div>
			<div class="stat"><b>{mb(s.cover_bytes)}</b><span>Cover images</span></div>
			<div class="stat"><b>{mb(s.database_bytes)}</b><span>Database</span></div>
		</div>
	{/if}
	{#if error}<p class="error">{error}</p>{/if}
</div>

<style>
	.stats {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 8px;
	}
	.stat {
		border: 1px solid var(--line);
		border-radius: 12px;
		padding: 10px 12px;
		background: var(--surface);
	}
	.stat b {
		display: block;
		font-size: 24px;
		font-variant-numeric: tabular-nums;
	}
	.stat span {
		font-size: 14px;
		color: var(--muted);
	}
</style>
