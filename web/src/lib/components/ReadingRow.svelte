<script lang="ts">
	// One book in the Reading view.
	import { ago, dateOfMs } from '../dates.ts';
	import { lastReadDate, pagesOn, type BookInfo } from '../derive.ts';
	import { library } from '../library.svelte.ts';
	import { auth } from '../auth.svelte.ts';
	import { pause, plusOne, resume, setPages } from '../actions.ts';
	import Cover from './Cover.svelte';

	let {
		info,
		today,
		editing = false,
		setup = false,
		dragging = false,
		dy = 0,
		onedit,
		onsetup,
		oncancel,
		onstart,
		ongrab
	}: {
		info: BookInfo;
		today: string;
		editing?: boolean;
		setup?: boolean;
		dragging?: boolean;
		dy?: number;
		onedit: () => void;
		onsetup: () => void;
		oncancel: () => void;
		onstart: (total: number) => void;
		ongrab: (e: PointerEvent) => void;
	} = $props();

	const r = $derived(info.reading);
	const pct = $derived(r ? Math.round((100 * r.pos) / r.total) : 0);
	const todayPages = $derived(r ? pagesOn(library.view, r.id, today) : 0);
	const lastRead = $derived(r ? lastReadDate(library.view, r.id) : null);
	const addedOn = $derived(dateOfMs(info.book.added_at, auth.me?.time_zone ?? 'UTC'));

	let editPos = $state(0);
	let editTotal = $state(0);
	let setupTotal = $state('');
	let setupError = $state('');

	$effect(() => {
		if (editing && r) {
			editPos = r.pos;
			editTotal = r.total;
		}
	});

	function saveEdit() {
		if (!r) return;
		const total = Number.isFinite(editTotal) && editTotal >= 1 ? Math.floor(editTotal) : r.total;
		const pos = Number.isFinite(editPos) ? Math.floor(editPos) : r.pos;
		oncancel();
		if (pos !== r.pos || total !== r.total) setPages(info, pos, total);
	}

	function editKey(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			saveEdit();
		} else if (e.key === 'Escape') oncancel();
	}

	function editFocusOut(e: FocusEvent) {
		const group = e.currentTarget as HTMLElement;
		if (e.relatedTarget instanceof Node && group.contains(e.relatedTarget)) return;
		if (editing) saveEdit();
	}

	function startSetup(e: SubmitEvent) {
		e.preventDefault();
		const n = Number(setupTotal);
		if (!Number.isInteger(n) || n < 1) {
			setupError = 'Enter the number of pages in your copy.';
			return;
		}
		setupError = '';
		onstart(n);
	}

	function focusFirst(node: HTMLInputElement) {
		node.focus();
		node.select();
	}
</script>

<li
	class="row"
	class:active={info.state === 'active'}
	class:idle={info.state !== 'active'}
	class:dragging
	style={dragging ? `transform: translateY(${dy}px)` : undefined}
