<script lang="ts">
	// Home for signed-in readers: the books in progress, then reading trends.
	// Everything comes from the synced library, so it works offline too.
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import { liveBooks, progressText, readingList } from '#lib/derive.ts';
	import { today as todayFn } from '#lib/actions.ts';
	import { fmtMonthDay } from '#lib/dates.ts';
	import { daysLeft, lastDays, pagesPerDay, yearTotals } from '#lib/stats.ts';
	import BarChart from '#lib/components/BarChart.svelte';
	import Cover from '#lib/components/Cover.svelte';
	import ReadingButtons from '#lib/components/ReadingButtons.svelte';

	const day = $derived.by(() => {
		void library.view;
		return todayFn();
	});
	const year = $derived(Number(day.slice(0, 4)));
	const perDay = $derived(pagesPerDay(library.view));
	const recent = $derived(lastDays(perDay, day, 14));
	const totals = $derived(yearTotals(library.view, perDay, year));
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
		<h1>Welcome, {auth.me?.display_name}!</h1>
	{:else}
		<h1>{greeting}, {auth.me?.display_name}!</h1>
	{/if}
</header>

{#if !library.ready}
	<p class="empty">Loading your library…</p>
{:else if !anyBooks}
	<div class="empty">
		<p>Your books and reading trends will appear here once you add a book.</p>
		<div class="row-end">
			<a class="btn primary" href="/add">Add your first book</a>
			<a class="btn" href="/settings/import">Import from Goodreads, StoryGraph…</a>
		</div>
	</div>
{:else}
	<!-- Books in progress -->
	<section aria-labelledby="cont-h">
		<div class="sec-head">
			<h2 id="cont-h">Pick up where you left off</h2>
			<a href="/reading">All {reading.length} {reading.length === 1 ? 'book' : 'books'}</a>
		</div>
		{#if active.length === 0}
			<p class="muted">Nothing in progress right now. <a href="/reading">Choose a book</a>.</p>
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
									>{progressText(r)}{#if left !== null}{' · '}about {left}
										{left === 1 ? 'day' : 'days'} left at your pace{/if}</span
								>
							</div>
							<!-- Only active books are listed here, so there's nothing to set up. -->
							<ReadingButtons info={b} onsetup={() => {}} />
						</li>
					{/if}
				{/each}
			</ul>
		{/if}
	</section>

	<!-- Trends -->
	<section aria-labelledby="trends-h">
		<div class="sec-head"><h2 id="trends-h">Your reading trends</h2></div>

		<div class="sub">
			<h3>Last 14 days</h3>
			<BarChart
				caption="Pages read each of the last 14 days"
				height={80}
				bars={recent.map((d, i) => ({
					label:
						i === recent.length - 1
							? 'today'
							: i % 2 === 1
								? ''
								: fmtMonthDay(d.date).split(' ')[1],
					value: d.pages,
					tip: `${fmtMonthDay(d.date)}: ${d.pages ? pages(d.pages) : 'no reading'}`,
					current: i === recent.length - 1
				}))}
			/>
		</div>

		<div class="sub">
			<h3>{year} (so far)</h3>
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
			</dl>
		</div>
	</section>
{/if}

<style>
	.head {
		padding: 28px 20px 22px;
		display: grid;
		gap: 10px;
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
		font-size: 28px;
		font-weight: 600;
		line-height: 1.15;
		text-wrap: balance;
	}
	section {
		padding: 22px 20px 26px;
		display: grid;
		gap: 16px;
		border-top: 1px solid var(--line);
	}
	.sec-head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 12px;
	}
	/* The two section headings share one style. */
	h2 {
		margin: 0;
		font-size: 19px;
		font-weight: 600;
		letter-spacing: -0.01em;
	}
	h3 {
		margin: 0;
		font-size: 14.5px;
		font-weight: 600;
		color: var(--muted);
	}
	.sub {
		display: grid;
		gap: 12px;
	}
	.sub + .sub {
		margin-top: 8px;
	}
	.sec-head a {
		font-size: 14px;
		font-weight: 500;
		color: var(--accent);
		text-decoration: none;
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
		padding: 8px 0;
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
	/* Three across at any width. Numbers on top, all on one line (smaller on
	   phones); labels underneath may wrap. */
	.tiles {
		margin: 0;
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 8px;
	}
	.tiles div {
		display: grid;
		align-content: start;
		gap: 2px;
		padding: 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
	}
	.tiles dd {
		order: -1;
		margin: 0;
		font-size: clamp(20px, 6vw, 28px);
		white-space: nowrap;
		font-weight: 600;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
	}
	.tiles dt {
		font-size: 13px;
		color: var(--muted);
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
		padding: 4px 20px 24px;
		color: var(--muted);
	}
	.empty p {
		margin: 0;
	}
</style>
