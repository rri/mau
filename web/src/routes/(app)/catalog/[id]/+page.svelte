<script lang="ts">
	// One book: status, rating and notes, tags, copies, and reading activity.
	import { afterNavigate, goto } from '$app/navigation';
	import { page } from '$app/state';
	import { ApiError, post } from '#lib/api/client.ts';
	import type { EditionRow } from '#lib/api/generated/EditionRow.ts';
	import { pause, plusOne, resume, setPages, startReading, today } from '#lib/actions.ts';
	import { fmtDate, fmtMonthDay } from '#lib/dates.ts';
	import { bookInfo, eventsOf, liveBooks, pagesOn } from '#lib/derive.ts';
	import { library } from '#lib/library.svelte.ts';
	import { uuidv7 } from '#lib/sync/ids.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import { uploadCover } from '#lib/covers.ts';
	import ActivityCalendar from '#lib/components/ActivityCalendar.svelte';
	import Cover from '#lib/components/Cover.svelte';
	import TagInput from '#lib/components/TagInput.svelte';

	const FORMATS = ['Hardcover', 'Paperback', 'Ebook', 'Audiobook'];

	// "‹ Catalog" goes back in history when we came from the catalog, so its
	// scroll position and filters are kept.
	let cameFromCatalog = $state(false);
	afterNavigate(({ from }) => (cameFromCatalog = from?.url.pathname === '/catalog'));
	function back(e: MouseEvent) {
		if (cameFromCatalog) {
			e.preventDefault();
			history.back();
		}
	}

	const id = $derived(page.params.id ?? '');
	const book = $derived(library.view.books.get(id));
	const info = $derived(book && !book.deleted ? bookInfo(library.view, book) : null);
	const r = $derived(info?.reading ?? null);
	const day = $derived.by(() => {
		void library.view;
		return today();
	});

	// ---- Header & details ------------------------------------------------
	const others = $derived(info?.work?.people.filter((p) => p.role !== 'author') ?? []);
	const roleLabel: Record<string, string> = {
		translator: 'Translated by',
		editor: 'Edited by',
		illustrator: 'Illustrated by'
	};

	let menu = $state(false);
	let editing = $state(false);
	let editTitle = $state('');
	let editAuthors = $state('');
	let coverFile = $state<FileList | null>(null);
	let editError = $state('');
	let busy = $state(false);

	function openEdit() {
		if (!info) return;
		menu = false;
		editTitle = info.title;
		editAuthors = info.authors.join(', ');
		editError = '';
		editing = true;
	}

	async function saveEdit(e: SubmitEvent) {
		e.preventDefault();
		if (!info) return;
		busy = true;
		editError = '';
		try {
			const fields: Record<string, unknown> = {};
			const workTitle = info.work?.title ?? '';
			const workAuthors =
				info.work?.people.filter((p) => p.role === 'author').map((p) => p.name) ?? [];
			const t = editTitle.trim();
			const a = editAuthors
				.split(',')
				.map((x) => x.trim())
				.filter(Boolean);
			if (t && t !== info.title) fields.title_override = t === workTitle ? '' : t;
			if (a.join('|') !== info.authors.join('|'))
				fields.authors_override = a.join('|') === workAuthors.join('|') ? [] : a;
			const file = coverFile?.[0];
			if (file) fields.cover_override = await uploadCover(file);
			if (Object.keys(fields).length)
				library.commit([{ type: 'book.set', id: info.book.id, fields }]);
			editing = false;
			coverFile = null;
		} catch (err) {
			editError =
				err instanceof ApiError && err.offline
					? 'Uploading a cover needs a connection.'
					: (err as Error).message;
		} finally {
			busy = false;
		}
	}

	const overridden = $derived(
		!!(info?.book.title_override || info?.book.authors_override || info?.book.cover_override)
	);
	function resetOverride() {
		if (!info) return;
		library.commit([
			{
				type: 'book.set',
				id: info.book.id,
				fields: { title_override: '', authors_override: [], cover_override: '' }
			}
		]);
	}

	async function suggest() {
		if (!info?.work) return;
		menu = false;
		try {
			await post(`/catalog/works/${info.work.id}/suggest`);
			toasts.show('Suggested for the shared catalog. An admin will review it.');
		} catch (e) {
			toasts.show((e as Error).message);
		}
	}

	// ---- Status ------------------------------------------------------------
	let pagesEditing = $state(false);
	let editPos = $state(0);
	let editTotal = $state(0);
	let setupOpen = $state(false);
	let setupTotal = $state('');
	let setupError = $state('');

	function openPages() {
		if (!r) return;
		editPos = r.pos;
		editTotal = r.total;
		pagesEditing = true;
	}
	function savePages() {
		if (!info || !r) return;
		pagesEditing = false;
		const total = editTotal >= 1 ? Math.floor(editTotal) : r.total;
		setPages(info, Math.floor(editPos), total);
	}
	function play() {
		if (!info) return;
		if (info.state === 'finished') startReading(info, r?.total ?? info.catalogPages ?? 1);
		else if (r) resume(info);
		else setupOpen = true;
	}
	function setup(e: SubmitEvent) {
		e.preventDefault();
		if (!info) return;
		const n = Number(setupTotal);
		if (!Number.isInteger(n) || n < 1) {
			setupError = 'Enter the number of pages in your copy.';
			return;
		}
		setupOpen = false;
		startReading(info, n);
	}

	// ---- Rating & notes ---------------------------------------------------------
	let notes = $state('');
	let notesFor = '';
	let saved = $state(false);
	let notesTimer: ReturnType<typeof setTimeout>;
	$effect(() => {
		if (info && notesFor !== info.book.id) {
			notesFor = info.book.id;
			notes = info.book.notes;
		}
	});
	function onNotes() {
		saved = false;
		clearTimeout(notesTimer);
		notesTimer = setTimeout(() => {
			if (!info) return;
			library.commit([{ type: 'book.set', id: info.book.id, fields: { notes } }]);
			saved = true;
		}, 700);
	}
	function rate(n: number) {
		if (!info) return;
		library.commit([
			{ type: 'book.set', id: info.book.id, fields: { rating: info.book.rating === n ? 0 : n } }
		]);
	}

	// ---- Tags ---------------------------------------------------------------
	const allTags = $derived([...new Set(liveBooks(library.view).flatMap((b) => b.tags))]);
	function addTag(tag: string) {
		if (info) library.commit([{ type: 'tag.set', book_id: info.book.id, tag, present: true }]);
	}
	function removeTag(tag: string) {
		if (info) library.commit([{ type: 'tag.set', book_id: info.book.id, tag, present: false }]);
	}

	// ---- Copies -------------------------------------------------------------
	const copies = $derived(
		[...library.view.copies.values()].filter((c) => c.book_id === id && !c.deleted)
	);
	const editions = $derived(info?.work?.editions ?? []);
	let adding = $state(false);
	let newFormat = $state('Paperback');
	let newEdition = $state('');
	let newLocation = $state('');

	function editionLabel(e: EditionRow | undefined): string {
		if (!e) return 'Unknown edition';
		return (
			[e.publisher, e.binding, e.year].filter((x) => x != null && x !== '').join(', ') || 'Edition'
		);
	}
	function addCopy(e: SubmitEvent) {
		e.preventDefault();
		if (!info) return;
		library.commit([
			{
				type: 'copy.create',
				id: uuidv7(),
				book_id: info.book.id,
				format: newFormat,
				location: newLocation.trim(),
				edition_id: newEdition || null
			}
		]);
		adding = false;
		newLocation = '';
		newEdition = '';
	}
	function removeCopy(copyId: string) {
		library.commit([{ type: 'copy.delete', id: copyId }]);
		toasts.show('Copy removed');
	}

	// ---- Activity -----------------------------------------------------------
	const perDay = $derived.by(() => {
		const m = new Map<string, number>();
		for (const rd of info?.readings ?? [])
			for (const e of eventsOf(library.view, rd.id))
				if (e.to_page > e.from_page)
					m.set(e.local_date, (m.get(e.local_date) ?? 0) + (e.to_page - e.from_page));
		return m;
	});
	const totalPages = $derived([...perDay.values()].reduce((a, b) => a + b, 0));
	const daysRead = $derived([...perDay.values()].filter((p) => p > 0).length);

	type Line = { date: string; from: number; to: number; rewind: boolean };
	function summaries(readingId: string): Line[] {
		const lines: Line[] = [];
		for (const e of eventsOf(library.view, readingId)) {
			const rewind = e.to_page < e.from_page;
			const prev = lines.at(-1);
			if (!rewind && prev && !prev.rewind && prev.date === e.local_date) prev.to = e.to_page;
			else lines.push({ date: e.local_date, from: e.from_page, to: e.to_page, rewind });
		}
		return lines.reverse();
	}

	// ---- Remove ------------------------------------------------------------------
	let confirmRemove = $state(false);
	function remove() {
		if (!info) return;
		library.commit([{ type: 'book.delete', id: info.book.id }]);
		confirmRemove = false;
		toasts.show(`Removed ${info.title} from your list`);
		goto('/catalog', { replaceState: true });
	}
	const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
