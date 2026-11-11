<script lang="ts">
	// One row per year with reading (empty years skipped), shaded by pages a day.
	import { addDays, fmtDate } from '../dates.ts';

	let {
		perDay,
		today,
		includeYear = null
	}: { perDay: Map<string, number>; today: string; includeYear?: number | null } = $props();

	const MONTHS = ['J', 'F', 'M', 'A', 'M', 'J', 'J', 'A', 'S', 'O', 'N', 'D'];
	const level = (p: number) => (p <= 0 ? 0 : p < 10 ? 1 : p < 25 ? 2 : p < 50 ? 3 : 4);

	type Cell = { date: string; pages: number; kind: 'day' | 'out' | 'future' };
	type Year = {
		year: number;
		weeks: number;
		cells: Cell[];
		months: { col: number; label: string }[];
		total: number;
	};

	const years = $derived.by((): Year[] => {
		const set = new Set<number>();
		for (const [d, p] of perDay) if (p > 0) set.add(Number(d.slice(0, 4)));
		if (includeYear) set.add(includeYear);
		return [...set]
			.sort((a, b) => b - a)
			.map((year) => {
				const jan1 = `${year}-01-01`;
				const dow = (new Date(jan1 + 'T00:00:00Z').getUTCDay() + 6) % 7; // Monday = 0
				let day = addDays(jan1, -dow);
				const cells: Cell[] = [];
				const months: { col: number; label: string }[] = [];
				let col = 0;
				let total = 0;
				while (day <= `${year}-12-31`) {
					for (let i = 0; i < 7; i++) {
						if (!day.startsWith(`${year}-`)) cells.push({ date: day, pages: 0, kind: 'out' });
						else {
							if (day.endsWith('-01'))
								months.push({ col, label: MONTHS[Number(day.slice(5, 7)) - 1] });
							const pages = perDay.get(day) ?? 0;
							total += pages;
							cells.push({ date: day, pages, kind: day > today ? 'future' : 'day' });
						}
						day = addDays(day, 1);
					}
					col++;
				}
				return { year, weeks: col, cells, months, total };
			});
	});

	let tip = $state<{ text: string; x: number; y: number } | null>(null);
	function show(e: PointerEvent, c: Cell) {
		if (c.kind !== 'day') return (tip = null);
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		tip = {
			text: `${fmtDate(c.date)}: ${c.pages ? `${c.pages} ${c.pages === 1 ? 'page' : 'pages'}` : 'no reading'}`,
			x: r.left + r.width / 2,
			y: r.top
		};
	}
</script>

<!-- Day tooltips are a pointer convenience; the grid itself is labelled for screen readers. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="years" onpointerleave={() => (tip = null)}>
	{#each years as y (y.year)}
		<div class="year" style="--w:{y.weeks}">
			<div class="year-head">
				<b>{y.year}</b>
				<span>{y.total} {y.total === 1 ? 'page' : 'pages'}</span>
			</div>
			<div class="months" aria-hidden="true">
				{#each y.months as m (m.col)}<span style="grid-column:{m.col + 1}">{m.label}</span>{/each}
			</div>
			<div class="grid" role="img" aria-label="Pages read per day in {y.year}">
				{#each y.cells as c (c.date)}
					<span
						class="cell l{level(c.pages)}"
						class:out={c.kind === 'out'}
						class:future={c.kind === 'future'}
						class:today={c.date === today}
						onpointerenter={(e) => show(e, c)}
					></span>
				{/each}
			</div>
		</div>
	{/each}
	<div class="legend" aria-hidden="true">
		Less <span class="cell"></span><span class="cell l1"></span><span class="cell l2"></span><span
			class="cell l3"
		></span><span class="cell l4"></span> More
	</div>
</div>
{#if tip}
	<div class="tip" style="left:{tip.x}px;top:{tip.y}px">{tip.text}</div>
{/if}

<style>
	.years {
		display: grid;
		gap: 12px;
		/* One teal ramp, light to dark (dark mode inverts toward light). */
		--h0: var(--line);
		--h1: color-mix(in srgb, var(--accent) 28%, var(--surface));
		--h2: color-mix(in srgb, var(--accent) 50%, var(--surface));
		--h3: color-mix(in srgb, var(--accent) 74%, var(--surface));
		--h4: var(--accent);
	}
	.year {
		display: grid;
		gap: 3px;
	}
	.year-head {
		display: flex;
		justify-content: space-between;
		font-size: 13px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.year-head b {
		color: var(--ink);
		font-weight: 600;
	}
	.months {
		display: grid;
		grid-template-columns: repeat(var(--w), 1fr);
		font-size: 9px;
		color: var(--muted);
		height: 12px;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(var(--w), 1fr);
		grid-template-rows: repeat(7, auto);
		grid-auto-flow: column;
		gap: 1.5px;
	}
	.cell {
		display: block;
		aspect-ratio: 1;
		border-radius: 1.5px;
		background: var(--h0);
	}
	.l1 {
		background: var(--h1);
	}
	.l2 {
		background: var(--h2);
	}
	.l3 {
		background: var(--h3);
	}
	.l4 {
		background: var(--h4);
	}
	.out,
	.future {
		background: transparent;
	}
	.today {
		outline: 1.5px solid var(--ink);
	}
	.legend {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 4px;
		font-size: 12px;
		color: var(--muted);
	}
	.legend .cell {
		width: 11px;
	}
	.tip {
		position: fixed;
		transform: translate(-50%, calc(-100% - 6px));
		pointer-events: none;
		z-index: 30;
		background: var(--ink);
		color: var(--surface);
		font-size: 13px;
		padding: 4px 8px;
		border-radius: 6px;
		white-space: nowrap;
	}
</style>
