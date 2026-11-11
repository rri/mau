<script lang="ts">
	// Home for signed-in readers: today and the streak, the books in progress,
	// and the year so far. Everything comes from the synced library, so it works
	// offline too.
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import { liveBooks, readingList } from '#lib/derive.ts';
	import { plusOne, today as todayFn } from '#lib/actions.ts';
	import { fmtMonthDay } from '#lib/dates.ts';
	import { byMonth, daysLeft, lastDays, pagesPerDay, streak, yearTotals } from '#lib/stats.ts';
	import BarChart from '#lib/components/BarChart.svelte';
	import Cover from '#lib/components/Cover.svelte';
	import Icon from '#lib/components/Icon.svelte';

	const MONTHS = [
		'Jan',
		'Feb',
		'Mar',
		'Apr',
		'May',
		'Jun',
		'Jul',
		'Aug',
		'Sep',
		'Oct',
		'Nov',
		'Dec'
	];

	const day = $derived.by(() => {
		void library.view;
		return todayFn();
	});
	const year = $derived(Number(day.slice(0, 4)));
	const perDay = $derived(pagesPerDay(library.view));
	const todayPages = $derived(perDay.get(day) ?? 0);
	const run = $derived(streak(perDay, day));
	const recent = $derived(lastDays(perDay, day, 14));
	const totals = $derived(yearTotals(library.view, perDay, year));
	const months = $derived(byMonth(library.view, perDay, year));
	const reading = $derived(readingList(library.view));
	const active = $derived(reading.filter((b) => b.state === 'active'));
	const anyBooks = $derived(liveBooks(library.view).length > 0);

	const greeting = $derived.by(() => {
		void day;
		const h = new Date().getHours();
		return h < 5
			? 'Good night'
			: h < 12
				? 'Good morning'
				: h < 18
					? 'Good afternoon'
					: 'Good evening';
	});
	const dateLine = $derived(
		new Intl.DateTimeFormat('en-GB', {
			weekday: 'long',
			day: 'numeric',
			month: 'long',
			timeZone: 'UTC'
		}).format(new Date(day + 'T00:00:00Z'))
	);
	const pages = (n: number) => `${n.toLocaleString()} ${n === 1 ? 'page' : 'pages'}`;
	const nf = (n: number) => n.toLocaleString();
</script>

