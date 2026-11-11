<script lang="ts">
	// A single-series bar chart: bars grow from a shared baseline, every bar
	// has a tooltip on hover, and a hidden table carries the numbers for
	// keyboard and screen-reader users.
	type Bar = {
		label: string;
		value: number;
		/** Tooltip and table text, e.g. "Mar 4: 12 pages". */
		tip: string;
		/** Below the label, e.g. books finished that month. */
		note?: string;
		/** Today, or the current month. */
		current?: boolean;
		/** Not reached yet (later months): drawn as empty. */
		future?: boolean;
	};

	let {
		bars,
		caption,
		height = 96,
		unit = 'pages'
	}: { bars: Bar[]; caption: string; height?: number; unit?: string } = $props();

	const max = $derived(Math.max(1, ...bars.map((b) => b.value)));
	let tip = $state<{ i: number; text: string } | null>(null);
</script>

<figure class="chart" style="--n:{bars.length}">
	<div class="plot" style="height:{height}px" aria-hidden="true">
		{#each bars as b, i (i)}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="col"
				onpointerenter={() => (tip = { i, text: b.tip })}
				onpointerleave={() => (tip = null)}
			>
				{#if tip?.i === i}<span class="tip">{tip.text}</span>{/if}
				<span
					class="bar"
					class:current={b.current}
					class:zero={b.value === 0}
					class:future={b.future}
					style="height:{b.value ? Math.max(3, Math.round((b.value / max) * 100)) : 0}%"
				></span>
			</div>
		{/each}
	</div>
	<div class="labels" aria-hidden="true">
		{#each bars as b, i (i)}
			<span class:current={b.current}
				>{b.label}{#if b.note}<small>{b.note}</small>{/if}</span
			>
		{/each}
	</div>
	<table class="sr-only">
		<caption>{caption}</caption>
		<tbody>
			{#each bars as b, i (i)}
				{#if !b.future}<tr><td>{b.tip}</td></tr>{/if}
			{/each}
		</tbody>
	</table>
	<figcaption class="sr-only">{caption}, in {unit}</figcaption>
</figure>

<style>
	.chart {
		margin: 0;
		display: grid;
		gap: 6px;
	}
	.plot,
	.labels {
		display: grid;
		grid-template-columns: repeat(var(--n), minmax(0, 1fr));
		gap: 4px;
	}
	.plot {
		align-items: end;
		border-bottom: 1px solid var(--line);
	}
	.col {
		position: relative;
		height: 100%;
		display: flex;
		align-items: flex-end;
		cursor: default;
	}
	.bar {
		width: 100%;
		border-radius: 4px 4px 0 0;
		background: color-mix(in srgb, var(--accent) 55%, var(--surface));
		transition: height 0.3s;
	}
	.bar.current {
		background: var(--accent);
	}
	.bar.zero:not(.future) {
		height: 3px !important;
		border-radius: 2px;
		background: var(--line);
	}
	.col:hover .bar:not(.zero) {
		background: var(--accent);
	}
	.tip {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 50%;
		transform: translateX(-50%);
		z-index: 5;
		padding: 4px 8px;
		border-radius: 6px;
		background: var(--ink);
		color: var(--surface);
		font-size: 12px;
		white-space: nowrap;
		pointer-events: none;
	}
	.labels span {
		display: grid;
		justify-items: center;
		font-size: 11.5px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.labels span.current {
		color: var(--ink);
		font-weight: 600;
	}
	.labels small {
		font-size: 11px;
		color: var(--accent);
		font-weight: 600;
	}
</style>
