<script lang="ts">
	// Unknown addresses (and any other routing error): a plain card with the
	// way back, in the style of the sign-in pages.
	import { page } from '$app/state';
	import { auth } from '#lib/auth.svelte.ts';

	const missing = $derived(page.status === 404);
</script>

<main class="public">
	<div class="card">
		<a class="logo" href="/"><img src="/logo.svg" alt="mau" width="112" height="112" /></a>
		<h1>{missing ? 'Page not found' : 'Something went wrong'}</h1>
		<p>
			{missing
				? "There's nothing at this address. The link may be mistyped, or the page has moved."
				: (page.error?.message ?? 'The page could not be shown.')}
		</p>
		<div class="links">
			<a class="btn primary" href="/">{auth.me ? 'Go to Home' : 'Go to the front page'}</a>
		</div>
	</div>
</main>

<style>
	.public {
		min-height: 100%;
		display: grid;
		place-items: center;
		padding: 24px 16px;
	}
	.card {
		width: 100%;
		max-width: 420px;
		display: grid;
		gap: 16px;
		padding: 28px 24px;
		background: var(--surface);
		border: 1px solid var(--line);
		border-radius: 18px;
		text-align: center;
	}
	h1 {
		margin: 0;
		font-size: 24px;
		font-weight: 600;
		text-wrap: balance;
	}
	p {
		margin: 0;
		color: var(--muted);
	}
	.logo {
		display: block;
		width: max-content;
		margin: -4px auto 4px;
		border-radius: 22.5%;
	}
	.logo img {
		display: block;
	}
	.links {
		display: grid;
		justify-items: center;
	}
</style>
