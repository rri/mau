<script lang="ts">
	// Settings pages: a breadcrumb trail (Settings › [Admin ›] page) above the title.
	import { page } from '$app/state';
	import Breadcrumbs, { type Crumb } from './Breadcrumbs.svelte';

	let { title }: { title: string } = $props();

	const items = $derived<Crumb[]>([
		{ label: 'Settings', href: '/settings' },
		...(page.url.pathname.startsWith('/settings/admin/')
			? [{ label: 'Admin', href: '/settings#admin' }]
			: []),
		{ label: title }
	]);
</script>

<header class="page-head">
	<Breadcrumbs {items} />
	<h1>{title}</h1>
</header>
