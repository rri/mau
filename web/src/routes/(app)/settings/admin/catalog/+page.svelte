<script lang="ts">
	import { get, put } from '#lib/api/client.ts';
	import type { EditionRow } from '#lib/api/generated/EditionRow.ts';
	import type { WorkRow } from '#lib/api/generated/WorkRow.ts';
	import { ApiError } from '#lib/api/client.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import Cover from '#lib/components/Cover.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import WorkEditor from '#lib/components/WorkEditor.svelte';

	const FORMATS = ['', 'Hardcover', 'Paperback', 'Ebook', 'Audiobook'];
	let q = $state('');
	let results = $state<WorkRow[]>([]);
	let searched = $state('');
	let editing = $state<string | null>(null);
	let editingEdition = $state<EditionRow | null>(null);
	let error = $state('');

	async function search(e?: SubmitEvent) {
		e?.preventDefault();
		error = '';
		try {
			results = await get<WorkRow[]>(`/admin/catalog?q=${encodeURIComponent(q.trim())}`);
			searched = q.trim();
		} catch (err) {
			error = (err as Error).message;
		}
	}

	async function saveWork(id: string, edit: unknown) {
		try {
			const w = await put<WorkRow>(`/admin/catalog/works/${id}`, edit);
			results = results.map((r) => (r.id === id ? w : r));
			editing = null;
			toasts.show('Saved for everyone. Recorded in the audit log.');
		} catch (err) {
			error = (err as Error).message;
		}
	}

	async function saveEdition(e: SubmitEvent) {
		e.preventDefault();
		if (!editingEdition) return;
		try {
			await put(`/admin/catalog/editions/${editingEdition.id}`, {
				...editingEdition,
				binding: editingEdition.binding || null
			});
			editingEdition = null;
			toasts.show('Edition saved');
			search();
		} catch (err) {
			error = (err as Error).message;
		}
	}

	async function cover(work: WorkRow, files: FileList | null) {
		const file = files?.[0];
		if (!file) return;
		try {
			const res = await fetch(`/api/admin/catalog/works/${work.id}/cover`, {
				method: 'POST',
				body: file,
				headers: { 'Content-Type': file.type }
			});
			const data = await res.json().catch(() => null);
			if (!res.ok || !data)
				throw new ApiError(
					res.status,
					data?.error ?? 'error',
					data?.message ?? `The server answered ${res.status}.`
				);
			results = results.map((r) => (r.id === work.id ? data : r));
			toasts.show('Cover replaced');
		} catch (err) {
			error = (err as Error).message;
		}
	}
	const edLabel = (e: EditionRow) =>
		[e.publisher, e.binding, e.year, e.isbn13, e.pages && `${e.pages} pages`]
			.filter(Boolean)
			.join(' · ') || 'Edition';
</script>

<PageHead title="Shared catalog" />
<div class="sec">
	<p class="help">
		Fix a book's details for everyone. People who changed details for themselves keep their own
		version.
	</p>
	<form class="find" onsubmit={search}>
		<input
			type="search"
			bind:value={q}
			placeholder="Title, author, or ISBN"
			aria-label="Find a book"
		/>
		<button class="btn" type="submit">Find</button>
	</form>
	<div class="cards">
		{#each results as w (w.id)}
			<div class="card">
				<div class="row">
					<span class="cov"
						><Cover title={w.title} author={w.people[0]?.name ?? ''} cover={w.cover} /></span
					>
					<div>
						<div class="name">{w.title}</div>
						<div class="meta">
							{w.people.map((p) => p.name).join(', ')}{w.first_year ? ` · ${w.first_year}` : ''} · {w
								.editions.length}
							{w.editions.length === 1 ? 'edition' : 'editions'}
						</div>
					</div>
				</div>
				{#if editing === w.id}
					<WorkEditor
						work={w}
						oncancel={() => (editing = null)}
						onsave={(edit) => saveWork(w.id, edit)}
					/>
				{:else}
					<div class="acts">
						<button class="btn sm" type="button" onclick={() => (editing = w.id)}>Edit</button>
						<label class="btn sm"
							>Replace cover<input
								class="sr-only"
								type="file"
								accept="image/*"
								onchange={(e) => cover(w, (e.currentTarget as HTMLInputElement).files)}
							/></label
						>
					</div>
				{/if}
				<details>
					<summary class="meta">Editions</summary>
					<ul class="eds">
						{#each w.editions as e (e.id)}
							<li>
								{#if editingEdition?.id === e.id}
									<form class="ed-form" onsubmit={saveEdition}>
										<input
											bind:value={editingEdition.publisher}
											placeholder="Publisher"
											aria-label="Publisher"
										/>
										<select bind:value={editingEdition.binding} aria-label="Binding"
											>{#each FORMATS as f (f)}<option value={f}>{f || 'Binding unknown'}</option
												>{/each}</select
										>
										<input
											type="number"
											bind:value={editingEdition.year}
											placeholder="Year"
											aria-label="Year"
										/>
										<input
											bind:value={editingEdition.isbn13}
											placeholder="ISBN"
											aria-label="ISBN"
										/>
										<input
											type="number"
											bind:value={editingEdition.pages}
											placeholder="Pages"
											aria-label="Pages"
										/>
										<div class="row-end">
											<button class="btn sm" type="button" onclick={() => (editingEdition = null)}
												>Cancel</button
											><button class="btn sm primary" type="submit">Save</button>
										</div>
									</form>
								{:else}
									{edLabel(e)}
									<button class="link" type="button" onclick={() => (editingEdition = { ...e })}
										>Edit</button
									>
								{/if}
							</li>
						{/each}
					</ul>
				</details>
			</div>
		{/each}
	</div>
	{#if searched && !results.length && !error}
		<p class="help">No books in the catalog match “{searched}”.</p>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
	.find {
		display: flex;
		gap: 8px;
	}
	.find input {
		flex: 1;
		padding: 9px 11px;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--surface);
	}
	.row {
		display: grid;
		grid-template-columns: 34px minmax(0, 1fr);
		gap: 10px;
		align-items: center;
	}
	.cov {
		width: 34px;
		height: 49px;
	}
	.eds {
		margin: 6px 0 0;
		padding-left: 18px;
		font-size: 14px;
		display: grid;
		gap: 4px;
	}
	.ed-form {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 6px;
	}
	.ed-form input,
	.ed-form select {
		padding: 5px 7px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--surface);
	}
	.ed-form .row-end {
		grid-column: 1 / -1;
	}
</style>
