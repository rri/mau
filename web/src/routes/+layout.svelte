<script lang="ts">
	import '#lib/styles/theme.css';
	import favicon from '#lib/assets/favicon.svg';
	import { auth } from '#lib/auth.svelte.ts';
	import { applyStoredTheme } from '#lib/theme.ts';
	import type { LayoutProps } from './$types.ts';

	let { children }: LayoutProps = $props();

	applyStoredTheme();
	auth.refresh();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>mau</title>
</svelte:head>

{#if auth.ready}
	{@render children()}
{/if}
