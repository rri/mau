<script lang="ts">
	// One book in the Reading view's grid: cover with play, or pause and +.
	import type { BookInfo } from '../derive.ts';
	import { pause, plusOne, resume } from '../actions.ts';
	import Cover from './Cover.svelte';

	let { info, onsetup }: { info: BookInfo; onsetup: () => void } = $props();

	const r = $derived(info.reading);
	const pct = $derived(r ? Math.round((100 * r.pos) / r.total) : 0);
</script>

<li class="tile" class:idle={info.state !== 'active'}>
	<div class="art">
		<a href="/catalog/{info.book.id}" aria-label="{info.title}, {info.authors.join(', ')}"
			><Cover title={info.title} author={info.authors[0] ?? ''} cover={info.cover} size="m" /></a
		>
		<div class="actions">
			{#if info.state === 'active'}
				<button
					class="pause"
					type="button"
					onclick={() => pause(info)}
					aria-label="Pause {info.title}"
					title="Pause"
					><svg width="10" height="10" viewBox="0 0 14 14" aria-hidden="true"
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
					><svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"
						><path
							d="M4.5 2.5v11a.6.6 0 0 0 .9.5l9-5.5a.6.6 0 0 0 0-1l-9-5.5a.6.6 0 0 0-.9.5z"
							fill="currentColor"
						/></svg
					></button
				>
			{/if}
		</div>
	</div>
	{#if r}
		<span class="mini"><span style="width:{pct}%"></span></span>
	{/if}
	<a class="t" href="/catalog/{info.book.id}" tabindex="-1">{info.title}</a>
	{#if r}<span class="pages">p. {r.pos} of {r.total}</span>{/if}
</li>

<style>
	.tile {
		display: grid;
		gap: 7px;
		align-content: start;
	}
	.art {
		position: relative;
		aspect-ratio: 2 / 3;
	}
	.art > a {
		display: block;
		height: 100%;
	}
	/* Active covers get an accent outline; inactive ones are desaturated. */
	.tile:not(.idle) .art > a {
		border-radius: 5px;
		outline: 2.5px solid var(--accent);
		outline-offset: 3px;
	}
	.idle .art > a {
		filter: saturate(0.25);
		opacity: 0.7;
	}
	.idle .t {
		color: var(--muted);
		font-weight: 500;
	}
	.actions {
		position: absolute;
		right: 5px;
		bottom: 5px;
		display: flex;
		gap: 5px;
		align-items: center;
	}
	.plus,
	.play,
	.pause {
		border-radius: 50%;
		cursor: pointer;
		display: grid;
		place-items: center;
		box-shadow: 0 1px 4px rgb(0 0 0 / 0.3);
	}
	.plus,
	.play {
		width: 38px;
		height: 38px;
	}
	.plus {
		border: 0;
		background: var(--accent);
		color: var(--on-accent);
		font-size: 22px;
		line-height: 1;
	}
	.play {
		border: 0;
		background: var(--surface);
		color: var(--accent);
	}
	.pause {
		width: 28px;
		height: 28px;
		border: 0;
		background: var(--surface);
		color: var(--muted);
	}
	.plus:active,
	.play:active {
		transform: scale(0.94);
	}
	.mini {
		height: 3px;
		border-radius: 99px;
		background: var(--line);
		overflow: hidden;
	}
	.mini span {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 0.3s;
	}
	.idle .mini span {
		background: var(--muted);
	}
	.t {
		font-weight: 600;
		font-size: 13.5px;
		line-height: 1.3;
		color: var(--ink);
		text-decoration: none;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.pages {
		margin-top: -4px;
		font-size: 12.5px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
</style>
