<script lang="ts">
	// Signed-in app shell: a bottom tab bar on phones, a left sidebar on wide screens.
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
			<span class="brand">mau</span>
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
		max-width: 640px;
		margin: 0 auto;
		padding-bottom: 72px;
	}
	.tabs {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: 20;
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		background: var(--surface);
		border-top: 1px solid var(--line);
		padding-bottom: env(safe-area-inset-bottom);
	}
	.brand {
		display: none;
	}
	.tab {
		display: grid;
		justify-items: center;
		gap: 2px;
		padding: 8px 0 10px;
		font-size: 13px;
		color: var(--muted);
		text-decoration: none;
	}
	.tab[aria-current='page'] {
		color: var(--accent);
		font-weight: 600;
	}
	@media (min-width: 900px) {
		.shell {
			flex-direction: row;
		}
		.tabs {
			position: sticky;
			top: 0;
			height: 100vh;
			width: 200px;
			flex: none;
			display: flex;
			flex-direction: column;
			gap: 2px;
			padding: 20px 12px;
			border-top: 0;
			border-right: 1px solid var(--line);
		}
		.brand {
			display: block;
			font-size: 26px;
			font-weight: 600;
			padding: 0 10px 18px;
		}
		.tab {
			display: flex;
			gap: 10px;
			align-items: center;
			padding: 9px 10px;
			border-radius: 10px;
			font-size: 16px;
		}
		.tab[aria-current='page'] {
			background: var(--accent-soft);
		}
		.content {
			padding: 12px 0 40px;
		}
	}
</style>
