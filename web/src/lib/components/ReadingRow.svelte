<script lang="ts">
	// One book in the Reading view.
	import { ago, dateOfMs } from '../dates.ts';
	import { lastReadDate, pagesOn, progressText, type BookInfo } from '../derive.ts';
	import { library } from '../library.svelte.ts';
	import { auth } from '../auth.svelte.ts';
	import type { Start } from '../actions.ts';
	import Cover from './Cover.svelte';
	import ProgressEditor from './ProgressEditor.svelte';
	import ReadingButtons from './ReadingButtons.svelte';
	import StartForm from './StartForm.svelte';

	let {
		info,
		today,
		editing = false,
		setup = false,
		dragging = false,
		draggable = true,
		dy = 0,
		onedit,
		onsetup,
		oncancel,
		onstart,
		ongrab,
		onmove
	}: {
		info: BookInfo;
		today: string;
		editing?: boolean;
		setup?: boolean;
		dragging?: boolean;
		/** False while searching: the handle is hidden and its column kept. */
		draggable?: boolean;
		dy?: number;
		onedit: () => void;
		onsetup: () => void;
		oncancel: () => void;
		onstart: (start: Start) => void;
		ongrab: (e: PointerEvent) => void;
		/** Arrow keys on the handle: one place up (-1) or down (1). */
		onmove: (by: -1 | 1) => void;
	} = $props();

	const r = $derived(info.reading);
	const pct = $derived(r ? Math.round((100 * r.pos) / r.total) : 0);
	const todayPages = $derived(r ? pagesOn(library.view, r, today) : 0);
	const lastRead = $derived(r ? lastReadDate(library.view, r.id) : null);
	const addedOn = $derived(dateOfMs(info.book.added_at, auth.me?.time_zone ?? 'UTC'));

	// Inline forms can open near the bottom of the screen; bring the whole
	// form into view (scroll-padding keeps it clear of the phone tab bar).
	function reveal(node: HTMLElement) {
		requestAnimationFrame(() => node.scrollIntoView({ block: 'nearest' }));
	}
</script>

<li
	class="row"
	class:active={info.state === 'active'}
	class:idle={info.state !== 'active'}
	class:dragging
	style={dragging ? `transform: translateY(${dy}px)` : undefined}
>
	{#if draggable}
		<button
			class="handle"
			type="button"
			aria-label="Drag to reorder {info.title}, or use the up and down arrow keys"
			title="Drag to reorder (or use ↑ ↓)"
			onpointerdown={ongrab}
			onkeydown={(e) => {
				if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
				e.preventDefault();
				onmove(e.key === 'ArrowUp' ? -1 : 1);
			}}
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
	{:else}
		<span></span>
	{/if}
	<a class="cover" href="/catalog/{info.book.id}" tabindex="-1" aria-hidden="true"
		><Cover title={info.title} author={info.authors[0] ?? ''} cover={info.cover} /></a
	>
	<div class="main">
		<a class="title" href="/catalog/{info.book.id}">{info.title}</a>
		<div class="author">{info.authors.join(', ')}</div>
		<div class="bar" class:empty={!r}><span style="width:{pct}%"></span></div>
		<div class="meta">
			{#if !r}
				<span>Not started</span>
				<span>added {ago(addedOn, today)}</span>
			{:else if editing}
				<ProgressEditor {info} ondone={oncancel} />
			{:else}
				<button
					class="pos"
					type="button"
					onclick={onedit}
					aria-label="Edit progress, now {progressText(r)}">{progressText(r)}</button
				>
				{#if todayPages > 0}
					<span class="today"
						>+{r.unit === 'percent'
							? `${todayPages}%`
							: `${todayPages} ${todayPages === 1 ? 'page' : 'pages'}`} today</span
					>
				{:else if r.pos === 0 && !lastRead}
					<span>no progress yet</span>
				{:else}
					<span>last read {ago(lastRead ?? r.started_on, today)}</span>
				{/if}
			{/if}
		</div>
	</div>
	{#if !setup && !editing}
		<ReadingButtons {info} {onsetup} />
	{/if}
	{#if setup}
		<div class="setup" use:reveal>
			<StartForm {info} {onstart} {oncancel} />
		</div>
	{/if}
</li>

<style>
	.row {
		display: grid;
		grid-template-columns: 14px 36px minmax(0, 1fr) auto;
		gap: 10px;
		align-items: center;
		min-height: 64px;
		padding: 11px 12px 11px 6px;
		margin-bottom: 2px;
		border-radius: 12px;
		position: relative;
		background: var(--surface);
	}
	/* Active books sit on a tinted card with an accent bar; inactive ones are
	   quiet: desaturated cover, grey title. */
	.row.active {
		background: var(--active-bg);
	}
	.row.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 12px;
		bottom: 12px;
		width: 3px;
		border-radius: 3px;
		background: var(--accent);
	}
	.row.idle .cover {
		filter: saturate(0.25);
		opacity: 0.7;
	}
	.row.idle .title {
		color: var(--muted);
		font-weight: 500;
	}
	.row.idle .pos {
		color: var(--muted);
	}
	.row.dragging {
		z-index: 5;
		box-shadow: 0 10px 28px rgb(0 0 0 / 0.18);
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
		width: 36px;
		height: 52px;
	}
	.title {
		font-weight: 600;
		font-size: 15.5px;
		letter-spacing: -0.01em;
		line-height: 1.3;
		color: var(--ink);
		text-decoration: none;
	}
	.author {
		font-size: 13.5px;
		color: var(--muted);
		line-height: 1.3;
	}
	.bar {
		height: 4px;
		background: var(--line);
		margin: 7px 0 4px;
		border-radius: 99px;
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		border-radius: 99px;
		background: var(--accent);
		transition: width 0.3s;
	}
	.row.idle .bar span {
		background: var(--muted);
		opacity: 0.6;
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
		text-decoration: underline dashed color-mix(in srgb, currentColor 35%, transparent);
		text-decoration-thickness: 1px;
		text-underline-offset: 4px;
	}
	.pos:hover {
		background: var(--accent-soft);
	}
	.today {
		color: var(--accent);
		font-weight: 600;
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
</style>
