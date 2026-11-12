<script lang="ts">
	// Signed-in app shell: a tab bar floating above the bottom edge on phones
	// (clear of the home indicator and the browser's own bottom bar, with
	// thumb-sized tabs); on wide screens a sidebar docked to the left of the
	// content, with the content centered on screen.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import Toast from '#lib/components/Toast.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Landing from '#lib/components/Landing.svelte';
	import { origin } from '#lib/origin.svelte.ts';

	let { children } = $props();

	// Signed out: the front page shows the landing page; anything else asks
	// to sign in. Signed in, the app as usual.
	const landing = $derived(!auth.me && page.url.pathname === '/');

	// Remember the main section, so books and Add a book can lead back to it.
	$effect(() => origin.visit(page.url.pathname));

	$effect(() => {
		if (auth.me) library.open(auth.me.id);
		else if (!landing) goto('/signin', { replaceState: true });
	});

	// A book or Add a book belongs to the section the reader came from, so
	// that tab stays highlighted (matching the breadcrumb trail).
	const section = $derived.by(() => {
		const p = page.url.pathname;
		if (p.startsWith('/catalog/') || p.startsWith('/add')) return origin.section.href;
		return p;
	});
	const tabs = [
		// Phones only: on wide screens the logo is the way home.
		{ href: '/', label: 'Home', match: (p: string) => p === '/', phoneOnly: true },
		{ href: '/reading', label: 'Reading', match: (p: string) => p === '/reading' },
		{ href: '/catalog', label: 'Catalog', match: (p: string) => p === '/catalog' },
		{ href: '/settings', label: 'Settings', match: (p: string) => p.startsWith('/settings') }
	];
</script>

