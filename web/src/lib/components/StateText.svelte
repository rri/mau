<script lang="ts">
	import { fmtDate } from '../dates.ts';
	import type { BookInfo } from '../derive.ts';
	let { info }: { info: BookInfo } = $props();
	const r = $derived(info.reading);
</script>

{#if info.state === 'finished' && r?.finished_on}
	<span class="st fin">Finished · {fmtDate(r.finished_on)}</span>
{:else if info.state === 'active'}
	<span class="st act">Active{r ? ` · page ${r.pos} of ${r.total}` : ''}</span>
{:else}
	<span class="st">Inactive{r && r.pos > 0 ? ` · page ${r.pos} of ${r.total}` : ''}</span>
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