<header class="head">
	<p class="eyebrow">{dateLine}</p>
	{#if library.ready && !anyBooks}
		<h1>Welcome, {auth.me?.display_name}.</h1>
	{:else}
		<h1>{greeting}, {auth.me?.display_name}</h1>
	{/if}
</header>

{#if !library.ready}
	<p class="empty">Loading your library…</p>
{:else if !anyBooks}
	<div class="empty">
		<p>Your reading summaries will appear here once you add a book and log a few pages.</p>
		<div class="row-end">
			<a class="btn primary" href="/add">Add your first book</a>
			<a class="btn" href="/settings/import">Import from Goodreads, StoryGraph…</a>
		</div>
	</div>
{:else}
	<!-- Today and the streak -->
	<section class="today" aria-labelledby="today-h">
		<div class="today-top">
			<div>
				<h2 id="today-h" class="sr-only">Today</h2>
				<p class="big">
					{#if todayPages}{pages(todayPages)} <span>today</span>{:else}Nothing yet <span>today</span
						>{/if}
				</p>
			</div>
			<p class="streak" class:on={run.days > 0}>
				{#if run.days === 0}
					No streak yet
				{:else if run.readToday}
					{run.days}-day streak
				{:else}
					{run.days}-day streak · read today to keep it
				{/if}
			</p>
		</div>
		<BarChart
			caption="Pages read each of the last 14 days"
			height={72}
			bars={recent.map((d, i) => ({
				label:
					i === recent.length - 1 ? 'today' : i % 2 === 1 ? '' : fmtMonthDay(d.date).split(' ')[1],
				value: d.pages,
				tip: `${fmtMonthDay(d.date)}: ${d.pages ? pages(d.pages) : 'no reading'}`,
				current: i === recent.length - 1
			}))}
		/>
	</section>

	<!-- Books in progress -->
	<section aria-labelledby="cont-h">
		<div class="sec-head">
			<h2 id="cont-h">Continue reading</h2>
			<a href="/reading">All {reading.length} {reading.length === 1 ? 'book' : 'books'}</a>
		</div>
		{#if active.length === 0}
			<p class="muted">Nothing active right now. <a href="/reading">Pick a book to start</a>.</p>
		{:else}
			<ul class="books">
				{#each active.slice(0, 5) as b (b.book.id)}
					{@const r = b.reading}
					{#if r}
						{@const left = daysLeft(library.view, r, day)}
						<li>
							<a class="cov" href="/catalog/{b.book.id}" tabindex="-1" aria-hidden="true"
								><Cover title={b.title} author={b.authors[0] ?? ''} cover={b.cover} /></a
							>
							<div class="info">
								<a class="t" href="/catalog/{b.book.id}">{b.title}</a>
								<span class="bar"
									><span style="width:{Math.round((100 * r.pos) / r.total)}%"></span></span
								>
								<span class="meta"
									>page {nf(r.pos)} of {nf(r.total)}{#if left !== null}{' · '}about {left}
										{left === 1 ? 'day' : 'days'} left at your pace{/if}</span
								>
							</div>
							<button
								class="plus"
								type="button"
								onclick={() => plusOne(b)}
								aria-label="Add 1 page to {b.title}"><Icon name="plus" size={18} /></button
							>
						</li>
					{/if}
				{/each}
			</ul>
		{/if}
	</section>

	<!-- The year in numbers -->
	<section aria-labelledby="year-h">
		<div class="sec-head"><h2 id="year-h">{year} so far</h2></div>
		<dl class="tiles">
			<div>
				<dt>books finished</dt>
				<dd>{nf(totals.finished)}</dd>
			</div>
			<div>
				<dt>pages read</dt>
				<dd>{nf(totals.pages)}</dd>
			</div>
			<div>
				<dt>days with reading</dt>
				<dd>{nf(totals.days)}</dd>
			</div>
			<div>
				<dt>pages per reading day</dt>
				<dd>{nf(totals.perReadingDay)}</dd>
			</div>
		</dl>
	</section>

	<!-- Pages by month -->
	<section aria-labelledby="month-h">
		<div class="sec-head">
			<h2 id="month-h">Pages by month</h2>
			<span class="legend"><span class="tick">✓</span> books finished</span>
		</div>
		<BarChart
			caption="Pages read and books finished each month of {year}"
			height={120}
			bars={months.map((m) => {
				const current = m.month === Number(day.slice(5, 7)) - 1;
				return {
					label: MONTHS[m.month].slice(0, 1),
					note: m.finished ? `✓${m.finished}` : undefined,
					value: m.pages,
					tip: `${MONTHS[m.month]} ${year}: ${pages(m.pages)}${m.finished ? `, ${m.finished} finished` : ''}`,
					current,
					future: m.month > Number(day.slice(5, 7)) - 1
				};
			})}
		/>
	</section>
{/if}

<style>
	.head {
		padding: 22px 20px 6px;
		display: grid;
		gap: 2px;
	}
	.eyebrow {
		margin: 0;
		font-size: 12.5px;
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--muted);
	}
	h1 {
		margin: 0;
		font-size: 24px;
		font-weight: 700;
		text-wrap: balance;
	}
	section {
		padding: 18px 20px;
		display: grid;
		gap: 12px;
	}
	section + section {
		border-top: 1px solid var(--line);
	}
	.sec-head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 12px;
	}
	h2 {
		margin: 0;
		font-size: 15px;
		font-weight: 700;
		letter-spacing: -0.01em;
	}
	.sec-head a {
		font-size: 14px;
		font-weight: 500;
		color: var(--accent);
		text-decoration: none;
	}
	.today {
		margin: 8px 12px 6px;
		padding: 18px;
		border-radius: 14px;
		background: var(--active-bg);
	}
	.today + section {
		border-top: 0;
	}
	.today-top {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: baseline;
		gap: 6px 16px;
	}
	.big {
		margin: 0;
		font-size: 30px;
		font-weight: 700;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
	}
	.big span {
		font-size: 18px;
		font-weight: 500;
		color: var(--muted);
	}
	.streak {
		margin: 0;
		font-size: 14px;
		font-weight: 600;
		color: var(--muted);
	}
	.streak.on {
		color: var(--accent);
	}
	.books {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 4px;
	}
	.books li {
		display: grid;
		grid-template-columns: 36px minmax(0, 1fr) auto;
		gap: 12px;
		align-items: center;
		padding: 10px 0;
	}
	.cov {
		width: 36px;
		height: 52px;
	}
	.info {
		display: grid;
		gap: 4px;
		min-width: 0;
	}
	.t {
		font-weight: 600;
		font-size: 15px;
		color: var(--ink);
		text-decoration: none;
	}
	.bar {
		height: 4px;
		border-radius: 99px;
		background: var(--line);
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		border-radius: 99px;
		background: var(--accent);
	}
	.meta {
		font-size: 13px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.plus {
		width: 40px;
		height: 40px;
		border: 0;
		border-radius: 50%;
		background: var(--accent);
		color: var(--on-accent);
		display: grid;
		place-items: center;
		padding: 0;
		cursor: pointer;
		box-shadow: 0 2px 6px color-mix(in srgb, var(--accent) 35%, transparent);
	}
	.plus:active {
		transform: scale(0.94);
	}
	.tiles {
		margin: 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
		gap: 10px;
	}
	/* Numbers on top, all on one line; labels underneath may wrap. */
	.tiles div {
		display: grid;
		align-content: start;
		gap: 2px;
		padding: 14px 16px;
		border: 1px solid var(--line);
		border-radius: 12px;
	}
	.tiles dd {
		order: -1;
	}
	.tiles dt {
		font-size: 13px;
		color: var(--muted);
	}
	.tiles dd {
		margin: 0;
		font-size: 28px;
		font-weight: 700;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
	}
	.legend {
		font-size: 12.5px;
		color: var(--muted);
	}
	.tick {
		color: var(--accent);
		font-weight: 700;
	}
	.muted {
		margin: 0;
		color: var(--muted);
		font-size: 14px;
	}
	.muted a {
		color: var(--accent);
	}
	.empty {
		display: grid;
		gap: 14px;
		padding: 24px 20px;
		color: var(--muted);
	}
	.empty p {
		margin: 0;
	}
</style>
