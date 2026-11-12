<script lang="ts">
	// Add a book and Import: without their own Google Books key, readers are
	// told that adding one gets better matches (it goes once they add it).
	import { auth } from '../auth.svelte.ts';

	let {
		what,
		inset = false
	}: {
		what: 'search' | 'import';
		/** Inside a page's side margins (the Add page). */
		inset?: boolean;
	} = $props();
</script>

{#if auth.me && !auth.me.google_books}
	<aside class="tip" class:inset aria-label="Tip">
		<strong>Get better matches with your own Google Books key.</strong>
		<span>
			{what === 'import'
				? 'Imports match your books on Open Library'
				: 'Searches use the server’s shared catalog sources'} until you add your own free Google Books
			API key, which usually finds more books, editions, and covers.
			<a href="/settings/google-books">Add a key in Settings</a> (it takes a few minutes).
		</span>
	</aside>
{/if}

<style>
	.tip {
		display: grid;
		gap: 4px;
		padding: 12px 14px;
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		border-left: 4px solid var(--accent);
		border-radius: 10px;
		background: var(--accent-soft);
		font-size: 14px;
		line-height: 1.45;
	}
	.inset {
		margin: 14px 20px 2px;
	}
	strong {
		font-weight: 600;
		color: var(--ink);
	}
	span {
		color: var(--muted);
	}
	a {
		color: var(--accent);
		font-weight: 500;
	}
</style>