</script>

<svelte:window onclick={() => (menu = false)} />

{#if !library.ready}
	<p class="pad muted">Loading…</p>
{:else if !info}
	<div class="pad">
		<p>This book isn't on your list.</p>
		<a class="link" href="/catalog">Back to the catalog</a>
	</div>
{:else}
	<div class="topbar">
		<a class="back" href="/catalog" onclick={back}>‹ Catalog</a>
		<div class="menu-wrap">
			<button
				class="more"
				type="button"
				aria-label="More actions"
				aria-haspopup="true"
				aria-expanded={menu}
				onclick={(e) => {
					e.stopPropagation();
					menu = !menu;
				}}>⋯</button
			>
			{#if menu}
				<div class="menu" role="menu">
					<button type="button" role="menuitem" onclick={openEdit}>Edit details</button>
					{#if info.work?.private && !info.work.suggested}
						<button type="button" role="menuitem" onclick={suggest}
							>Suggest for shared catalog</button
						>
					{/if}
					<button
						type="button"
						role="menuitem"
						class="danger"
						onclick={() => {
							menu = false;
							confirmRemove = true;
						}}>Remove from list</button
					>
				</div>
			{/if}
		</div>
	</div>

	<header class="head">
		<div class="art">
			<Cover title={info.title} author={info.authors[0] ?? ''} cover={info.cover} size="m" />
		</div>
		<div>
			<h1>{info.title}</h1>
			<div class="by">{info.authors.join(', ')}</div>
			{#each others as p (p.name + p.role)}<div class="pub">
					{roleLabel[p.role] ?? p.role}
					{p.name}
				</div>{/each}
			{#if info.work?.first_year}<div class="pub">First published {info.work.first_year}</div>{/if}
			{#if info.work?.private}<div class="pub">
					{info.work.suggested ? 'Suggested for the shared catalog' : 'Only you can see this entry'}
				</div>{/if}
			{#if overridden}
				<div class="pub">
					You've edited these details. <button class="link" type="button" onclick={resetOverride}
						>Reset to catalog</button
					>
				</div>
			{/if}
		</div>
	</header>

	{#if editing}
		<section class="block">
			<h2>Edit details (only for you)</h2>
			<form class="form" onsubmit={saveEdit}>
				<label class="field">Title<input bind:value={editTitle} /></label>
				<label class="field">Authors, separated by commas<input bind:value={editAuthors} /></label>
				<label class="field"
					>Cover image<input type="file" accept="image/*" bind:files={coverFile} /></label
				>
				{#if editError}<p class="error" role="alert">{editError}</p>{/if}
				<div class="row-end">
					<button class="btn" type="button" onclick={() => (editing = false)}>Cancel</button>
					<button class="btn primary" type="submit" disabled={busy}>Save</button>
				</div>
			</form>
		</section>
	{/if}

	<section class="block">
		<div class="status">
			<div>
				<span class="pill {info.state}"
					>{info.state === 'active'
						? 'Active'
						: info.state === 'finished'
							? 'Finished'
							: 'Inactive'}</span
				>
				{#if r}<div class="bar {info.state}">
						<span style="width:{Math.round((100 * r.pos) / r.total)}%"></span>
					</div>{/if}
				<div class="meta">
					{#if pagesEditing && r}
						<span class="edit">
							page <input type="number" min="0" bind:value={editPos} aria-label="Current page" />
							of <input type="number" min="1" bind:value={editTotal} aria-label="Total pages" />
							<button class="btn sm primary" type="button" onclick={savePages}>Save</button>
							<button class="btn sm" type="button" onclick={() => (pagesEditing = false)}
								>Cancel</button
							>
						</span>
					{:else if info.state === 'finished' && r?.finished_on}
						<span>Finished {fmtDate(r.finished_on)}</span>
					{:else if r}
						<button class="pos" type="button" onclick={openPages}>page {r.pos} of {r.total}</button>
						{#if pagesOn(library.view, r.id, day) > 0}<span class="today"
								>+{pagesOn(library.view, r.id, day)} pages today</span
							>{/if}
					{:else}
						<span>Not started</span>
					{/if}
				</div>
			</div>
			{#if !pagesEditing && !setupOpen}
				<div class="actions">
					{#if info.state === 'active'}
						<button
							class="pause"
							type="button"
							aria-label="Pause"
							title="Pause"
							onclick={() => pause(info)}>❚❚</button
						>
						<button class="plus" type="button" aria-label="Add 1 page" onclick={() => plusOne(info)}
							>+</button
						>
					{:else}
						<button class="play" type="button" onclick={play}
							>▶ {info.state === 'finished' ? 'Re-read' : r ? 'Resume' : 'Start'}</button
						>
					{/if}
				</div>
			{/if}
		</div>
		{#if setupOpen}
			<form class="form" onsubmit={setup}>
				<label class="field"
					>How many pages does your copy have?<input
						type="number"
						min="1"
						bind:value={setupTotal}
					/></label
				>
				{#if info.catalogPages}<button
						class="link"
						type="button"
						onclick={() => (setupTotal = String(info.catalogPages))}
						>Catalog lists {info.catalogPages} pages</button
					>{/if}
				{#if setupError}<p class="error">{setupError}</p>{/if}
				<div class="row-end">
					<button class="btn" type="button" onclick={() => (setupOpen = false)}>Cancel</button>
					<button class="btn primary" type="submit">Start reading</button>
				</div>
			</form>
		{/if}
	</section>

	<section class="block">
		<h2>Rating and notes <span class="saved">{saved ? 'Saved' : ''}</span></h2>
		<div class="stars" role="radiogroup" aria-label="Rating">
			{#each [1, 2, 3, 4, 5] as n (n)}
				<button
					type="button"
					role="radio"
					aria-checked={info.book.rating === n}
					aria-label="{n} {n === 1 ? 'star' : 'stars'}"
					class:on={n <= info.book.rating}
					onclick={() => rate(n)}>★</button
				>
			{/each}
		</div>
		<textarea
			bind:value={notes}
			oninput={onNotes}
			placeholder="Private notes about this book"
			aria-label="Notes"></textarea>
	</section>

	<section class="block">
		<h2>Tags</h2>
		<div class="tags">
			{#each info.tags as t (t)}
				<span class="tag"
					>{t}<button type="button" aria-label="Remove tag {t}" onclick={() => removeTag(t)}
						>×</button
					></span
				>
			{/each}
			<TagInput existing={allTags} current={info.tags} onadd={addTag} />
		</div>
	</section>

	<section class="block">
		<h2>
			<span>Copies{copies.length ? ' · owned' : ''}</span>
			{#if !adding}<button class="link" type="button" onclick={() => (adding = true)}
					>+ Add copy</button
				>{/if}
		</h2>
		{#if copies.length}
			<ul class="copies">
				{#each copies as c (c.id)}
					<li>
						<div>
							<b>{c.format}</b> · <span class="muted">{c.location || 'No location'}</span>
							<div class="ed">{editionLabel(editions.find((e) => e.id === c.edition_id))}</div>
						</div>
						<button
							class="x"
							type="button"
							aria-label="Remove this copy"
							onclick={() => removeCopy(c.id)}>×</button
						>
					</li>
				{/each}
			</ul>
		{:else if !adding}
			<p class="help">You don't own a copy. Add one to mark this book as owned.</p>
		{/if}
		{#if adding}
			<form class="form" onsubmit={addCopy}>
				<div class="two">
					<label class="field"
						>Format<select bind:value={newFormat}
							>{#each FORMATS as f (f)}<option>{f}</option>{/each}</select
						></label
					>
					<label class="field"
						>Edition<select bind:value={newEdition}
							>{#each editions as e (e.id)}<option value={e.id}>{editionLabel(e)}</option
								>{/each}<option value="">Unknown edition</option></select
						></label
					>
				</div>
				<label class="field"
					>Location<input bind:value={newLocation} placeholder="e.g. Office, top shelf" /></label
				>
				<div class="row-end">
					<button class="btn" type="button" onclick={() => (adding = false)}>Cancel</button>
					<button class="btn primary" type="submit">Add copy</button>
				</div>
			</form>
		{/if}
	</section>

	<section class="block">
		<h2>Reading activity</h2>
		{#if info.readings.length}
			<div class="stats">
				<span><b>{daysRead}</b> days read</span><span><b>{totalPages}</b> pages</span><span
					><b>{info.readings.length}</b> {info.readings.length === 1 ? 'reading' : 'readings'}</span
				>
			</div>
			<ActivityCalendar
				{perDay}
				today={day}
				includeYear={r && !r.finished_on ? Number(day.slice(0, 4)) : null}
			/>
			{#each [...info.readings].reverse() as rd, i (rd.id)}
				{@const lines = summaries(rd.id)}
				<details class="reading" open={i === 0}>
					<summary
						><span class="rt"
							>Reading {info.readings.length - i}{rd.finished_on ? ' · finished' : ''}</span
						><span class="muted"
							>{fmtDate(rd.started_on)} – {rd.finished_on ? fmtDate(rd.finished_on) : 'now'}</span
						></summary
					>
					<ul class="days">
						{#each lines as l, j (j)}
							<li>
								<span class="muted">{fmtMonthDay(l.date)}</span>
								{#if l.rewind}<span class="back">moved back page {l.from} → {l.to}</span><span
									></span>
								{:else}<span>page {l.from} → {l.to}</span><span class="delta">+{l.to - l.from}</span
									>{/if}
							</li>
						{:else}
							<li><span></span><span class="back">No progress yet</span><span></span></li>
						{/each}
					</ul>
				</details>
			{/each}
		{:else}
			<p class="help">No reading yet.</p>
		{/if}
	</section>

	{#if confirmRemove}
		<div class="dialog-back" role="presentation">
			<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="rm-h">
				<h3 id="rm-h">Remove {info.title}?</h3>
				<p>
					This permanently deletes {plural(info.readings.length, 'reading', 'readings')} with their activity,
					your rating, notes,
					{plural(copies.length, 'copy', 'copies')} and {plural(info.tags.length, 'tag', 'tags')}.
					This can't be undone.
				</p>
				<div class="row-end">
					<button class="btn" type="button" onclick={() => (confirmRemove = false)}>Cancel</button>
					<button class="btn danger" type="button" onclick={remove}>Remove</button>
				</div>
			</div>
		</div>
	{/if}
{/if}

<style>
	.pad {
		padding: 24px 16px;
	}
	.muted {
		color: var(--muted);
	}
	.topbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 10px 10px 4px;
	}
	.back {
		color: var(--accent);
		text-decoration: none;
		padding: 6px 8px;
	}
	.menu-wrap {
		position: relative;
	}
	.more {
		background: none;
		border: 0;
		font-size: 20px;
		padding: 2px 10px;
		border-radius: 8px;
		cursor: pointer;
		color: var(--accent);
	}
	.menu {
		position: absolute;
		right: 0;
		top: 36px;
		z-index: 10;
		background: var(--surface);
		border: 1px solid var(--line);
		border-radius: 12px;
		box-shadow: 0 8px 28px rgb(0 0 0 / 0.16);
		display: grid;
		min-width: 230px;
		padding: 4px;
	}
	.menu button {
		text-align: left;
		background: none;
		border: 0;
		padding: 9px 12px;
		border-radius: 8px;
		cursor: pointer;
	}
	.menu button:hover {
		background: var(--bg);
	}
	.menu .danger {
		color: var(--danger);
	}
	.head {
		display: grid;
		grid-template-columns: 108px minmax(0, 1fr);
		gap: 16px;
		padding: 4px 18px 18px;
		align-items: end;
	}
	.art {
		aspect-ratio: 2 / 3;
	}
	h1 {
		margin: 0;
		font-size: 24px;
		font-weight: 600;
		line-height: 1.2;
		text-wrap: balance;
	}
	.by {
		color: var(--muted);
		margin-top: 2px;
	}
	.pub {
		color: var(--muted);
		font-size: 14px;
		margin-top: 4px;
	}
	.block {
		padding: 14px 18px;
		border-top: 1px solid var(--line);
		display: grid;
		gap: 10px;
	}
	h2 {
		margin: 0;
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	h2 .link {
		text-transform: none;
		letter-spacing: 0;
		font-size: 14px;
	}
	.saved {
		text-transform: none;
		letter-spacing: 0;
		font-weight: 400;
	}
	.form {
		display: grid;
		gap: 10px;
		padding: 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
		background: var(--bg);
	}
	.two {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}
	.status {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 12px;
		align-items: center;
	}
	.pill {
		display: inline-block;
		font-size: 13px;
		font-weight: 600;
		padding: 1px 9px;
		border-radius: 999px;
		background: var(--bg);
		color: var(--muted);
	}
	.pill.active,
	.pill.finished {
		background: var(--accent-soft);
		color: var(--accent);
	}
	.bar {
		height: 4px;
		border-radius: 2px;
		background: var(--line);
		margin: 8px 0 6px;
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.bar.inactive span {
		background: var(--muted);
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 10px;
		align-items: center;
		font-size: 14px;
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
		text-decoration: underline dotted var(--muted);
		text-underline-offset: 3px;
	}
	.edit {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
		color: var(--ink);
	}
	.edit input {
		width: 72px;
		padding: 2px 6px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		background: var(--surface);
	}
	.today {
		color: var(--accent);
		font-weight: 500;
	}
	.actions {
		display: flex;
		gap: 6px;
		align-items: center;
	}
	.plus {
		width: 44px;
		height: 44px;
		border-radius: 50%;
		border: 0;
		background: var(--accent);
		color: var(--surface);
		font-size: 25px;
		cursor: pointer;
	}
	.pause {
		width: 30px;
		height: 30px;
		border-radius: 50%;
		border: 1px solid var(--line);
		background: none;
		color: var(--muted);
		font-size: 10px;
		cursor: pointer;
	}
	.play {
		height: 44px;
		padding: 0 14px;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: none;
		color: var(--accent);
		font-weight: 500;
		cursor: pointer;
	}
	.stars {
		display: flex;
		gap: 2px;
	}
	.stars button {
		background: none;
		border: 0;
		font-size: 28px;
		line-height: 1;
		padding: 0 2px;
		color: var(--line);
		cursor: pointer;
	}
	.stars button.on {
		color: var(--accent);
	}
	textarea {
		width: 100%;
		min-height: 80px;
		resize: vertical;
		padding: 9px 11px;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--surface);
		line-height: 1.45;
	}
	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		padding: 2px 4px 2px 10px;
		border-radius: 999px;
		background: var(--accent-soft);
		color: var(--accent);
		font-size: 14px;
	}
	.tag::before {
		content: '#';
		opacity: 0.6;
	}
	.tag button {
		background: none;
		border: 0;
		color: inherit;
		cursor: pointer;
		padding: 0 5px;
	}
	.copies {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 6px;
	}
	.copies li {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border: 1px solid var(--line);
		border-radius: 10px;
	}
	.copies b {
		font-weight: 600;
	}
	.ed {
		font-size: 13px;
		color: var(--muted);
	}
	.x {
		background: none;
		border: 0;
		color: var(--muted);
		cursor: pointer;
		padding: 2px 8px;
		font-size: 18px;
	}
	.stats {
		display: flex;
		gap: 14px;
		font-size: 14px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.stats b {
		color: var(--ink);
	}
	details.reading {
		border: 1px solid var(--line);
		border-radius: 10px;
	}
	summary {
		cursor: pointer;
		padding: 8px 12px;
		display: flex;
		justify-content: space-between;
		gap: 8px;
		font-size: 15px;
	}
	.rt {
		font-weight: 600;
	}
	.days {
		list-style: none;
		margin: 0;
		padding: 0 12px 10px 18px;
		display: grid;
		gap: 2px;
		font-size: 14px;
		font-variant-numeric: tabular-nums;
		max-height: 260px;
		overflow-y: auto;
	}
	.days li {
		display: grid;
		grid-template-columns: 64px minmax(0, 1fr) auto;
		gap: 8px;
	}
	.delta {
		color: var(--accent);
		font-weight: 500;
	}
	.back {
		color: var(--muted);
		font-style: italic;
	}
	.dialog-back {
		position: fixed;
		inset: 0;
		background: rgb(0 0 0 / 0.4);
		display: grid;
		place-items: center;
		padding: 20px;
		z-index: 50;
	}
	.dialog {
		background: var(--surface);
		border-radius: 14px;
		padding: 18px;
		display: grid;
		gap: 12px;
		max-width: 360px;
	}
	.dialog h3 {
		margin: 0;
		font-size: 18px;
	}
	.dialog p {
		margin: 0;
		color: var(--muted);
	}
</style>