>
	<button
		class="handle"
		type="button"
		aria-label="Drag to reorder {info.title}"
		title="Drag to reorder"
		onpointerdown={ongrab}
	>
		<svg width="10" height="16" viewBox="0 0 10 16" aria-hidden="true"
			><g fill="currentColor"
				><circle cx="3" cy="3" r="1.3" /><circle cx="7" cy="3" r="1.3" /><circle
					cx="3"
					cy="8"
					r="1.3"
				/><circle cx="7" cy="8" r="1.3" /><circle cx="3" cy="13" r="1.3" /><circle
					cx="7"
					cy="13"
					r="1.3"
				/></g
			></svg
		>
	</button>
	<a class="cover" href="/catalog/{info.book.id}" tabindex="-1" aria-hidden="true"
		><Cover title={info.title} author={info.authors[0] ?? ''} cover={info.cover} /></a
	>
	<div class="main">
		<a class="title" href="/catalog/{info.book.id}">{info.title}</a>
		<div class="author">{info.authors.join(', ')}</div>
		<div class="bar" class:empty={!r}><span style="width:{pct}%"></span></div>
		<div class="meta">
			{#if !r}
				<span>Not started</span><span>added {ago(addedOn, today)}</span>
			{:else if editing}
				<span class="edit" role="group" aria-label="Edit pages" onfocusout={editFocusOut}>
					page <input
						type="number"
						inputmode="numeric"
						min="0"
						bind:value={editPos}
						onkeydown={editKey}
						aria-label="Current page"
						use:focusFirst
					/>
					of
					<input
						type="number"
						inputmode="numeric"
						min="1"
						bind:value={editTotal}
						onkeydown={editKey}
						aria-label="Total pages"
					/>
					<button class="btn sm primary" type="button" onclick={saveEdit}>Save</button>
				</span>
			{:else}
				<button
					class="pos"
					type="button"
					onclick={onedit}
					aria-label="Edit pages, now page {r.pos} of {r.total}">page {r.pos} of {r.total}</button
				>
				{#if todayPages > 0}
					<span class="today">+{todayPages} {todayPages === 1 ? 'page' : 'pages'} today</span>
				{:else if r.pos === 0 && !lastRead}
					<span>no progress yet</span>
				{:else}
					<span>last read {ago(lastRead ?? r.started_on, today)}</span>
				{/if}
			{/if}
		</div>
	</div>
	{#if !setup}
		<div class="actions">
			{#if info.state === 'active'}
				<button
					class="pause"
					type="button"
					onclick={() => pause(info)}
					aria-label="Pause {info.title}"
					title="Pause"
					><svg width="11" height="11" viewBox="0 0 14 14" aria-hidden="true"
						><rect x="3" y="2" width="3" height="10" rx="1" fill="currentColor" /><rect
							x="8"
							y="2"
							width="3"
							height="10"
							rx="1"
							fill="currentColor"
						/></svg
					></button
				>
				<button
					class="plus"
					type="button"
					onclick={() => plusOne(info)}
					aria-label="Add 1 page to {info.title}">+</button
				>
			{:else}
				<button
					class="play"
					type="button"
					onclick={() => (r ? resume(info) : onsetup())}
					aria-label="{r ? 'Resume' : 'Start'} {info.title}"
					title={r ? 'Resume' : 'Start'}
					><svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
						><path
							d="M4.5 2.5v11a.6.6 0 0 0 .9.5l9-5.5a.6.6 0 0 0 0-1l-9-5.5a.6.6 0 0 0-.9.5z"
							fill="currentColor"
						/></svg
					></button
				>
			{/if}
		</div>
	{/if}
	{#if setup}
		<form class="setup" onsubmit={startSetup}>
			<label class="field" for="setup-{info.book.id}"
				>How many pages does your copy have?
				<input
					id="setup-{info.book.id}"
					type="number"
					inputmode="numeric"
					min="1"
					bind:value={setupTotal}
					placeholder="Total pages"
					use:focusFirst
				/></label
			>
			{#if info.catalogPages}
				<button
					class="link hint"
					type="button"
					onclick={() => (setupTotal = String(info.catalogPages))}
					>Catalog lists {info.catalogPages} pages</button
				>
			{/if}
			{#if setupError}<p class="error" role="alert">{setupError}</p>{/if}
			<div class="row-end">
				<button class="btn" type="button" onclick={oncancel}>Cancel</button>
				<button class="btn primary" type="submit">Start reading</button>
			</div>
		</form>
	{/if}
</li>

<style>
	.row {
		display: grid;
		grid-template-columns: 18px 32px minmax(0, 1fr) auto;
		gap: 10px;
		align-items: center;
		min-height: 60px;
		padding: 7px 12px 7px 6px;
		border-bottom: 1px solid var(--line);
		position: relative;
		background: var(--surface);
	}
	.row.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 8px;
		bottom: 8px;
		width: 2px;
		background: var(--accent);
	}
	.row.idle .title,
	.row.idle .cover {
		opacity: 0.7;
	}
	.row.dragging {
		z-index: 5;
		box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
	}
	.handle {
		width: 18px;
		height: 40px;
		display: grid;
		place-items: center;
		border: 0;
		background: none;
		color: var(--muted);
		opacity: 0.6;
		cursor: grab;
		touch-action: none;
		padding: 0;
		border-radius: 6px;
	}
	.handle:hover {
		opacity: 1;
	}
	.cover {
		width: 32px;
		height: 46px;
	}
	.title {
		font-weight: 600;
		font-size: 15.5px;
		line-height: 1.2;
		color: var(--ink);
		text-decoration: none;
	}
	.author {
		font-size: 13px;
		color: var(--muted);
		line-height: 1.25;
	}
	.bar {
		height: 3px;
		background: var(--line);
		margin: 5px 0 3px;
		border-radius: 2px;
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 0.3s;
	}
	.row.idle .bar span {
		background: var(--muted);
	}
	.bar.empty {
		background: repeating-linear-gradient(90deg, var(--line) 0 6px, transparent 6px 10px);
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 2px 10px;
		align-items: center;
		font-size: 13px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.pos {
		background: none;
		border: 0;
		padding: 2px 6px;
		margin-left: -6px;
		border-radius: 6px;
		color: var(--ink);
		font-weight: 500;
		cursor: text;
		text-decoration: underline dotted var(--muted);
		text-underline-offset: 3px;
	}
	.pos:hover {
		background: var(--accent-soft);
	}
	.edit {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
		color: var(--ink);
	}
	.edit input {
		width: 72px;
		padding: 2px 6px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		background: var(--surface);
	}
	.today {
		color: var(--accent);
		font-weight: 500;
	}
	.actions {
		display: flex;
		gap: 6px;
		align-items: center;
	}
	.plus,
	.play {
		width: 44px;
		height: 44px;
		border-radius: 50%;
		cursor: pointer;
		display: grid;
		place-items: center;
	}
	.plus {
		border: 0;
		background: var(--accent);
		color: var(--surface);
		font-size: 25px;
		line-height: 1;
	}
	.play {
		border: 1px solid var(--line);
		background: transparent;
		color: var(--accent);
	}
	.plus:active,
	.play:active {
		transform: scale(0.94);
	}
	.pause {
		width: 30px;
		height: 30px;
		border-radius: 50%;
		border: 1px solid var(--line);
		background: transparent;
		color: var(--muted);
		cursor: pointer;
		display: grid;
		place-items: center;
	}
	.setup {
		grid-column: 2 / -1;
		display: grid;
		gap: 10px;
		padding: 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
		background: var(--bg);
	}
	.hint {
		justify-self: start;
		font-size: 14px;
	}
</style>
