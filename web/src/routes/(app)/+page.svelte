<script lang="ts">
	// The Reading view: unfinished books, active first, in the user's order.
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import { matchesSearch, readingList, type BookInfo } from '#lib/derive.ts';
	import { moveBook, startReading, today } from '#lib/actions.ts';
	import { getPref, setPref } from '#lib/prefs.ts';
	import ReadingRow from '#lib/components/ReadingRow.svelte';
	import ReadingTile from '#lib/components/ReadingTile.svelte';
	import LayoutToggle from '#lib/components/LayoutToggle.svelte';
	import StartDialog from '#lib/components/StartDialog.svelte';

	// The layout is remembered on this device (apart from the Catalog's); the
	// search starts empty on each visit.
	let layout = $state(getPref('reading.layout', ['list', 'grid'] as const, 'list'));
	$effect(() => setPref('reading.layout', layout));
	let q = $state('');
	const needle = $derived(q.trim().toLocaleLowerCase());

	const all = $derived(readingList(library.view));
	// Reordering needs the whole list, so dragging is off while searching.
	const list = $derived(needle ? all.filter((b) => matchesSearch(b, needle)) : all);
	const day = $derived.by(() => {
		void library.view; // recompute when data changes (cheap)
		return today();
	});

	let editing = $state<string | null>(null);
	let setup = $state<{ id: string; rank?: string } | null>(null);
	const setupInfo = $derived(setup ? all.find((b) => b.book.id === setup?.id) : undefined);

	// ---- Drag to reorder -------------------------------------------------
	let listEl = $state<HTMLUListElement>();
	let drag = $state<{ id: string; startY: number; dy: number; index: number | null } | null>(null);
	let others: { top: number; h: number; active: boolean }[] = [];
	let dropTop = $state(0);
	let dropLabel = $state('');
	let dropInert = $state(false);

	function grab(info: BookInfo, e: PointerEvent) {
		if (e.button > 0) return;
		e.preventDefault();
		const rows = [...(listEl?.querySelectorAll<HTMLElement>(':scope > li') ?? [])];
		others = rows
			.map((el, i) => ({ el, info: list[i] }))
			.filter((x) => x.info && x.info.book.id !== info.book.id)
			.map((x) => ({
				top: x.el.offsetTop,
				h: x.el.offsetHeight,
				active: x.info.state === 'active'
			}));
		drag = { id: info.book.id, startY: e.clientY, dy: 0, index: null };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		const move = (ev: PointerEvent) => onMove(info, ev);
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			window.removeEventListener('pointercancel', cancel);
			drop(info);
		};
		const cancel = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			window.removeEventListener('pointercancel', cancel);
			drag = null;
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
		window.addEventListener('pointercancel', cancel);
	}

	function onMove(info: BookInfo, e: PointerEvent) {
		if (!drag || !listEl) return;
		drag.dy = e.clientY - drag.startY;
		const y = e.clientY - listEl.getBoundingClientRect().top;
		let i = others.findIndex((o) => y < o.top + o.h / 2);
		if (i === -1) i = others.length;
		drag.index = i;
		const last = others.at(-1);
		dropTop = i < others.length ? others[i].top : last ? last.top + last.h : 0;
		const nActive = others.filter((o) => o.active).length;
		const active = info.state === 'active';
		dropInert = active && i > nActive;
		dropLabel = dropInert
			? 'Stays active: goes to end of active'
			: !active && i < nActive
				? info.reading
					? 'Activate here'
					: 'Start here'
				: 'Move here';
	}

	function drop(info: BookInfo) {
		const d = drag;
		drag = null;
		if (!d || d.index === null) return;
		moveBook(info, d.index, (rank) => (setup = { id: info.book.id, rank }));
	}
</script>

