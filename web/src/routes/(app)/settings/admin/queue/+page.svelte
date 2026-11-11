<script lang="ts">
	import { onMount } from 'svelte';
	import { get, post } from '#lib/api/client.ts';
	import type { QueueItem } from '#lib/api/generated/QueueItem.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';
	import WorkEditor from '#lib/components/WorkEditor.svelte';

	let items = $state<QueueItem[]>([]);
	let loaded = $state(false);
	let editing = $state<string | null>(null);
	let error = $state('');

	async function load() {
		try {
			items = await get<QueueItem[]>('/admin/queue');
		} catch (e) {
			error = (e as Error).message;
		} finally {
			loaded = true;
		}
	}
	onMount(load);

	async function act(path: string, body: unknown, message: string) {
		error = '';
		try {
			await post(path, body);
			toasts.show(message);
			editing = null;
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
	const edLine = (q: QueueItem) =>
		q.work.editions
			.map((e) =>
				[e.publisher, e.binding, e.year, e.isbn13 && `ISBN ${e.isbn13}`].filter(Boolean).join(', ')
			)
			.join('; ') || 'Unknown edition';
</script>

<PageHead title="Review queue" />
<div class="sec">
	<p class="help">
		Books that people entered by hand and suggested for the shared catalog. You see the book details
		only, never their reading.
	</p>
	<div class="cards">
		{#each items as q (q.work.id)}
			<div class="card">
				<div class="top">
					<span class="name">{q.work.title}</span><span class="meta">from @{q.owner}</span>
				</div>
				<div class="meta">
					{q.work.people
						.map((p) => (p.role === 'author' ? p.name : `${p.name} (${p.role})`))
						.join(', ')}{q.work.first_year ? ` · ${q.work.first_year}` : ''}<br />{edLine(q)}
				</div>
				{#each q.similar as s (s.id)}
					<div class="notice">
						Possibly the same as <b>{s.title}</b>{s.first_year ? ` (${s.first_year})` : ''}.
						<button
							class="link"
							type="button"
							onclick={() =>
								act(
									`/admin/queue/${q.work.id}/merge`,
									{ into: s.id },
									`Merged into ${s.title}. @${q.owner}'s book now links to it.`
								)}>Merge into it</button
						>
					</div>
				{/each}
				{#if editing === q.work.id}
					<WorkEditor
						work={q.work}
						saveLabel="Add to shared catalog"
						oncancel={() => (editing = null)}
						onsave={(edit) =>
							act(
								`/admin/queue/${q.work.id}/promote`,
								edit,
								`${edit.title} is now in the shared catalog.`
							)}
					/>
				{:else}
					<div class="acts">
						<button class="btn sm" type="button" onclick={() => (editing = q.work.id)}
							>Edit, then add</button
						>
						<button
							class="btn sm primary"
							type="button"
							onclick={() =>
								act(
									`/admin/queue/${q.work.id}/promote`,
									null,
									`${q.work.title} is now in the shared catalog.`
								)}>Add to shared catalog</button
						>
						<button
							class="btn sm danger-ghost"
							type="button"
							onclick={() =>
								act(
									`/admin/queue/${q.work.id}/decline`,
									{},
									`Declined. It stays private to @${q.owner}.`
								)}>Decline</button
						>
					</div>
				{/if}
			</div>
		{:else}
			{#if loaded && !error}<p class="help">No suggestions waiting.</p>{/if}
		{/each}
	</div>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
