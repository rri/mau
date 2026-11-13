<script lang="ts">
	import { fmtDate, UNKNOWN_DATE } from '../dates.ts';
	import { progressText, type BookInfo } from '../derive.ts';
	let { info }: { info: BookInfo } = $props();
	const r = $derived(info.reading);
</script>

{#if info.state === 'finished'}
	<!-- Without a known date, just "Finished". -->
	<span class="st fin"
		>Finished{r?.finished_on && r.finished_on !== UNKNOWN_DATE
			? ` · ${fmtDate(r.finished_on)}`
			: ''}</span
	>
{:else if info.state === 'active'}
	<span class="st act">Active{r ? ` · ${progressText(r)}` : ''}</span>
{:else}
	<span class="st">Inactive{r && r.pos > 0 ? ` · ${progressText(r)}` : ''}</span>
{/if}

<style>
	.st {
		font-weight: 500;
	}
	.act,
	.fin {
		color: var(--accent);
	}
</style>
