<script lang="ts">
	let {
		title,
		back = '/settings',
		backLabel = 'Settings'
	}: { title: string; back?: string | null; backLabel?: string } = $props();
</script>

<header class="page-head">
	{#if back}<a class="back" href={back}>‹ {backLabel}</a>{/if}
	<h1>{title}</h1>
</header>
