<script lang="ts">
	// One book in the Reading view's grid: a clean cover, then the progress,
	// the progress, the reading buttons, then the title.
	import { progressShort, type BookInfo } from '../derive.ts';
	import Cover from './Cover.svelte';
	import ReadingButtons from './ReadingButtons.svelte';

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
		<ReadingButtons {info} {onsetup} size="sm" />
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
	/* The progress, then the buttons on their own line (right-aligned). */
	.controls {
		display: grid;
		gap: 4px;
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
