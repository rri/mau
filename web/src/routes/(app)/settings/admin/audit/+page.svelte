<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from '#lib/api/client.ts';
	import type { AuditEntry } from '#lib/api/generated/AuditEntry.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let entries = $state<AuditEntry[]>([]);
	let loaded = $state(false);
	let more = $state(true);
	let error = $state('');
	const fmt = new Intl.DateTimeFormat('en-US', {
		month: 'short',
		day: 'numeric',
		year: 'numeric',
		hour: '2-digit',
		minute: '2-digit'
	});

	async function load(before?: number) {
		try {
			const page = await get<AuditEntry[]>(`/admin/audit${before ? `?before=${before}` : ''}`);
			entries = before ? [...entries, ...page] : page;
			more = page.length === 200;
		} catch (e) {
			error = (e as Error).message;
		} finally {
			loaded = true;
		}
	}
	onMount(() => load());
</script>

<PageHead title="Audit log" />
<div class="sec">
	{#if entries.length}
		<ul class="log">
			{#each entries as e, i (i)}
				<li>
					<time>{fmt.format(new Date(e.at))}</time><span><b>{e.actor_name}</b> {e.action}</span>
				</li>
			{/each}
		</ul>
	{:else if loaded && !error}
		<p class="help">Nothing recorded yet.</p>
	{/if}
	{#if more && entries.length}<button
			class="btn"
			type="button"
			onclick={() => load(entries.at(-1)!.id)}>Older</button
		>{/if}
	{#if error}<p class="error">{error}</p>{/if}
</div>

<style>
	.log {
		list-style: none;
		margin: 0;
		padding: 0;
		font-size: 15px;
	}
	.log li {
		display: grid;
		grid-template-columns: 170px minmax(0, 1fr);
		gap: 10px;
		padding: 8px 0;
		border-bottom: 1px solid var(--line);
	}
	time {
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	b {
		font-weight: 600;
	}
</style>
