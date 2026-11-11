<script lang="ts">
	import { onMount } from 'svelte';
	import { del, get, patch, post } from '#lib/api/client.ts';
	import type { Share } from '#lib/api/generated/Share.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { liveBooks } from '#lib/derive.ts';
	import { library } from '#lib/library.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let shares = $state<Share[]>([]);
	let loaded = $state(false);
	let error = $state('');
	let creating = $state(false);
	let kind = $state('finished');
	let tag = $state('');
	let ratings = $state(false);

	const tags = $derived(
		[...new Set(liveBooks(library.view).flatMap((b) => b.tags))].sort((a, b) => a.localeCompare(b))
	);
	// A tag share starts on the first tag, so what's shown is what's sent.
	$effect(() => {
		if (kind === 'tag' && !tags.includes(tag)) tag = tags[0] ?? '';
	});
	const profileUrl = `${location.origin}/u/${auth.me?.username}`;
	const label = (s: Share) =>
		s.kind === 'finished'
			? 'Finished books'
			: s.kind === 'active'
				? 'Currently reading'
				: s.kind === 'catalog'
					? 'Whole catalog'
					: `#${s.tag}`;

	async function load() {
		try {
			shares = await get<Share[]>('/shares');
		} catch (e) {
			error = (e as Error).message;
		} finally {
			loaded = true;
		}
	}
	onMount(load);

	async function create(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		try {
			await post('/shares', { kind, tag: kind === 'tag' ? tag : null, include_ratings: ratings });
			creating = false;
			toasts.show('Published');
			load();
		} catch (err) {
			error = (err as Error).message;
		}
	}

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			toasts.show('Link copied');
		} catch {
			toasts.show(text);
		}
	}

	async function update(s: Share, body: object, message: string) {
		error = '';
		try {
			await patch(`/shares/${s.id}`, body);
			toasts.show(message);
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}

	async function remove(s: Share) {
		error = '';
		try {
			await del(`/shares/${s.id}`);
			toasts.show(`Stopped sharing ${label(s)}`);
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
</script>

<PageHead title="Sharing" />

<div class="sec">
	<p class="help">
		Everything is private unless you publish it here. Published items are public: they're listed on
		your profile at
		<a class="link" href="/u/{auth.me?.username}" target="_blank" rel="noopener"
			>{profileUrl.replace(/^https?:\/\//, '')}</a
		>, each has its own link, and search engines may index them. Notes are never shared. Until you
		publish something, your profile doesn't exist publicly.
	</p>
	<div class="cards">
		{#each shares as s (s.id)}
			<div class="card">
				<div class="top">
					<span class="name">{label(s)}</span><span class="pill warn">Public</span>
				</div>
				<div class="meta">
					{s.include_ratings ? 'Includes ratings' : 'No ratings'} ·
					<span class="mono">/s/{s.token}</span>
				</div>
				<div class="acts">
					<button
						class="btn sm"
						type="button"
						onclick={() => copy(`${location.origin}/s/${s.token}`)}>Copy link</button
					>
					<button
						class="btn sm"
						type="button"
						onclick={() =>
							update(
								s,
								{ include_ratings: !s.include_ratings },
								s.include_ratings ? 'Ratings hidden' : 'Ratings shown'
							)}>{s.include_ratings ? 'Hide ratings' : 'Show ratings'}</button
					>
					<button
						class="btn sm"
						type="button"
						onclick={() =>
							update(s, { new_link: true }, 'New link made. The old link no longer works.')}
						>Make a new link</button
					>
					<button class="btn sm danger-ghost" type="button" onclick={() => remove(s)}
						>Stop sharing</button
					>
				</div>
			</div>
		{:else}
			{#if loaded && !error}<p class="help">You're not sharing anything.</p>{/if}
		{/each}
	</div>
	{#if creating}
		<form class="card" onsubmit={create}>
			<label class="field"
				>What to share<select bind:value={kind}>
					<option value="finished">Finished books</option>
					<option value="active">Currently reading</option>
					<option value="tag" disabled={!tags.length}
						>{tags.length ? 'A tag…' : 'A tag… (no tags yet)'}</option
					>
					<option value="catalog">Whole catalog</option>
				</select></label
			>
			{#if kind === 'tag'}
				<label class="field"
					>Tag<select bind:value={tag}
						>{#each tags as t (t)}<option value={t}>#{t}</option>{/each}</select
					></label
				>
			{/if}
			<p class="help">Anyone can view it, no sign-in needed. It appears on your public profile.</p>
			<label class="check"
				><input type="checkbox" bind:checked={ratings} /> Include my ratings</label
			>
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (creating = false)}>Cancel</button>
				<button class="btn primary" type="submit" disabled={kind === 'tag' && !tag}>Publish</button>
			</div>
		</form>
	{:else}
		<div class="row-end">
			<button class="btn primary" type="button" onclick={() => (creating = true)}
				>Share something</button
			>
		</div>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
