<script lang="ts">
	// Adding a book: search (title, author, or ISBN), scan a barcode, or enter
	// it by hand. Picking a result opens a confirm sheet.
	import { goto } from '$app/navigation';
	import { ApiError, get, post } from '#lib/api/client.ts';
	import type { Candidate } from '#lib/api/generated/Candidate.ts';
	import type { CandidateEdition } from '#lib/api/generated/CandidateEdition.ts';
	import type { OpKind } from '#lib/api/generated/OpKind.ts';
	import type { SearchResponse } from '#lib/api/generated/SearchResponse.ts';
	import type { SearchResult } from '#lib/api/generated/SearchResult.ts';
	import type { SimilarWork } from '#lib/api/generated/SimilarWork.ts';
	import type { WorkRow } from '#lib/api/generated/WorkRow.ts';
	import { topOfInactive } from '#lib/actions.ts';
	import { uploadCover } from '#lib/covers.ts';
	import { authorsOf, liveBooks } from '#lib/derive.ts';
	import { looksLikeIsbn, normalizeIsbn } from '#lib/isbn.ts';
	import { library } from '#lib/library.svelte.ts';
	import { uuidv7 } from '#lib/sync/ids.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import Cover from '#lib/components/Cover.svelte';
	import Scanner from '#lib/components/Scanner.svelte';
	import TagInput from '#lib/components/TagInput.svelte';
	import Breadcrumbs from '#lib/components/Breadcrumbs.svelte';
	import { origin } from '#lib/origin.svelte.ts';

	const FORMATS = ['Hardcover', 'Paperback', 'Ebook', 'Audiobook'];
	const ROLES = [
		{ value: 'author', label: 'Author' },
		{ value: 'translator', label: 'Translator' },
		{ value: 'editor', label: 'Editor' },
		{ value: 'illustrator', label: 'Illustrator' }
	];

	let q = $state('');
	let searching = $state(false);
	let error = $state('');
	let res = $state<SearchResponse | null>(null);
	let scanning = $state(false);
	let mode = $state<'search' | 'manual'>('search');

	function statusLine(r: SearchResponse): string {
		const parts = [
			r.isbn ? `ISBN ${r.isbn}` : '',
			`${r.results.length} ${r.results.length === 1 ? 'result' : 'results'}`
		];
		let line = parts.filter(Boolean).join(' · ');
		if (r.sources.length) line += ` from ${r.sources.join(', then ')}`;
		if (r.errors.length) line += ` · ${r.errors[0]}`;
		return line;
	}

	const thumb = (url: string | null) =>
		url ? `/api/catalog/thumb?u=${encodeURIComponent(url)}` : null;

	async function search(e?: SubmitEvent) {
		e?.preventDefault();
		const query = q.trim();
		if (!query) return;
		if (looksLikeIsbn(query) && !normalizeIsbn(query)) {
			error = "That ISBN's check digit doesn't match. Check for a typo.";
			res = null;
			return;
		}
		searching = true;
		error = '';
		try {
			res = await get<SearchResponse>(`/catalog/search?q=${encodeURIComponent(query)}`);
			if (res.isbn && res.results.length === 1) openSheet(res.results[0], res.isbn);
		} catch (err) {
			res = null;
			error = (err as ApiError).message;
		} finally {
			searching = false;
		}
	}

	function scanned(isbn: string) {
		scanning = false;
		q = isbn;
		search();
	}

	// ---- Confirm sheet -------------------------------------------------------
	type Sheet = {
		candidate?: Candidate;
		work?: WorkRow;
		bookId: string | null;
		isbn: string | null;
		editions: CandidateEdition[];
		loadingEditions: boolean;
	};
	let sheet = $state<Sheet | null>(null);
	let chosen = $state('');
	let owned = $state(false);
	let format = $state('Paperback');
	let location = $state('');
	let tags = $state<string[]>([]);
	let adding = $state(false);
	let sheetError = $state('');
	const allTags = $derived([...new Set(liveBooks(library.view).flatMap((b) => b.tags))]);

	// Editions are chosen by their position in the list: providers sometimes
	// list the same ISBN twice, so ISBNs can't be keys.
	const pickByIsbn = (list: CandidateEdition[], isbn: string | null) =>
		isbn ? String(list.findIndex((e) => e.isbn13 === isbn)).replace('-1', '') : '';
	function editionLabel(e: {
		publisher?: string | null;
		binding?: string | null;
		year?: number | null;
		isbn13?: string | null;
	}) {
		return (
			[e.publisher, e.binding, e.year].filter((x) => x != null && x !== '').join(', ') ||
			(e.isbn13 ? `ISBN ${e.isbn13}` : 'Edition')
		);
	}

	async function openSheet(r: SearchResult, isbn: string | null = null) {
		const { book_id, ...candidate } = r;
		sheet = {
			candidate: candidate as Candidate,
			bookId: book_id,
			isbn,
			editions: candidate.editions,
			loadingEditions: true
		};
		chosen = pickByIsbn(candidate.editions, isbn);
		owned = false;
		tags = [];
		location = '';
		sheetError = '';
		format = candidate.editions.find((e) => e.isbn13 === isbn)?.binding ?? 'Paperback';
		try {
			const list = await get<CandidateEdition[]>(
				`/catalog/editions?provider=${encodeURIComponent(candidate.provider)}&source_id=${encodeURIComponent(candidate.source_id)}`
			);
			if (sheet?.candidate?.source_id === candidate.source_id) {
				const wanted = chosen ? (sheet.editions[Number(chosen)]?.isbn13 ?? null) : null;
				sheet.editions = list;
				chosen = pickByIsbn(list, wanted);
			}
		} catch {
			// Keep the editions the search returned.
		} finally {
			if (sheet) sheet.loadingEditions = false;
		}
	}

	async function openWork(id: string) {
		try {
			const work = await get<WorkRow>(`/catalog/works/${id}`);
			const existing = liveBooks(library.view).find((b) => b.book.work_id === id);
			mode = 'search';
			sheet = {
				work,
				bookId: existing?.book.id ?? null,
				isbn: null,
				editions: work.editions.map((e) => ({ ...e, source_id: e.id, cover_url: null })),
				loadingEditions: false
			};
			chosen = '';
			owned = false;
			tags = [];
			sheetError = '';
		} catch (e) {
			toasts.show((e as Error).message);
		}
	}

	async function confirm() {
		if (!sheet) return;
		adding = true;
		sheetError = '';
		try {
			const pick = chosen === '' ? undefined : sheet.editions[Number(chosen)];
			let work = sheet.work;
			if (!work && sheet.candidate) {
				work = await post<WorkRow>('/catalog/ensure', {
					provider: sheet.candidate.provider,
					source_id: sheet.candidate.source_id,
					isbn: pick?.isbn13 ?? sheet.isbn ?? null
				});
			}
			if (!work) return;
			library.addWorks([work]);
			const editionId =
				work.editions.find((e) => pick?.isbn13 && e.isbn13 === pick.isbn13)?.id ??
				work.editions.find((e) => pick && e.id === pick.source_id)?.id ??
				null;
			const ops: OpKind[] = [];
			let bookId = sheet.bookId;
			if (!bookId) {
				bookId = uuidv7();
				ops.push({
					type: 'book.create',
					id: bookId,
					work_id: work.id,
					rank: topOfInactive(),
					added_at: Date.now()
				});
				for (const t of tags) ops.push({ type: 'tag.set', book_id: bookId, tag: t, present: true });
			}
			if (owned || sheet.bookId)
				ops.push({
					type: 'copy.create',
					id: uuidv7(),
					book_id: bookId,
					format,
					location: location.trim(),
					edition_id: editionId
				});
			library.commit(ops);
			const id = bookId;
			toasts.show(
				sheet.bookId
					? `Added another copy of ${work.title}`
					: `Added ${work.title} to the top of your inactive books${owned ? ', with a copy' : ''}`,
				{ label: 'View', run: () => goto(`/catalog/${id}`) }
			);
			sheet = null;
		} catch (e) {
			sheetError = (e as ApiError).offline
				? 'Adding from the catalog needs a connection.'
				: (e as Error).message;
		} finally {
			adding = false;
		}
	}

	// ---- Manual entry --------------------------------------------------------
	let mTitle = $state('');
	let mPeople = $state([{ name: '', role: 'author' }]);
	let mYear = $state('');
	let mCover = $state<FileList | null>(null);
	let mPublisher = $state('');
	let mBinding = $state('Paperback');
	let mEdYear = $state('');
	let mIsbn = $state('');
	let mError = $state('');
	let mSimilar = $state<SimilarWork[] | null>(null);
	let mBusy = $state(false);

	function openManual() {
		mode = 'manual';
		mTitle = q.trim() && !looksLikeIsbn(q) ? q.trim() : '';
		mIsbn = looksLikeIsbn(q) ? q.trim() : '';
		mSimilar = null;
		mError = '';
	}

	const num = (s: string) => (s.trim() ? Number(s) : null);

	async function saveManual(e: SubmitEvent) {
		e.preventDefault();
		mError = '';
		const title = mTitle.trim();
		const people = mPeople
			.map((p) => ({ name: p.name.trim(), role: p.role }))
			.filter((p) => p.name);
		if (!title || !people.some((p) => p.role === 'author')) {
			mError = 'Enter at least a title and an author.';
			return;
		}
		const isbn = mIsbn.trim() ? normalizeIsbn(mIsbn) : null;
		if (mIsbn.trim() && !isbn) {
			mError = "That ISBN doesn't look right. Check for a typo, or leave it empty.";
			return;
		}
		const year = num(mYear);
		if (year !== null && !(Number.isInteger(year) && year > 0 && year < 3000)) {
			mError = 'Enter the year as a number, like 1998.';
			return;
		}
		mBusy = true;
		try {
			// Check the shared catalog for this book first (once).
			if (mSimilar === null && library.online) {
				try {
					const found = await get<SimilarWork[]>(
						`/catalog/similar?title=${encodeURIComponent(title)}`
					);
					if (found.length) {
						mSimilar = found;
						return;
					}
				} catch {
					// Offline or failed: carry on.
				}
			}
			let cover: string | null = null;
			const file = mCover?.[0];
			if (file) cover = await uploadCover(file);
			const workId = uuidv7();
			const bookId = uuidv7();
			const hasEdition = mPublisher.trim() || mEdYear.trim() || isbn;
			const ops: OpKind[] = [
				{
					type: 'work.create',
					work: {
						id: workId,
						title,
						first_year: year,
						people,
						edition: hasEdition
							? {
									id: uuidv7(),
									publisher: mPublisher.trim() || null,
									binding: mBinding,
									year: num(mEdYear),
									isbn13: isbn,
									pages: null
								}
							: null
					}
				},
				{
					type: 'book.create',
					id: bookId,
					work_id: workId,
					rank: topOfInactive(),
					added_at: Date.now()
				}
			];
			if (cover) ops.push({ type: 'book.set', id: bookId, fields: { cover_override: cover } });
			library.commit(ops);
			toasts.show(`Added ${title} (private to you) to the top of your inactive books`, {
				label: 'View',
				run: () => goto(`/catalog/${bookId}`)
			});
			mode = 'search';
			mTitle = '';
			mPeople = [{ name: '', role: 'author' }];
			mYear = mPublisher = mEdYear = mIsbn = '';
			mCover = null;
			mSimilar = null;
		} catch (err) {
			mError = (err as Error).message;
		} finally {
			mBusy = false;
		}
	}
