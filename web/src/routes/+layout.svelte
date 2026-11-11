<script lang="ts">
	import '#lib/styles/theme.css';
	import { page } from '$app/state';
	import { auth } from '#lib/auth.svelte.ts';
	import { bookInfo } from '#lib/derive.ts';
	import { library } from '#lib/library.svelte.ts';
	import { applyStoredTheme } from '#lib/theme.ts';
	import { pageTitle } from '#lib/title.ts';
	import type { LayoutProps } from './$types.ts';

	let { children }: LayoutProps = $props();

	applyStoredTheme();
	auth.refresh();

	const title = $derived.by(() => {
		const id = page.url.pathname.startsWith('/catalog/') ? page.params.id : undefined;
		const b = id ? library.view.books.get(id) : undefined;
		return pageTitle(page.url.pathname, {
			signedIn: !!auth.me,
			status: page.status,
			book: b && !b.deleted ? bookInfo(library.view, b).title : null
		});
	});
</script>

<svelte:head>
	<title>{title}</title>
</svelte:head>

{#if auth.ready}
	{@render children()}
{/if}
