<script lang="ts">
	// Control icons drawn on a 24-unit grid so they sit exactly centered in
	// round buttons (text glyphs like "+" don't: fonts place them off-center).
	let {
		name,
		size = 16,
		filled = true
	}: {
		name: 'plus' | 'pause' | 'play' | 'star' | 'check' | 'shelf';
		size?: number;
		filled?: boolean;
	} = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" class="icon">
	{#if name === 'plus'}
		<path
			d="M12 5v14M5 12h14"
			fill="none"
			stroke="currentColor"
			stroke-width="2.4"
			stroke-linecap="round"
		/>
	{:else if name === 'pause'}
		<rect x="6.5" y="5.5" width="4" height="13" rx="1.3" fill="currentColor" />
		<rect x="13.5" y="5.5" width="4" height="13" rx="1.3" fill="currentColor" />
	{:else if name === 'check'}
		<path
			d="M5.5 12.5l4.2 4.2 8.8-9.4"
			fill="none"
			stroke="currentColor"
			stroke-width="2.6"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	{:else if name === 'shelf'}
		<g fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round">
			<rect x="4" y="4.5" width="4.2" height="15" rx="1" />
			<rect x="10" y="4.5" width="4.2" height="15" rx="1" />
			<path d="M16 6.2l3-.8 2 13.4-3 .8z" />
		</g>
	{:else if name === 'star'}
		<path
			d="M12 3.6l2.6 5.3 5.8.85-4.2 4.1 1 5.8L12 16.9l-5.2 2.75 1-5.8-4.2-4.1 5.8-.85z"
			fill={filled ? 'currentColor' : 'none'}
			stroke="currentColor"
			stroke-width="1.6"
			stroke-linejoin="round"
		/>
	{:else}
		<!-- The triangle's centroid, not its box, sits at the center. -->
		<path
			d="M8.5 5.8v12.4a1 1 0 0 0 1.5.86l10-6.2a1 1 0 0 0 0-1.72l-10-6.2a1 1 0 0 0-1.5.86z"
			fill="currentColor"
		/>
	{/if}
</svg>

<style>
	.icon {
		display: block;
		flex: none;
	}
</style>
