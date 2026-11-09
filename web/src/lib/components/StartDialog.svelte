<script lang="ts">
	// Asks for the total pages before a never-started book's first activation
	// from the Reading grid. The list asks inline in the row instead.
	import type { BookInfo } from '../derive.ts';

	let {
		info,
		onstart,
		oncancel
	}: { info: BookInfo; onstart: (total: number) => void; oncancel: () => void } = $props();

	let total = $state('');
	let error = $state('');

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const n = Number(total);
		if (!Number.isInteger(n) || n < 1) {
			error = 'Enter the number of pages in your copy.';
			return;
		}
		onstart(n);
	}

	function focus(node: HTMLInputElement) {
		node.focus();
	}
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && oncancel()} />

<div class="dialog-back" role="presentation">
	<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="start-h">
		<form onsubmit={submit}>
			<h3 id="start-h">{info.title}</h3>
			<label class="field" for="start-total"
				>How many pages does your copy have?
				<input
					id="start-total"
					type="number"
					inputmode="numeric"
					min="1"
					bind:value={total}
					placeholder="Total pages"
					use:focus
				/></label
			>
			{#if info.catalogPages}
				<button class="link hint" type="button" onclick={() => (total = String(info.catalogPages))}
					>Catalog lists {info.catalogPages} pages</button
				>
			{/if}
			{#if error}<p class="error" role="alert">{error}</p>{/if}
			<div class="row-end">
				<button class="btn" type="button" onclick={oncancel}>Cancel</button>
				<button class="btn primary" type="submit">Start reading</button>
			</div>
		</form>
	</div>
</div>

<style>
	form {
		display: grid;
		gap: 12px;
	}
	.hint {
		justify-self: start;
		font-size: 14px;
	}
</style>