<header class="head">
	<div class="title">
		<h1>Reading</h1>
		{#if !library.online}
			<span class="status">Offline{library.pending ? ` · ${library.pending} unsynced` : ''}</span>
		{:else if library.pending}
			<span class="status">Syncing…</span>
		{/if}
	</div>
	{#if all.length}
		<div class="head-actions"><LayoutToggle bind:layout /></div>
	{/if}
</header>

{#if library.ready && all.length}
	<div class="filters">
		<input
			class="search"
			type="search"
			bind:value={q}
			placeholder="Search title or author"
			aria-label="Search books you're reading"
		/>
		{#if needle}
			<div class="count">
				{list.length} of {all.length}
				{all.length === 1 ? 'book' : 'books'}
				<button class="link" type="button" onclick={() => (q = '')}>Clear search</button>
			</div>
		{/if}
	</div>
{/if}

{#if !library.ready}
	<p class="loading">Loading your library…</p>
{:else if all.length === 0}
	<div class="empty">
		<strong>Welcome, {auth.me?.display_name}.</strong>
		<p>Books you're reading will appear here, with a + to log each page.</p>
		<a class="btn primary" href="/add">Add your first book</a>
		<a class="btn" href="/settings/import">Import from Goodreads, StoryGraph…</a>
	</div>
{:else if list.length === 0}
	<p class="loading">No books you're reading match “{q.trim()}”.</p>
{:else if layout === 'grid'}
	<ul class="grid">
		{#each list as info (info.book.id)}
			<ReadingTile {info} onsetup={() => (setup = { id: info.book.id })} />
		{/each}
	</ul>
	{#if setupInfo}
		<StartDialog
			info={setupInfo}
			oncancel={() => (setup = null)}
			onstart={(total) => {
				const info = setupInfo;
				setup = null;
				startReading(info, total);
			}}
		/>
	{/if}
{:else}
	<div class="list-wrap">
		<ul class="list" bind:this={listEl}>
			{#each list as info (info.book.id)}
				<ReadingRow
					{info}
					today={day}
					editing={editing === info.book.id}
					setup={setup?.id === info.book.id}
					dragging={drag?.id === info.book.id}
					draggable={!needle}
					dy={drag?.id === info.book.id ? drag.dy : 0}
					onedit={() => {
						setup = null;
						editing = info.book.id;
					}}
					onsetup={() => {
						editing = null;
						setup = { id: info.book.id };
					}}
					oncancel={() => {
						editing = null;
						setup = null;
					}}
					onstart={(total) => {
						const rank = setup?.rank;
						setup = null;
						startReading(info, total, rank);
					}}
					ongrab={(e) => grab(info, e)}
				/>
			{/each}
		</ul>
		{#if drag && drag.index !== null}
			<div class="drop" class:inert={dropInert} style="top:{dropTop - 1}px">
				<span>{dropLabel}</span>
			</div>
		{/if}
	</div>
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		padding: 14px 14px 8px 16px;
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 12px;
	}
	.head-actions {
		display: flex;
		gap: 6px;
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
	.count {
		display: flex;
		justify-content: space-between;
		font-size: 14px;
		color: var(--muted);
	}
	.grid {
		list-style: none;
		margin: 0;
		padding: 14px;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
		gap: 16px 12px;
	}
	h1 {
		margin: 0;
		font-size: 23px;
		font-weight: 600;
	}
	.status {
		font-size: 13px;
		color: var(--muted);
	}
	.loading {
		padding: 24px 16px;
		color: var(--muted);
	}
	.list-wrap {
		position: relative;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.drop {
		position: absolute;
		left: 8px;
		right: 8px;
		height: 3px;
		border-radius: 2px;
		background: var(--accent);
		pointer-events: none;
		z-index: 6;
	}
	.drop.inert {
		background: var(--muted);
	}
	.drop span {
		position: absolute;
		right: 0;
		top: -22px;
		font-size: 12px;
		color: var(--surface);
		background: var(--accent);
		padding: 1px 7px;
		border-radius: 6px;
		white-space: nowrap;
	}
	.drop.inert span {
		background: var(--muted);
	}
	.empty {
		display: grid;
		justify-items: center;
		gap: 12px;
		padding: 40px 24px;
		text-align: center;
	}
	.empty p {
		margin: 0;
		color: var(--muted);
		max-width: 30ch;
	}
</style>