{#if landing}
	<Landing />
{:else if auth.me}
	<div class="shell">
		<nav class="tabs" aria-label="Main">
			<a
				class="brand"
				href="/"
				aria-label="mau home"
				aria-current={section === '/' ? 'page' : undefined}
				><img src="/logo.svg" alt="" width="176" height="176" /></a
			>
			{#each tabs as t (t.href)}
				<a
					href={t.href}
					class="tab"
					class:phone-only={t.phoneOnly}
					aria-current={t.match(section) ? 'page' : undefined}
				>
					{#if t.label === 'Home'}
						<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"
							><path
								d="M3.5 9 10 3.5 16.5 9v7.5a1 1 0 0 1-1 1h-3.5v-5h-4v5H4.5a1 1 0 0 1-1-1z"
								fill="none"
								stroke="currentColor"
								stroke-width="1.6"
								stroke-linejoin="round"
							/></svg
						>
					{:else if t.label === 'Reading'}
						<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"
							><path
								d="M3 4.5C5.5 3.5 8 3.8 10 5.2c2-1.4 4.5-1.7 7-.7v11c-2.5-1-5-.7-7 .7-2-1.4-4.5-1.7-7-.7z"
								fill="none"
								stroke="currentColor"
								stroke-width="1.6"
								stroke-linejoin="round"
							/><path d="M10 5.2v11" stroke="currentColor" stroke-width="1.6" /></svg
						>
					{:else if t.label === 'Catalog'}
						<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"
							><g fill="none" stroke="currentColor" stroke-width="1.6"
								><rect x="3" y="3" width="3" height="14" rx="1" /><rect
									x="8"
									y="3"
									width="3"
									height="14"
									rx="1"
								/><path d="M13 4.2l2.8-.8 2.2 13.4-2.8.8z" stroke-linejoin="round" /></g
							></svg
						>
					{:else}
						<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"
							><g fill="none" stroke="currentColor" stroke-width="1.6"
								><circle cx="10" cy="10" r="2.6" /><path
									d="M10 2.5v2.2M10 15.3v2.2M2.5 10h2.2M15.3 10h2.2M4.7 4.7l1.6 1.6M13.7 13.7l1.6 1.6M4.7 15.3l1.6-1.6M13.7 6.3l1.6-1.6"
								/></g
							></svg
						>
					{/if}
					<span>{t.label}</span>
				</a>
			{/each}
		</nav>
		<main class="content">
			{#if library.downloading}
				{@const d = library.downloading}
				<p class="downloading" role="status">
					Downloading your library · {d.done.toLocaleString()} of {d.total.toLocaleString()}
					books
					<span class="track"
						><span style="width:{(100 * d.done) / Math.max(1, d.total)}%"></span></span
					>
				</p>
			{/if}
			{@render children()}
		</main>
	</div>
	<Toast />
	<ConfirmDialog />
{/if}

<style>
	.shell {
		min-height: 100%;
		display: flex;
		flex-direction: column;
		/* Phones on their side: clear of the notch. */
		padding-inline: env(safe-area-inset-left) env(safe-area-inset-right);
		--tabbar-h: 62px;
		--tabbar-gap: max(14px, env(safe-area-inset-bottom));
	}
	.content {
		flex: 1;
		width: 100%;
		max-width: 720px;
		margin: 0 auto;
		/* Room to scroll the last row above the tab bar. */
		padding-bottom: calc(var(--tabbar-h) + var(--tabbar-gap) + 16px);
		background: var(--surface);
	}
	/* A new device's first download (the app works meanwhile). */
	.downloading {
		margin: 0;
		padding: 8px 20px 10px;
		display: grid;
		gap: 6px;
		font-size: 13px;
		color: var(--muted);
		background: var(--accent-soft);
	}
	.track {
		height: 3px;
		border-radius: 99px;
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		overflow: hidden;
	}
	.track span {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 0.3s;
	}
	.tabs {
		position: fixed;
		left: max(12px, env(safe-area-inset-left));
		right: max(12px, env(safe-area-inset-right));
		bottom: var(--tabbar-gap);
		z-index: 20;
		max-width: 480px;
		height: var(--tabbar-h);
		margin: 0 auto;
		padding: 4px;
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 2px;
		background: color-mix(in srgb, var(--surface) 88%, transparent);
		backdrop-filter: blur(16px);
		border: 1px solid var(--line);
		border-radius: 22px;
		box-shadow: 0 6px 24px rgb(0 0 0 / 0.14);
	}
	.brand {
		display: none;
	}
	.tab {
		display: grid;
		place-content: center;
		justify-items: center;
		gap: 3px;
		border-radius: 18px;
		font-size: 12px;
		font-weight: 500;
		color: var(--muted);
		text-decoration: none;
		-webkit-tap-highlight-color: transparent;
	}
	.tab svg {
		width: 22px;
		height: 22px;
	}
	.tab[aria-current='page'] {
		background: var(--accent-soft);
		color: var(--accent);
		font-weight: 600;
	}
	@media (min-width: 900px) {
		/* The sidebar and content side by side, centered as a pair. */
		.shell {
			display: grid;
			grid-template-columns: minmax(0, 1fr) 200px minmax(0, 720px) minmax(0, 1fr);
		}
		.tabs {
			grid-column: 2;
			position: sticky;
			inset: 0 auto auto;
			max-width: none;
			height: 100vh;
			margin: 0;
			display: flex;
			flex-direction: column;
			gap: 2px;
			padding: 24px 12px;
			border: 0;
			border-radius: 0;
			box-shadow: none;
			background: none;
			backdrop-filter: none;
		}
		/* As wide as the tab buttons below it; it's the link home. */
		.brand {
			display: block;
			margin: 0 0 20px;
			border-radius: 22.5%;
		}
		.brand img {
			display: block;
			width: 100%;
			height: auto;
		}
		.brand[aria-current='page'] img {
			outline: 2px solid var(--accent);
			outline-offset: 3px;
		}
		.tab.phone-only {
			display: none;
		}
		.tab {
			display: flex;
			place-content: normal;
			gap: 10px;
			align-items: center;
			padding: 8px 12px;
			border-radius: 10px;
			font-size: 14.5px;
		}
		.tab svg {
			width: 20px;
			height: 20px;
		}
		.tab:hover {
			color: var(--ink);
		}
		.tab[aria-current='page'] {
			background: var(--accent-soft);
			color: var(--accent);
		}
		.content {
			grid-column: 3;
			max-width: none;
			margin: 0;
			min-height: 100vh;
			padding: 8px 0 48px;
			border-left: 1px solid var(--line);
			border-right: 1px solid var(--line);
		}
	}
	@media (min-width: 1120px) {
		/* Wide enough to center the content itself, with the sidebar beside it. */
		.shell {
			grid-template-columns: minmax(0, 1fr) 200px 720px 200px minmax(0, 1fr);
		}
	}
</style>