</script>

<div class="topbar">
	{#if mode === 'manual'}
		<Breadcrumbs
			items={[
				{ label: origin.section.label, href: origin.section.href },
				{ label: 'Add a book', onclick: () => (mode = 'search') },
				{ label: 'Enter a book' }
			]}
		/>
		<h1>Enter a book</h1>
	{:else}
		<Breadcrumbs
			items={[{ label: origin.section.label, href: origin.section.href }, { label: 'Add a book' }]}
		/>
		<h1>Add a book</h1>
	{/if}
</div>

{#if mode === 'search'}
	<form class="searchbar" onsubmit={search}>
		<input
			class="search"
			type="search"
			enterkeyhint="search"
			bind:value={q}
			placeholder="Title, author, or ISBN"
			aria-label="Search by title, author, or ISBN"
			autocomplete="off"
		/>
		<button class="btn" type="button" onclick={() => (scanning = true)} aria-label="Scan barcode"
			>Scan</button
		>
	</form>

	{#if !library.online}
		<p class="note">
			Searching the catalog needs a connection. You can still enter a book by hand.
		</p>
	{/if}
	{#if searching}
		<p class="note">Searching…</p>
	{:else if error}
		<p class="note error">{error}</p>
	{:else if res}
		<p class="note">{statusLine(res)}</p>
		<ul class="results">
			{#each res.results as r (r.provider + r.source_id)}
				<li>
					<button class="result" type="button" onclick={() => openSheet(r, res?.isbn ?? null)}>
						<span class="cov"
							><Cover title={r.title} author={r.authors[0] ?? ''} src={thumb(r.cover_url)} /></span
						>
						<span class="info">
							<span class="title">{r.title}</span>
							<span class="author">{r.authors.join(', ')}</span>
							<span class="sub"
								>{[
									r.first_year ?? 'Year unknown',
									r.edition_count
										? `${r.edition_count} ${r.edition_count === 1 ? 'edition' : 'editions'}`
										: ''
								]
									.filter(Boolean)
									.join(' · ')}</span
							>
						</span>
						{#if r.book_id}<span class="on-list">On your list</span>{:else}<span class="chev"
								>›</span
							>{/if}
					</button>
				</li>
			{:else}
				<li class="note">No books found for "{q}".</li>
			{/each}
		</ul>
	{:else}
		<p class="note">
			Type a title, author, or ISBN and press Enter. Or tap Scan to read a barcode.
		</p>
	{/if}
	<p class="manual">
		Can't find it? <button class="link" type="button" onclick={openManual}>Enter it manually</button
		>
	</p>
{:else}
	<form class="form" onsubmit={saveManual}>
		<label class="field">Title (required)<input bind:value={mTitle} /></label>
		{#each mPeople as p, i (i)}
			<div class="person">
				<label class="field"
					>{i === 0 ? 'Author (required)' : 'Name'}<input bind:value={p.name} /></label
				>
				{#if i > 0}
					<label class="field"
						>Role<select bind:value={p.role}
							>{#each ROLES as r (r.value)}<option value={r.value}>{r.label}</option>{/each}</select
						></label
					>
					<button
						class="x"
						type="button"
						aria-label="Remove this person"
						onclick={() => (mPeople = mPeople.filter((_, j) => j !== i))}>×</button
					>
				{/if}
			</div>
		{/each}
		<button
			class="link start"
			type="button"
			onclick={() => (mPeople = [...mPeople, { name: '', role: 'author' }])}
			>+ Add another person</button
		>
		<label class="field"
			>First published year<input
				bind:value={mYear}
				inputmode="numeric"
				placeholder="e.g. 1998"
			/></label
		>
		<label class="field"
			>Cover image (optional; otherwise a cover is generated)<input
				type="file"
				accept="image/*"
				bind:files={mCover}
			/></label
		>
		<details class="ed">
			<summary>Edition details (optional)</summary>
			<div class="ed-fields">
				<div class="two">
					<label class="field">Publisher<input bind:value={mPublisher} /></label>
					<label class="field"
						>Binding<select bind:value={mBinding}
							>{#each FORMATS as f (f)}<option>{f}</option>{/each}</select
						></label
					>
				</div>
				<div class="two">
					<label class="field">Edition year<input bind:value={mEdYear} inputmode="numeric" /></label
					>
					<label class="field">ISBN<input bind:value={mIsbn} inputmode="numeric" /></label>
				</div>
			</div>
		</details>
		<p class="private">
			Only you will see this book unless you suggest it for the shared catalog later.
		</p>
		{#if mSimilar?.length}
			<div class="similar">
				<strong>Is it one of these?</strong>
				<span class="help"
					>These are already in the catalog. Pick one to add it instead, or continue with your own
					entry.</span
				>
				{#each mSimilar as w (w.id)}
					<button class="sim" type="button" onclick={() => openWork(w.id)}>
						<span class="cov"
							><Cover title={w.title} author={w.authors[0] ?? ''} cover={w.cover} /></span
						>
						<span
							><span class="title">{w.title}</span><br /><span class="author"
								>{w.authors.join(', ')}{w.first_year ? ` · ${w.first_year}` : ''}</span
							></span
						>
					</button>
				{/each}
			</div>
		{/if}
		{#if mError}<p class="error" role="alert">{mError}</p>{/if}
		<div class="row-end">
			<button class="btn" type="button" onclick={() => (mode = 'search')}>Cancel</button>
			<button class="btn primary" type="submit" disabled={mBusy}
				>{mSimilar?.length ? 'No, add my book' : 'Continue'}</button
			>
		</div>
	</form>
{/if}

{#if scanning}
	<Scanner onfound={scanned} onclose={() => (scanning = false)} />
{/if}

{#if sheet}
	{@const title = sheet.candidate?.title ?? sheet.work?.title ?? ''}
	{@const authors = sheet.candidate?.authors ?? authorsOf(sheet.work)}
	{@const year = sheet.candidate?.first_year ?? sheet.work?.first_year}
	<div
		class="sheet-back"
		role="presentation"
		onclick={(e) => e.target === e.currentTarget && (sheet = null)}
	>
		<div class="sheet" role="dialog" aria-modal="true" aria-label="Add {title}">
			<div class="grab"></div>
			<div class="sheet-head">
				<span class="cov big"
					><Cover
						{title}
						author={authors[0] ?? ''}
						src={thumb(sheet.candidate?.cover_url ?? null)}
						cover={sheet.work?.cover ?? null}
					/></span
				>
				<div>
					<div class="title">{title}</div>
					<div class="author">{authors.join(', ')}{year ? ` · ${year}` : ''}</div>
				</div>
			</div>
			{#if sheet.bookId}
				<p class="help">
					This book is already on your list. Add another copy, or <a
						class="link"
						href="/catalog/{sheet.bookId}">open its page</a
					>.
				</p>
			{/if}
			<label class="field"
				>Edition
				<select bind:value={chosen}>
					<option value=""
						>{sheet.loadingEditions ? 'Loading editions…' : 'Not sure / choose later'}</option
					>
					{#each sheet.editions as e, i (i)}<option value={String(i)}>{editionLabel(e)}</option
						>{/each}
				</select></label
			>
			{#if !sheet.bookId}
				<label class="toggle"
					><span>I own a copy</span><input type="checkbox" bind:checked={owned} /></label
				>
			{/if}
			{#if owned || sheet.bookId}
				<div class="two">
					<label class="field"
						>Format<select bind:value={format}
							>{#each FORMATS as f (f)}<option>{f}</option>{/each}</select
						></label
					>
					<label class="field"
						>Location<input bind:value={location} placeholder="e.g. Office shelf" /></label
					>
				</div>
			{/if}
			{#if !sheet.bookId}
				<div class="field">
					Tags
					<div class="tags">
						{#each tags as t (t)}<span class="tag"
								>{t}<button
									type="button"
									aria-label="Remove tag {t}"
									onclick={() => (tags = tags.filter((x) => x !== t))}>×</button
								></span
							>{/each}
						<TagInput existing={allTags} current={tags} onadd={(t) => (tags = [...tags, t])} />
					</div>
				</div>
			{/if}
			{#if sheetError}<p class="error" role="alert">{sheetError}</p>{/if}
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (sheet = null)}>Cancel</button>
				<button class="btn primary" type="button" onclick={confirm} disabled={adding}
					>{sheet.bookId ? 'Add copy' : 'Add to my list'}</button
				>
			</div>
		</div>
	</div>
{/if}

<style>
	.topbar {
		display: grid;
		gap: 10px;
		padding: 18px 20px 8px;
	}
	.topbar h1 {
		margin: 0;
		font-size: 24px;
		font-weight: 600;
	}
	.searchbar {
		display: flex;
		gap: 8px;
		padding: 6px 14px 12px;
		border-bottom: 1px solid var(--line);
	}
	.search {
		flex: 1;
		min-width: 0;
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--surface);
		font-size: 16px;
	}
	.note {
		padding: 10px 16px 0;
		margin: 0;
		font-size: 14px;
		color: var(--muted);
	}
	.results {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
	}
	.result {
		width: 100%;
		display: grid;
		grid-template-columns: 40px minmax(0, 1fr) auto;
		gap: 12px;
		align-items: center;
		padding: 9px 14px;
		border: 0;
		border-bottom: 1px solid var(--line);
		background: none;
		text-align: left;
		cursor: pointer;
	}
	.result:hover {
		background: var(--surface);
	}
	.cov {
		width: 40px;
		height: 58px;
		display: block;
	}
	.cov.big {
		width: 56px;
		height: 81px;
	}
	.info {
		display: grid;
	}
	.title {
		font-weight: 600;
		font-size: 16px;
		line-height: 1.2;
	}
	.author,
	.sub {
		font-size: 13px;
		color: var(--muted);
	}
	.on-list {
		font-size: 12px;
		font-weight: 600;
		color: var(--accent);
		border: 1px solid currentColor;
		border-radius: 999px;
		padding: 1px 7px;
		white-space: nowrap;
	}
	.chev {
		color: var(--muted);
	}
	.manual {
		padding: 14px 16px;
		color: var(--muted);
	}
	.form {
		display: grid;
		gap: 12px;
		padding: 12px 16px 24px;
	}
	.person {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 130px auto;
		gap: 6px;
		align-items: end;
	}
	.person .field:only-child {
		grid-column: 1 / -1;
	}
	.start {
		justify-self: start;
	}
	.x {
		background: none;
		border: 0;
		color: var(--muted);
		font-size: 20px;
		padding: 8px;
		cursor: pointer;
	}
	.two {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}
	details.ed {
		border: 1px solid var(--line);
		border-radius: 10px;
		padding: 8px 12px;
	}
	.ed-fields {
		display: grid;
		gap: 10px;
		margin-top: 10px;
	}
	.private {
		margin: 0;
		font-size: 14px;
		color: var(--muted);
		background: var(--surface);
		border-radius: 10px;
		padding: 10px 12px;
	}
	.similar {
		display: grid;
		gap: 8px;
		padding: 12px;
		border: 1px solid var(--accent);
		border-radius: 12px;
		background: var(--accent-soft);
	}
	.sim {
		display: grid;
		grid-template-columns: 40px minmax(0, 1fr);
		gap: 10px;
		align-items: center;
		text-align: left;
		padding: 6px;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--surface);
		cursor: pointer;
	}
	.sheet-back {
		position: fixed;
		inset: 0;
		z-index: 50;
		background: rgb(0 0 0 / 0.35);
		display: flex;
		align-items: flex-end;
		justify-content: center;
	}
	.sheet {
		grid-template-columns: minmax(0, 1fr);
		width: 100%;
		max-width: 560px;
		max-height: 92%;
		overflow-y: auto;
		background: var(--surface);
		border-radius: 18px 18px 0 0;
		padding: 8px 16px calc(16px + env(safe-area-inset-bottom));
		display: grid;
		gap: 12px;
	}
	.grab {
		width: 40px;
		height: 4px;
		border-radius: 2px;
		background: var(--line);
		justify-self: center;
	}
	.sheet-head {
		display: grid;
		grid-template-columns: 56px minmax(0, 1fr);
		gap: 12px;
		align-items: center;
	}
	.toggle {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-radius: 10px;
		cursor: pointer;
	}
	.toggle input {
		width: 18px;
		height: 18px;
		accent-color: var(--accent);
	}
	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
		color: var(--ink);
	}
	.tag {
		display: inline-flex;
		align-items: center;
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
	}
</style>
