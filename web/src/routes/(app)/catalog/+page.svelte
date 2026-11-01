<script lang="ts">
	// The catalog: every book on the user's list, in a list or grid.
	import { library } from '#lib/library.svelte.ts';
	import { liveBooks, type BookInfo, type BookState } from '#lib/derive.ts';
	import { getPref, setPref } from '#lib/prefs.ts';
	import { authorKey, titleKey } from '#lib/sorting.ts';
	import Cover from '#lib/components/Cover.svelte';
	import StateText from '#lib/components/StateText.svelte';

	const LAYOUTS = ['list', 'grid'] as const;
	const SORTS = ['added', 'title', 'author', 'finished'] as const;
	const STATES: { key: BookState; label: string }[] = [
		{ key: 'active', label: 'Active' },
		{ key: 'inactive', label: 'Inactive' },
		{ key: 'finished', label: 'Finished' }
	];

	// Layout and sort are remembered on this device; filters start fresh.
	let layout = $state(getPref('catalog.layout', LAYOUTS, 'list'));
	let sort = $state(getPref('catalog.sort', SORTS, 'added'));
	let states = $state<BookState[]>(['active', 'inactive', 'finished']);
	let ownedOnly = $state(false);
	let tags = $state<string[]>([]);
	let q = $state('');

	$effect(() => setPref('catalog.layout', layout));
	$effect(() => setPref('catalog.sort', sort));

	const all = $derived(liveBooks(library.view));
	const allTags = $derived(
		[...new Set(all.flatMap((b) => b.tags))].sort((a, b) => a.localeCompare(b))
	);

	function finishedOn(b: BookInfo): string {
		return [...b.readings].reverse().find((r) => r.finished_on)?.finished_on ?? '';
	}

	const shown = $derived.by(() => {
		const needle = q.trim().toLocaleLowerCase();
		const list = all.filter(
			(b) =>
				states.includes(b.state) &&
				(!ownedOnly || b.owned) &&
				tags.every((t) => b.tags.includes(t)) &&
				(!needle ||
					b.title.toLocaleLowerCase().includes(needle) ||
					b.authors.some((a) => a.toLocaleLowerCase().includes(needle)))
		);
		const byAdded = (a: BookInfo, b: BookInfo) => b.book.added_at - a.book.added_at;
		const cmp: Record<(typeof SORTS)[number], (a: BookInfo, b: BookInfo) => number> = {
			added: byAdded,
			title: (a, b) => titleKey(a.title).localeCompare(titleKey(b.title)),
			author: (a, b) =>
				authorKey(a.authors[0]).localeCompare(authorKey(b.authors[0])) ||
				titleKey(a.title).localeCompare(titleKey(b.title)),
			finished: (a, b) => finishedOn(b).localeCompare(finishedOn(a)) || byAdded(a, b)
		};
		return list.sort(cmp[sort]);
	});

	const filtering = $derived(
		states.length !== 3 || ownedOnly || tags.length > 0 || q.trim() !== ''
	);

	function toggle<T>(list: T[], v: T): T[] {
		return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
	}
	function clear() {
		states = ['active', 'inactive', 'finished'];
		ownedOnly = false;
		tags = [];
		q = '';
	}
</script>

<header class="head">
	<h1>Catalog</h1>
	<div class="head-actions">
		<button
			class="icon"
			type="button"
			aria-label="List layout"
			aria-pressed={layout === 'list'}
			onclick={() => (layout = 'list')}
			><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"
				><g fill="currentColor"
					><rect x="1" y="2" width="14" height="2" rx="1" /><rect
						x="1"
						y="7"
						width="14"
						height="2"
						rx="1"
					/><rect x="1" y="12" width="14" height="2" rx="1" /></g
				></svg
			></button
		>
		<button
			class="icon"
			type="button"
			aria-label="Grid layout"
			aria-pressed={layout === 'grid'}
			onclick={() => (layout = 'grid')}
			><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"
				><g fill="currentColor"
					><rect x="1" y="1" width="6" height="6" rx="1.5" /><rect
						x="9"
						y="1"
						width="6"
						height="6"
						rx="1.5"
					/><rect x="1" y="9" width="6" height="6" rx="1.5" /><rect
						x="9"
						y="9"
						width="6"
						height="6"
						rx="1.5"
					/></g
				></svg
			></button
		>
		<a class="icon add" href="/add" aria-label="Add a book" title="Add a book">+</a>
	</div>
