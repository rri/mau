<script lang="ts">
	import { auth } from '#lib/auth.svelte.ts';
</script>

<header class="head"><h1>Reading</h1></header>
<div class="empty">
	<strong>Welcome, {auth.me?.display_name}.</strong>
	<p>Books you're reading will appear here, with a + to log each page.</p>
	<a class="btn primary" href="/add">Add your first book</a>
	<a class="btn" href="/settings/import">Import from Goodreads, StoryGraph…</a>
</div>

<style>
	.head {
		padding: 18px 18px 10px;
	}
	h1 {
		margin: 0;
		font-size: 23px;
		font-weight: 600;
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
