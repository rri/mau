<script lang="ts">
	// The list / grid switch in the Reading and Catalog headers.
	let { layout = $bindable() }: { layout: 'list' | 'grid' } = $props();
</script>

<button
	class="icon"
	type="button"
	aria-label="List layout"
	aria-pressed={layout === 'list'}
	onclick={() => (layout = 'list')}
	><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"
		><g fill="currentColor"
			><rect x="1" y="2" width="14" height="2" rx="1" /><rect
				x="1"
				y="7"
				width="14"
				height="2"
				rx="1"
			/><rect x="1" y="12" width="14" height="2" rx="1" /></g
		></svg
	></button
>
<button
	class="icon"
	type="button"
	aria-label="Grid layout"
	aria-pressed={layout === 'grid'}
	onclick={() => (layout = 'grid')}
	><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"
		><g fill="currentColor"
			><rect x="1" y="1" width="6" height="6" rx="1.5" /><rect
				x="9"
				y="1"
				width="6"
				height="6"
				rx="1.5"
			/><rect x="1" y="9" width="6" height="6" rx="1.5" /><rect
				x="9"
				y="9"
				width="6"
				height="6"
				rx="1.5"
			/></g
		></svg
	></button
>

<style>
	.icon {
		width: 38px;
		height: 38px;
		border-radius: 10px;
		border: 1px solid var(--line);
		background: var(--surface);
		display: grid;
		place-items: center;
		cursor: pointer;
		color: var(--ink);
	}
	.icon[aria-pressed='true'] {
		background: var(--accent-soft);
		color: var(--accent);
		border-color: transparent;
	}
</style>
