<script lang="ts">
	// The front page for visitors who aren't signed in: the logo, the name, a
	// random line of public-domain science fiction, and the way in.
	import '@fontsource/pinyon-script/latin-400.css';
	import { onMount } from 'svelte';
	import { get } from '../api/client.ts';
	import type { Quote } from '../api/generated/Quote.ts';
	import { auth } from '../auth.svelte.ts';

	let quote = $state<Quote | null>(null);

	onMount(async () => {
		try {
			quote = await get<Quote>('/quote');
		} catch {
			// Offline or the server is down: the page simply goes without one.
		}
	});

	const signupOpen = $derived(auth.status?.signup_mode === 'open' && !auth.status.setup_required);
</script>

<svelte:head>
	<title>mau · your personal library and reading log</title>
	<meta name="theme-color" content="#12161a" />
</svelte:head>

<div class="landing">
	<main>
		<img class="badge" src="/logo.svg" alt="" width="220" height="220" />
		<div class="name">
			<h1>mau</h1>
			<p class="tagline">your personal library and reading log</p>
		</div>

		<!-- Reserves its space so the page doesn't jump when the quote arrives. -->
		<figure class="quote" aria-live="polite">
			{#if quote}
				<blockquote>“{quote.text}”</blockquote>
				<figcaption>
					{quote.author},
					<a href="https://www.gutenberg.org/ebooks/{quote.gutenberg}" rel="noopener"
						><cite>{quote.title}</cite></a
					>{#if quote.year}, {quote.year}{/if}
				</figcaption>
			{/if}
		</figure>

		<div class="actions">
			<a class="primary" href="/signin">Sign in</a>
			{#if signupOpen}<a class="secondary" href="/signup">Create account</a>{/if}
		</div>
	</main>

	<footer>
		<a href="https://github.com/rri/mau" rel="noopener">github.com/rri/mau</a>
		<span aria-hidden="true">·</span>
		<span>Source code licensed under AGPL-3.0</span>
	</footer>
</div>

<style>
	/* One deliberate look in the logo's own navy, in light and dark mode alike. */
	.landing {
		--navy: #12161a;
		--paper: #f7f3eb;
		--cream: #e6dfd5;
		--soft: #c3ccc7;
		--dim: #9aa59f;
		--amber: #d89448;
		--ring: #2c343b;
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		background: radial-gradient(120% 80% at 50% 0%, #1e242b 0%, var(--navy) 60%);
		color: var(--soft);
		color-scheme: dark;
	}
	main {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 28px;
		padding: 56px max(20px, env(safe-area-inset-left, 0px)) 40px
			max(20px, env(safe-area-inset-right, 0px));
		text-align: center;
	}
	.badge {
		display: block;
		width: min(220px, 56vw);
		height: auto;
		border-radius: 22.5%;
		outline-color: var(--ring);
		box-shadow: 0 30px 80px rgb(0 0 0 / 0.5);
	}
	.name {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
	}
	h1 {
		margin: 0;
		padding: 0 0.15em;
		font-family: 'Pinyon Script', 'Snell Roundhand', 'Apple Chancery', cursive;
		font-weight: 400;
		font-size: clamp(120px, 26vw, 210px);
		line-height: 1;
		letter-spacing: 0;
		color: var(--paper);
	}
	.tagline {
		margin: 0;
		font-size: 20px;
		font-variant-caps: small-caps;
		letter-spacing: 0.08em;
		color: var(--soft);
	}
	.quote {
		margin: 8px 0 0;
		max-width: 36em;
		min-height: 8.5em;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 10px;
	}
	blockquote {
		margin: 0;
		font-size: 18px;
		line-height: 1.55;
		font-style: italic;
		color: var(--cream);
		text-wrap: balance;
	}
	figcaption {
		font-size: 13.5px;
		letter-spacing: 0.02em;
		color: var(--dim);
	}
	figcaption a {
		color: var(--soft);
		text-decoration: underline;
		text-decoration-color: rgb(195 204 199 / 0.4);
		text-underline-offset: 3px;
	}
	figcaption a:hover {
		color: var(--paper);
	}
	cite {
		font-style: italic;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 12px;
	}
	.actions a {
		display: inline-flex;
		align-items: center;
		min-height: 48px;
		padding: 0 26px;
		border-radius: 12px;
		font-size: 16px;
		font-weight: 600;
		text-decoration: none;
	}
	.primary {
		background: var(--amber);
		color: #1e140c;
	}
	.primary:hover {
		filter: brightness(1.08);
	}
	.secondary {
		border: 1px solid #3a444c;
		color: var(--paper);
	}
	.secondary:hover {
		border-color: var(--soft);
	}
	.actions a:focus-visible,
	figcaption a:focus-visible,
	footer a:focus-visible {
		outline: 2px solid var(--amber);
		outline-offset: 3px;
	}
	footer {
		display: flex;
		flex-wrap: nowrap;
		justify-content: center;
		align-items: center;
		gap: 4px 12px;
		padding: 20px max(20px, env(safe-area-inset-left, 0px))
			calc(28px + env(safe-area-inset-bottom, 0px)) max(20px, env(safe-area-inset-right, 0px));
		/* Small enough on phones that both parts and the dot share one line. */
		font-size: clamp(12px, 3.2vw, 13.5px);
		white-space: nowrap;
		color: var(--dim);
	}
	@media (max-width: 359px) {
		footer {
			flex-direction: column;
			white-space: normal;
		}
		footer span[aria-hidden] {
			display: none;
		}
	}
	footer a {
		color: var(--soft);
		text-underline-offset: 3px;
	}
</style>
