<script lang="ts">
	import { onMount } from 'svelte';
	import { del, get, put } from '#lib/api/client.ts';
	import type { GoogleBooksKey } from '#lib/api/generated/GoogleBooksKey.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let endsWith = $state<string | null>(null);
	let loaded = $state(false);
	let replacing = $state(false);
	let key = $state('');
	let busy = $state(false);
	let error = $state('');

	onMount(async () => {
		try {
			endsWith = (await get<GoogleBooksKey>('/account/google-books')).ends_with;
		} catch (err) {
			error = (err as Error).message;
		} finally {
			loaded = true;
		}
	});

	function setOn(on: boolean) {
		if (auth.me) auth.setMe({ ...auth.me, google_books: on });
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			endsWith = (await put<GoogleBooksKey>('/account/google-books', { key })).ends_with;
			setOn(true);
			key = '';
			replacing = false;
			toasts.show('Key saved');
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}

	async function remove() {
		busy = true;
		error = '';
		try {
			endsWith = (await del<GoogleBooksKey>('/account/google-books')).ends_with;
			setOn(false);
			toasts.show('Key removed');
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<PageHead title="Google Books" />

<div class="sec">
	<p class="help">
		With your own Google Books API key, searching on the Add page and matching books when you import
		use Google Books instead of the server's catalog sources. Without one, imports search Open
		Library.
	</p>
	<p class="help">
		A key is free: in the Google Cloud console, create a project, turn on the Books API, and create
		an API key. It allows 1,000 searches a day; an import uses up to three per book.
	</p>
</div>

{#if loaded}
	{#if endsWith !== null && !replacing}
		<div class="sec">
			<p><b>Key ending in …{endsWith}</b></p>
			{#if error}<p class="error" role="alert">{error}</p>{/if}
			<div class="row-end">
				<button class="btn danger-ghost" type="button" onclick={remove} disabled={busy}
					>Remove</button
				>
				<button class="btn" type="button" onclick={() => (replacing = true)} disabled={busy}
					>Replace…</button
				>
			</div>
		</div>
	{:else}
		<form class="sec" onsubmit={save}>
			<label class="field" for="gb-key"
				>API key<input
					id="gb-key"
					class="mono"
					bind:value={key}
					autocomplete="off"
					spellcheck="false"
					required
				/></label
			>
			<p class="help">mau tries one search with it before saving.</p>
			{#if error}<p class="error" role="alert">{error}</p>{/if}
			<div class="row-end">
				{#if replacing}
					<button class="btn" type="button" onclick={() => (replacing = false)}>Cancel</button>
				{/if}
				<button class="btn primary" type="submit" disabled={busy || !key.trim()}>Save key</button>
			</div>
		</form>
	{/if}
{/if}
