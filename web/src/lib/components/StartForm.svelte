<script lang="ts">
	// Starting a never-read book: its page count, with the catalog's as a
	// hint, or percent when the reader doesn't know it (or doesn't want
	// pages). Inline in Reading rows and on the book page, and in the grid's
	// dialog.
	import type { Start } from '../actions.ts';
	import type { BookInfo } from '../derive.ts';

	let {
		info,
		onstart,
		oncancel,
		heading = null
	}: {
		info: BookInfo;
		onstart: (start: Start) => void;
		oncancel: () => void;
		/** A heading above the question (the dialog's). */
		heading?: string | null;
	} = $props();

	let total = $state('');
	let error = $state('');
	const id = $derived(`start-${info.book.id}`);

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const n = Number(total);
		if (!Number.isInteger(n) || n < 1) {
			error = 'Enter the number of pages in your copy, or track by percent.';
			return;
		}
		error = '';
		onstart({ unit: 'pages', total: n });
	}

	function focus(node: HTMLInputElement) {
		node.focus();
	}
</script>

<form class="start" onsubmit={submit}>
	{#if heading}<h3 id="{id}-h">{heading}</h3>{/if}
	<label class="field" for={id}
		>How many pages does your copy have?
		<input
			{id}
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
	<div class="buttons">
		<button class="link" type="button" onclick={() => onstart({ unit: 'percent' })}
			>Don't know? Track by percent</button
		>
		<span class="row-end">
			<button class="btn" type="button" onclick={oncancel}>Cancel</button>
			<button class="btn primary" type="submit">Start reading</button>
		</span>
	</div>
</form>

<style>
	.start {
		display: grid;
		gap: 10px;
	}
	h3 {
		margin: 0;
	}
	.hint {
		justify-self: start;
		font-size: 14px;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 10px 16px;
		align-items: center;
		justify-content: space-between;
	}
	.buttons > .link {
		font-size: 14px;
	}
	.buttons .row-end {
		margin-left: auto;
	}
</style>
