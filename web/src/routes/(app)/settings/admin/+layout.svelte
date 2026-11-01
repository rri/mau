<script lang="ts">
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	let { children } = $props();
</script>

{#if !auth.me?.is_admin}
	<p class="sec">Only admins can see this.</p>
{:else if !library.online}
	<p class="sec notice">Admin pages need a connection.</p>
{:else}
	{@render children()}
{/if}
