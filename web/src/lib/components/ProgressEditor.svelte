<script lang="ts">
	// Editing a reading's position, opened from "page X of Y" or "42%".
	// Pages: page [ ] of [ ]. Percent: [ ] %, and the copy's page count if
	// known. Each can switch to the other; percent → pages needs the page
	// count. Enter, Save, or leaving the fields saves; Escape or Cancel cancels.
	import type { BookInfo } from '../derive.ts';
	import { setPageCount, setPages, switchUnit } from '../actions.ts';

	let { info, ondone }: { info: BookInfo; ondone: () => void } = $props();

	// Opened only for a book with a reading.
	const r = $derived(info.reading!);
	const percent = $derived(r.unit === 'percent');
	// The fields start from the reading as it was when opened.
	// svelte-ignore state_referenced_locally
	let pos = $state(info.reading!.pos);
	// svelte-ignore state_referenced_locally
	let total = $state(info.reading!.total);
	// svelte-ignore state_referenced_locally
	let pages = $state<number | null>(info.reading!.pages);
	let done = false;

	const int = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? Math.floor(v) : null);

	function save() {
		if (done) return;
		done = true;
		ondone();
		const p = int(pos) ?? r.pos;
		if (percent) {
			const n = Math.max(0, int(pages) ?? 0);
			if (n !== (r.pages ?? 0)) setPageCount(info, n);
			if (p !== r.pos) setPages(info, p);
		} else {
			const t = (int(total) ?? 0) >= 1 ? int(total)! : r.total;
			if (p !== r.pos || t !== r.total) setPages(info, p, t);
		}
	}

	function cancel() {
		done = true;
		ondone();
	}

	function toPercent() {
		done = true;
		ondone();
		switchUnit(info, 'percent');
	}

	function toPages() {
		const n = int(pages);
		if (!n || n < 1) return;
		done = true;
		ondone();
		switchUnit(info, 'pages', n, Math.round(((int(pos) ?? r.pos) * n) / 100));
	}

	function keys(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			save();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			cancel();
		}
	}

	function focusOut(e: FocusEvent) {
		const group = e.currentTarget as HTMLElement;
		if (e.relatedTarget instanceof Node && group.contains(e.relatedTarget)) return;
		save();
	}

	function focusFirst(node: HTMLInputElement) {
		node.focus();
		node.select();
	}

	// It can open near the bottom of the screen; bring it into view
	// (scroll-padding keeps it clear of the phone tab bar).
	function reveal(node: HTMLElement) {
		requestAnimationFrame(() => node.scrollIntoView?.({ block: 'nearest' }));
	}
</script>

<span
	class="edit"
	role="group"
	aria-label={percent ? 'Edit progress' : 'Edit pages'}
	onfocusout={focusOut}
	use:reveal
>
	<span class="line">
		{#if percent}
			<input
				type="number"
				inputmode="numeric"
				min="0"
				max="100"
				bind:value={pos}
				onkeydown={keys}
				aria-label="Percent read"
				use:focusFirst
			/> %
		{:else}
			page <input
				type="number"
				inputmode="numeric"
				min="0"
				bind:value={pos}
				onkeydown={keys}
				aria-label="Current page"
				use:focusFirst
			/>
			of
			<input
				type="number"
				inputmode="numeric"
				min="1"
				bind:value={total}
				onkeydown={keys}
				aria-label="Total pages"
			/>
		{/if}
		<button class="btn sm primary" type="button" onclick={save}>Save</button>
		<button class="btn sm" type="button" onclick={cancel}>Cancel</button>
	</span>
	{#if percent}
		<span class="line">
			<label
				>Pages in your copy <input
					type="number"
					inputmode="numeric"
					min="1"
					bind:value={pages}
					onkeydown={keys}
					placeholder="unknown"
				/></label
			>
			<button class="link" type="button" onclick={toPages} disabled={!int(pages)}
				>Count pages instead</button
			>
		</span>
	{:else}
		<span class="line">
			<button class="link" type="button" onclick={toPercent}>Track by percent instead</button>
		</span>
	{/if}
</span>

<style>
	.edit {
		display: grid;
		justify-items: start;
		gap: 8px;
		color: var(--ink);
	}
	.line {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 6px 10px;
		align-items: center;
	}
	label {
		display: inline-flex;
		gap: 6px;
		align-items: center;
		color: var(--muted);
	}
	input {
		width: 72px;
		padding: 2px 6px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		background: var(--surface);
		color: var(--ink);
		font: inherit;
	}
	.link:disabled {
		color: var(--muted);
		text-decoration: none;
		cursor: default;
	}
</style>
