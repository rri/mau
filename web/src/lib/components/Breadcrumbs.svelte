<script lang="ts" module>
	export type Crumb = { label: string; href?: string; onclick?: (e: MouseEvent) => void };
</script>

<script lang="ts">
	// A breadcrumb trail: earlier steps are links, the last is the current page.
	// On narrow screens it shortens to a single "‹ parent" link.

	let { items }: { items: Crumb[] } = $props();
	const parent = $derived(items.length > 1 ? items[items.length - 2] : null);
</script>

<nav class="crumbs" aria-label="Breadcrumb">
	<ol>
		{#each items as c, i (i)}
			<li>
				{#if i > 0}
					<svg class="sep" width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"
						><path
							d="M9 6l6 6-6 6"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
						/></svg
					>
				{/if}
				{#if i === items.length - 1}
					<span class="current" aria-current="page">{c.label}</span>
				{:else if c.href}
					<a href={c.href} onclick={c.onclick}>{c.label}</a>
				{:else if c.onclick}
					<button type="button" onclick={c.onclick}>{c.label}</button>
				{:else}
					<span>{c.label}</span>
				{/if}
			</li>
		{/each}
	</ol>
	{#if parent}
		<!-- Phones: just the way back. -->
		<span class="short">
			<svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"
				><path
					d="M15 6l-6 6 6 6"
					fill="none"
					stroke="currentColor"
					stroke-width="2.2"
					stroke-linecap="round"
					stroke-linejoin="round"
				/></svg
			>
			{#if parent.href}
				<a href={parent.href} onclick={parent.onclick}>{parent.label}</a>
			{:else}
				<button type="button" onclick={parent.onclick}>{parent.label}</button>
			{/if}
		</span>
	{/if}
</nav>

<style>
	.crumbs {
		font-size: 14px;
		line-height: 1.4;
		min-width: 0;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 2px;
		min-width: 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 2px;
		min-width: 0;
	}
	.sep {
		flex: none;
		color: var(--muted);
		opacity: 0.7;
	}
	a,
	button {
		padding: 4px 8px;
		margin: 0 -2px;
		border: 0;
		border-radius: 8px;
		background: none;
		color: var(--accent);
		font: inherit;
		font-weight: 550;
		text-decoration: none;
		cursor: pointer;
		white-space: nowrap;
	}
	a:hover,
	button:hover {
		background: var(--accent-soft);
	}
	li:first-child a,
	li:first-child button {
		margin-left: -8px;
	}
	.current {
		padding: 4px 6px;
		color: var(--ink);
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 28ch;
	}
	.short {
		display: none;
		align-items: center;
		color: var(--accent);
	}
	.short a,
	.short button {
		margin-left: -4px;
	}
	@media (max-width: 599px) {
		ol {
			display: none;
		}
		.short {
			display: inline-flex;
		}
	}
</style>
