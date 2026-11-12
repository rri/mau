<script lang="ts">
	// One book in the Reading view's grid: a clean cover, then the progress,
	// a row with the progress, ⋯, and pause / up arrow (or play), then the title.
	import { progressShort, type BookInfo } from '../derive.ts';
	import { pause, plusOne, resume } from '../actions.ts';
	import BookMenu from './BookMenu.svelte';
	import Cover from './Cover.svelte';
	import Icon from './Icon.svelte';

	let { info, onsetup }: { info: BookInfo; onsetup: () => void } = $props();

	const r = $derived(info.reading);
	const pct = $derived(r ? Math.round((100 * r.pos) / r.total) : 0);
</script>

<li class="tile" class:idle={info.state !== 'active'}>
	<a class="art" href="/catalog/{info.book.id}" tabindex="-1" aria-hidden="true"
		><Cover title={info.title} author={info.authors[0] ?? ''} cover={info.cover} size="m" /></a
	>
	<span class="mini" class:empty={!r}><span style="width:{pct}%"></span></span>
	<div class="controls">
		<span class="pages">{r ? progressShort(r) : 'Not started'}</span>
		<BookMenu {info} {onsetup} size="sm" />
		{#if info.state === 'active'}
			<button
				class="pause"
				type="button"
				onclick={() => pause(info)}
				aria-label="Pause {info.title}"
				title="Pause"><Icon name="pause" size={11} /></button
			>
			<button
				class="plus"
				type="button"
				onclick={() => plusOne(info)}
				aria-label="Add {r?.unit === 'percent' ? '1%' : '1 page'} to {info.title}"
				title={r?.unit === 'percent' ? '+1%' : '+1 page'}><Icon name="step" size={16} /></button
			>
		{:else}
			<button
				class="play"
				type="button"
				onclick={() => (r ? resume(info) : onsetup())}
				aria-label="{r ? 'Resume' : 'Start'} {info.title}"
				title={r ? 'Resume' : 'Start'}><Icon name="play" size={14} /></button
			>
		{/if}
	</div>
	<a class="t" href="/catalog/{info.book.id}">{info.title}</a>
</li>

<style>
	.tile {
		display: grid;
		gap: 8px;
		align-content: start;
		min-width: 0;
	}
	.art {
		display: block;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
	}
	/* Active covers get an accent ring; inactive ones are desaturated. */
	.tile:not(.idle) .art {
		outline: 2px solid var(--accent);
		outline-offset: 3px;
	}
	.idle .art {
		filter: saturate(0.25);
		opacity: 0.7;
	}
	.mini {
		height: 3px;
		margin-top: 2px;
		border-radius: 99px;
		background: var(--line);
		overflow: hidden;
	}
	.mini.empty {
		background: repeating-linear-gradient(90deg, var(--line) 0 6px, transparent 6px 10px);
	}
	.mini span {
		display: block;
		height: 100%;
		border-radius: 99px;
		background: var(--accent);
		transition: width 0.3s;
	}
	.idle .mini span {
		background: var(--muted);
		opacity: 0.6;
	}
	.controls {
		display: flex;
		align-items: center;
		gap: 4px;
		min-height: 32px;
	}
	.pages {
		flex: 1;
		min-width: 0;
		font-size: 12px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.plus,
	.play,
	.pause {
		flex: none;
		display: grid;
		place-items: center;
		padding: 0;
		border-radius: 50%;
		cursor: pointer;
	}
	.plus {
		width: 32px;
		height: 32px;
		border: 0;
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: 0 2px 6px color-mix(in srgb, var(--accent) 35%, transparent);
	}
	.play {
		width: 32px;
		height: 32px;
		border: 1px solid var(--line);
		background: var(--surface);
		color: var(--muted);
	}
	.play:hover {
		color: var(--accent);
		border-color: var(--accent);
	}
	.pause {
		width: 24px;
		height: 24px;
		border: 1px solid var(--line);
		background: var(--surface);
		color: var(--muted);
	}
	.plus:active,
	.play:active {
		transform: scale(0.94);
	}
	.t {
		margin-top: -2px;
		font-weight: 600;
		font-size: 13.5px;
		line-height: 1.3;
		letter-spacing: -0.01em;
		color: var(--ink);
		text-decoration: none;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.idle .t {
		color: var(--muted);
		font-weight: 500;
	}
</style>
