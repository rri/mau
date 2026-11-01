<script lang="ts">
	// Edits a work's title, people, and year (admin).
	import { untrack } from 'svelte';
	import type { WorkRow } from '../api/generated/WorkRow.ts';

	let {
		work,
		onsave,
		oncancel,
		saveLabel = 'Save'
	}: {
		work: WorkRow;
		onsave: (edit: {
			title: string;
			people: { name: string; role: string }[];
			first_year: number | null;
		}) => void;
		oncancel: () => void;
		saveLabel?: string;
	} = $props();

	// The form starts from the work as it was when opened.
	let title = $state(untrack(() => work.title));
	let people = $state(untrack(() => work.people.map((p) => ({ ...p }))));
	let year = $state(untrack(() => (work.first_year ? String(work.first_year) : '')));
	const ROLES = ['author', 'translator', 'editor', 'illustrator'];

	function submit(e: SubmitEvent) {
		e.preventDefault();
		onsave({
			title,
			people: people.filter((p) => p.name.trim()),
			first_year: year.trim() ? Number(year) : null
		});
	}
</script>

<form class="editor" onsubmit={submit}>
	<label class="field">Title<input bind:value={title} /></label>
	{#each people as p, i (i)}
		<div class="person">
			<input bind:value={p.name} aria-label="Name" />
			<select bind:value={p.role} aria-label="Role"
				>{#each ROLES as r (r)}<option value={r}>{r}</option>{/each}</select
			>
			<button
				class="btn sm"
				type="button"
				onclick={() => (people = people.filter((_, j) => j !== i))}
				aria-label="Remove">×</button
			>
		</div>
	{/each}
	<button
		class="link"
		type="button"
		onclick={() => (people = [...people, { name: '', role: 'author' }])}>+ Add person</button
	>
	<label class="field">First published<input bind:value={year} inputmode="numeric" /></label>
	<div class="row-end">
		<button class="btn" type="button" onclick={oncancel}>Cancel</button>
		<button class="btn primary" type="submit">{saveLabel}</button>
	</div>
</form>

<style>
	.editor {
		display: grid;
		gap: 8px;
	}
	.person {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 120px auto;
		gap: 6px;
	}
	.person input,
	.person select {
		padding: 6px 8px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--surface);
	}
	.link {
		justify-self: start;
	}
</style>
