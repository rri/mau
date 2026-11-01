<script lang="ts">
	import { onDestroy } from 'svelte';
	import { get, post } from '#lib/api/client.ts';
	import type { CommitResult } from '#lib/api/generated/CommitResult.ts';
	import type { CsvPreview } from '#lib/api/generated/CsvPreview.ts';
	import type { ImportStarted } from '#lib/api/generated/ImportStarted.ts';
	import type { JobStatus } from '#lib/api/generated/JobStatus.ts';
	import type { RowStatus } from '#lib/api/generated/RowStatus.ts';
	import { library } from '#lib/library.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	const SOURCES = [
		{ v: 'goodreads', l: 'Goodreads (CSV)' },
		{ v: 'storygraph', l: 'StoryGraph (CSV)' },
		{ v: 'librarything', l: 'LibraryThing (TSV or JSON)' },
		{ v: 'mau', l: 'mau (full JSON export)' },
		{ v: 'csv', l: 'Other CSV (map columns)' }
	];
	const FIELD_LABELS: Record<string, string> = {
		title: 'Title',
		authors: 'Authors',
		isbn: 'ISBN',
		state: 'State',
		finished: 'Finished date',
		rating: 'Rating',
		tags: 'Tags',
		owned: 'Owned',
		pages: 'Pages'
	};
	const KINDS = [
		{ k: 'matched', l: 'Matched' },
		{ k: 'uncertain', l: 'Check match' },
		{ k: 'unmatched', l: 'Not found' },
		{ k: 'dupe', l: 'Already on list' }
	] as const;

	let source = $state('goodreads');
	let files = $state<FileList | null>(null);
	let text = '';
	let step = $state<'choose' | 'map' | 'matching' | 'review'>('choose');
	let preview = $state<CsvPreview | null>(null);
	let mapping = $state<Record<string, string>>({});
	let job = $state<string | null>(null);
	let progress = $state({ done: 0, total: 0 });
	let rows = $state<RowStatus[]>([]);
	let tab = $state<(typeof KINDS)[number]['k']>('matched');
	let choices = $state<Record<number, string>>({});
	let error = $state('');
	let busy = $state(false);
	let poll: ReturnType<typeof setTimeout> | undefined;
	onDestroy(() => clearTimeout(poll));

	function guess(header: string): string {
		const h = header.toLowerCase();
		if (/isbn/.test(h)) return 'isbn';
		if (/title/.test(h)) return 'title';
		if (/author|writer/.test(h)) return 'authors';
		if (/status|state|shelf/.test(h)) return 'state';
		if (/(date).*(read|done|finish)|(finish|done|read).*date/.test(h)) return 'finished';
		if (/rating|stars/.test(h)) return 'rating';
		if (/tag|label|shel/.test(h)) return 'tags';
		if (/own|have/.test(h)) return 'owned';
		if (/page/.test(h)) return 'pages';
		return '';
	}

	async function read() {
		error = '';
		const file = files?.[0];
		if (!file) {
			error = 'Choose the file you exported.';
			return;
		}
		text = await file.text();
		if (source === 'csv') {
			try {
				preview = await post<CsvPreview>('/import/preview', { text });
				const m: Record<string, string> = {};
				for (const h of preview.headers) {
					const f = guess(h);
					if (f && !Object.values(m).includes(f)) m[h] = f;
				}
				mapping = m;
				step = 'map';
			} catch (e) {
				error = (e as Error).message;
			}
			return;
		}
		start(null);
	}

	async function start(map: Record<string, string> | null) {
		busy = true;
		error = '';
		try {
			const fieldToColumn = map
				? Object.fromEntries(
						Object.entries(map)
							.filter(([, f]) => f)
							.map(([c, f]) => [f, c])
					)
				: null;
			const s = await post<ImportStarted>('/import', { source, text, mapping: fieldToColumn });
			job = s.job;
			progress = { done: 0, total: s.total };
			step = 'matching';
			check();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}

	async function check() {
		if (!job) return;
		try {
			const st = await get<JobStatus>(`/import/${job}`);
			progress = { done: st.done, total: st.total };
			if (st.rows) {
				rows = st.rows;
				const first = KINDS.find((k) => st.rows!.some((r) => r.kind === k.k));
				tab = first?.k ?? 'matched';
				step = 'review';
				return;
			}
		} catch (e) {
			error = (e as Error).message;
			return;
		}
		poll = setTimeout(check, 800);
	}

	const count = (k: string) => rows.filter((r) => r.kind === k).length;
	const toImport = $derived(
		rows.filter((r) => r.kind !== 'dupe' && choices[r.index] !== 'skip').length
	);

	async function commit() {
		if (!job) return;
		busy = true;
		error = '';
		try {
			const list = Object.entries(choices).map(([i, v]) => {
				const index = Number(i);
				if (v === 'skip' || v === 'manual') return { index, action: v, candidate: null };
				return { index, action: 'match', candidate: Number(v) };
			});
			const r = await post<CommitResult>(`/import/${job}/commit`, { choices: list });
			await library.sync();
			toasts.show(
				`Imported ${r.imported} ${r.imported === 1 ? 'book' : 'books'}. Unfinished ones are at the top of your inactive books.`
			);
			reset();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}

	function reset() {
		step = 'choose';
		job = null;
		rows = [];
		choices = {};
		preview = null;
		files = null;
	}
</script>

<PageHead title="Import & export" />

<div class="sec">
	<h2>Import</h2>
	{#if !library.online}
		<p class="notice">Importing needs a connection.</p>
	{:else if step === 'choose'}
		<label class="field"
			>Import from<select bind:value={source}
				>{#each SOURCES as s (s.v)}<option value={s.v}>{s.l}</option>{/each}</select
			></label
		>
		<label class="field">File<input type="file" accept=".csv,.tsv,.txt,.json" bind:files /></label>
		<p class="help">
			Nothing changes until you review and confirm. Books you haven't finished come in as inactive.
		</p>
		<div class="row-end">
			<button class="btn primary" type="button" onclick={read} disabled={busy}>Read file</button>
		</div>
	{:else if step === 'map' && preview}
		<p class="help">Match your file's columns to mau fields. Needs Title and Authors, or ISBN.</p>
		<div class="map">
			{#each preview.headers as h, i (h + i)}
				<code>{h}</code>
				<select bind:value={mapping[h]} aria-label="mau field for {h}">
					<option value="">(ignore)</option>
					{#each preview.fields as f (f)}<option value={f}>{FIELD_LABELS[f] ?? f}</option>{/each}
				</select>
				<span class="sample">{preview.samples[0]?.[i] ?? ''}</span>
			{/each}
		</div>
		<p class="help">
			State values like read, finished, done, or yes mean finished; anything else comes in inactive.
		</p>
		<div class="row-end">
			<button class="btn" type="button" onclick={reset}>Cancel</button>
			<button class="btn primary" type="button" onclick={() => start(mapping)} disabled={busy}
				>Continue</button
			>
		</div>
	{:else if step === 'matching'}
		<p>Matching books with the catalog… {progress.done} of {progress.total}</p>
		<div class="meter">
			<span style="width:{progress.total ? (100 * progress.done) / progress.total : 0}%"></span>
		</div>
	{:else if step === 'review'}
		<p class="help">{rows.length} books in the file. Review, then import.</p>
		<div class="tabs">
			{#each KINDS as k (k.k)}
				<button class="chip" type="button" aria-pressed={tab === k.k} onclick={() => (tab = k.k)}
					>{k.l} {count(k.k)}</button
				>
			{/each}
		</div>
		<ul class="rows">
			{#each rows.filter((r) => r.kind === tab) as r (r.index)}
				<li>
					<div>
						{r.title}
						<div class="sub">
							{r.authors.join(', ')} · {r.summary}{r.tags.length ? ` · #${r.tags.join(' #')}` : ''}
						</div>
					</div>
					{#if r.kind === 'uncertain'}
						<select bind:value={choices[r.index]} aria-label="Choose a match">
							{#each r.candidates as c, i (i)}<option value={String(i)}
									>{c.title} — {c.authors[0] ?? ''}{c.first_year
										? ` (${c.first_year})`
										: ''}</option
								>{/each}
							<option value="manual">Not this book: add as entered</option>
							<option value="skip">Skip</option>
						</select>
					{:else if r.kind === 'unmatched'}
						<select bind:value={choices[r.index]} aria-label="What to do">
							<option value="manual">Add as manual entry</option>
							<option value="skip">Skip</option>
						</select>
					{:else if r.kind === 'dupe'}
						<span class="pill">Skipped</span>
					{:else}
						<label class="check"
							><input
								type="checkbox"
								checked={choices[r.index] !== 'skip'}
								onchange={(e) =>
									(choices[r.index] = (e.currentTarget as HTMLInputElement).checked ? '0' : 'skip')}
							/> Import</label
						>
					{/if}
				</li>
			{/each}
		</ul>
		<div class="row-end">
			<button class="btn" type="button" onclick={reset}>Cancel</button>
			<button class="btn primary" type="button" onclick={commit} disabled={busy || toImport === 0}
				>Import {toImport} {toImport === 1 ? 'book' : 'books'}</button
			>
		</div>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<div class="sec">
	<h2>Export</h2>
	<p class="help">
		The full export has everything, including daily reading history, and can be imported back into
		mau. The spreadsheet has one row per book.
	</p>
	<div class="row-end">
		<a class="btn" href="/api/export.csv" download>Spreadsheet (CSV)</a>
		<a class="btn primary" href="/api/export.json" download>Full export (JSON)</a>
	</div>
</div>

<style>
	.map {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr);
		gap: 8px;
		align-items: center;
		font-size: 14px;
	}
	code {
		background: var(--surface);
		padding: 2px 6px;
		border-radius: 5px;
		overflow-wrap: anywhere;
	}
	.sample {
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	select {
		padding: 5px 7px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--surface);
		max-width: 100%;
	}
	.meter {
		height: 8px;
		border-radius: 4px;
		background: var(--line);
		overflow: hidden;
	}
	.meter span {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 0.3s;
	}
	.tabs {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.chip {
		padding: 4px 11px;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: var(--surface);
		font-size: 14px;
		cursor: pointer;
	}
	.chip[aria-pressed='true'] {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--surface);
	}
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.rows li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 8px;
		align-items: center;
		padding: 8px 0;
		border-bottom: 1px solid var(--line);
	}
	.rows select {
		max-width: 200px;
	}
	.sub {
		font-size: 13px;
		color: var(--muted);
	}
</style>