</header>

<div class="filters">
	<input
		class="search"
		type="search"
		bind:value={q}
		placeholder="Search title or author"
		aria-label="Search your catalog"
	/>
	<div class="state-row">
		<div class="chips" role="group" aria-label="Reading state">
			{#each STATES as s (s.key)}
				<button
					class="chip"
					type="button"
					aria-pressed={states.includes(s.key)}
					onclick={() => (states = toggle(states, s.key))}>{s.label}</button
				>
			{/each}
		</div>
		<label class="own"><input type="checkbox" bind:checked={ownedOnly} /> Owned?</label>
	</div>
	{#if allTags.length}
		<div class="chips" role="group" aria-label="Tags">
			{#each allTags as t (t)}
				<button
					class="chip tag"
					type="button"
					aria-pressed={tags.includes(t)}
					onclick={() => (tags = toggle(tags, t))}>{t}</button
				>
			{/each}
		</div>
	{/if}
	<div class="bar-row">
		<span
			>{filtering
				? `${shown.length} of ${all.length} books`
				: `${all.length} ${all.length === 1 ? 'book' : 'books'}`}</span
		>
		<span class="sort">
			{#if filtering}<button class="link" type="button" onclick={clear}>Clear filters</button>{/if}
			<label for="sort">Sort</label>
			<select id="sort" bind:value={sort}>
				<option value="added">Recently added</option>
				<option value="title">Title</option>
				<option value="author">Author</option>
				<option value="finished">Recently finished</option>
			</select>
		</span>
	</div>
</div>

{#if !library.ready}
	<p class="empty">Loading…</p>
{:else if all.length === 0}
	<div class="empty">
		<p>Your catalog is empty.</p>
		<a class="btn primary" href="/add">Add a book</a>
	</div>
{:else if shown.length === 0}
	<p class="empty">
		{states.length
			? 'No books match these filters.'
			: 'Select Active, Inactive, or Finished to see books.'}
	</p>
{:else if layout === 'list'}
	<ul class="list">
		{#each shown as b (b.book.id)}
			<li>
				<a class="item" href="/catalog/{b.book.id}">
					<span class="cov"
						><Cover title={b.title} author={b.authors[0] ?? ''} cover={b.cover} /></span
					>
					<span class="info">
						<span class="title">{b.title}</span>
						<span class="author">{b.authors.join(', ')}</span>
						<span class="sub"
							><StateText info={b} />{#each b.tags.slice(0, 2) as t (t)}<span class="tagtxt"
									>#{t}</span
								>{/each}</span
						>
					</span>
					{#if b.owned}
						<span class="owned" title="You own a copy"
							><svg width="15" height="15" viewBox="0 0 16 16" aria-label="Owned"
								><g fill="none" stroke="currentColor" stroke-width="1.5"
									><rect x="2" y="2.5" width="3" height="11" rx=".8" /><rect
										x="6.5"
										y="2.5"
										width="3"
										height="11"
										rx=".8"
									/><path d="M11 3.4l2.4-.6 1.6 10.4-2.4.6z" stroke-linejoin="round" /></g
								></svg
							></span
						>
					{/if}
				</a>
			</li>
		{/each}
	</ul>
{:else}
	<ul class="grid">
		{#each shown as b (b.book.id)}
			<li>
				<a class="tile" href="/catalog/{b.book.id}" aria-label="{b.title}, {b.authors.join(', ')}">
					<span class="art">
						<Cover title={b.title} author={b.authors[0] ?? ''} cover={b.cover} size="m" />
						{#if b.state === 'finished'}<span class="badge tl" title="Finished">✓</span>{/if}
						{#if b.state === 'active'}<span class="badge tl dot" title="Active">●</span>{/if}
						{#if b.owned}<span class="badge br" title="Owned">⌂</span>{/if}
					</span>
					{#if b.reading && b.state !== 'finished' && b.reading.pos > 0}
						<span class="mini" class:idle={b.state === 'inactive'}
							><span style="width:{Math.round((100 * b.reading.pos) / b.reading.total)}%"
							></span></span
						>
					{/if}
					<span class="t">{b.title}</span>
				</a>
			</li>
		{/each}
	</ul>
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 14px 8px 16px;
	}
	h1 {
		margin: 0;
		font-size: 23px;
		font-weight: 600;
	}
	.head-actions {
		display: flex;
		gap: 6px;
	}
	.icon {
		width: 38px;
		height: 38px;
		border-radius: 10px;
		border: 1px solid var(--line);
		background: var(--surface);
		display: grid;
		place-items: center;
		cursor: pointer;
		color: var(--ink);
		text-decoration: none;
	}
	.icon[aria-pressed='true'] {
		background: var(--accent-soft);
		color: var(--accent);
		border-color: transparent;
	}
	.icon.add {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--surface);
		font-size: 22px;
	}
	.filters {
		display: grid;
		gap: 8px;
		padding: 0 14px 12px;
		border-bottom: 1px solid var(--line);
	}
	.search {
		width: 100%;
		padding: 9px 12px;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--surface);
		font-size: 16px;
	}
	.state-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
	}
	.chips {
		display: flex;
		gap: 6px;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.chip {
		flex: none;
		padding: 4px 11px;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: var(--surface);
		color: var(--muted);
		font-size: 14px;
		cursor: pointer;
		white-space: nowrap;
	}
	.chip.tag::before {
		content: '#';
		opacity: 0.6;
	}
	.chip[aria-pressed='true'] {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--surface);
	}
	.own {
		flex: none;
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 14px;
		cursor: pointer;
	}
	.own input {
		accent-color: var(--accent);
		width: 16px;
		height: 16px;
		margin: 0;
	}
	.bar-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		font-size: 14px;
		color: var(--muted);
	}
	.sort {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.sort select {
		border: 1px solid var(--line);
		border-radius: 8px;
		padding: 3px 6px;
		background: var(--surface);
		font-size: 14px;
	}
	.empty {
		display: grid;
		justify-items: center;
		gap: 12px;
		padding: 40px 18px;
		text-align: center;
		color: var(--muted);
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.item {
		display: grid;
		grid-template-columns: 34px minmax(0, 1fr) auto;
		gap: 12px;
		align-items: center;
		padding: 8px 14px;
		border-bottom: 1px solid var(--line);
		color: var(--ink);
		text-decoration: none;
		min-height: 60px;
	}
	.item:hover {
		background: var(--surface);
	}
	.cov {
		width: 34px;
		height: 49px;
	}
	.info {
		display: grid;
		min-width: 0;
	}
	.title {
		font-weight: 600;
		font-size: 15.5px;
		line-height: 1.2;
	}
	.author {
		font-size: 13px;
		color: var(--muted);
	}
	.sub {
		display: flex;
		flex-wrap: wrap;
		gap: 2px 8px;
		margin-top: 2px;
		font-size: 12.5px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.owned {
		color: var(--muted);
	}
	.grid {
		list-style: none;
		margin: 0;
		padding: 14px;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
		gap: 16px 12px;
	}
	.tile {
		display: grid;
		gap: 6px;
		align-content: start;
		color: var(--ink);
		text-decoration: none;
	}
	.art {
		position: relative;
		aspect-ratio: 2 / 3;
	}
	.badge {
		position: absolute;
		width: 22px;
		height: 22px;
		border-radius: 50%;
		background: var(--surface);
		color: var(--ink);
		display: grid;
		place-items: center;
		font-size: 12px;
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.25);
	}
	.badge.tl {
		left: 5px;
		top: 5px;
		color: var(--accent);
	}
	.badge.dot {
		font-size: 10px;
	}
	.badge.br {
		right: 5px;
		bottom: 5px;
	}
	.mini {
		height: 3px;
		border-radius: 2px;
		background: var(--line);
		overflow: hidden;
	}
	.mini span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.mini.idle span {
		background: var(--muted);
	}
	.t {
		font-weight: 600;
		font-size: 13px;
		line-height: 1.25;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
</style>
