<script lang="ts">
	// Signed-in app shell: a bottom tab bar on phones; on wide screens a sidebar
	// docked to the left of the content, with the content centered on screen.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import Toast from '#lib/components/Toast.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';

	let { children } = $props();

	$effect(() => {
		if (!auth.me) goto('/signin', { replaceState: true });
		else library.open(auth.me.id);
	});

	const tabs = [
		{ href: '/', label: 'Reading', match: (p: string) => p === '/' },
		{
			href: '/catalog',
			label: 'Catalog',
			match: (p: string) => p.startsWith('/catalog') || p.startsWith('/add')
		},
		{ href: '/settings', label: 'Settings', match: (p: string) => p.startsWith('/settings') }
	];
</script>

{#if auth.me}
	<div class="shell">
		<nav class="tabs" aria-label="Main">
			<img class="brand" src="/logo.svg" alt="mau" width="176" height="176" />
			{#each tabs as t (t.href)}
				<a href={t.href} class="tab" aria-current={t.match(page.url.pathname) ? 'page' : undefined}>
					{#if t.label === 'Reading'}
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
	}
	.content {
		flex: 1;
		width: 100%;
		max-width: 720px;
		margin: 0 auto;
		padding-bottom: 76px;
		background: var(--surface);
	}
	.tabs {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: 20;
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		background: color-mix(in srgb, var(--surface) 92%, transparent);
		backdrop-filter: blur(12px);
		border-top: 1px solid var(--line);
		padding-bottom: env(safe-area-inset-bottom);
	}
	.brand {
		display: none;
	}
	.tab {
		display: grid;
		justify-items: center;
		gap: 3px;
		padding: 8px 0 10px;
		font-size: 12px;
		font-weight: 500;
		color: var(--muted);
		text-decoration: none;
	}
	.tab[aria-current='page'] {
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
			top: 0;
			height: 100vh;
			display: flex;
			flex-direction: column;
			gap: 2px;
			padding: 24px 12px;
			border-top: 0;
			background: none;
			backdrop-filter: none;
		}
		/* As wide as the tab buttons below it. */
		.brand {
			display: block;
			width: 100%;
			height: auto;
			margin: 0 0 20px;
		}
		.tab {
			display: flex;
			gap: 10px;
			align-items: center;
			padding: 8px 12px;
			border-radius: 10px;
			font-size: 14.5px;
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
