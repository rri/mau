<script lang="ts">
	// The end of a partly drawn list (see paging.svelte.ts). It draws more on
	// its own while still well below the screen, so scrolling doesn't stop;
	// the button is there for keyboards and browsers that don't report it.
	import { PAGE_SIZE } from '../paging.svelte.ts';

	let { remaining, onmore }: { remaining: number; onmore: () => void } = $props();

	let el = $state<HTMLElement>();

	$effect(() => {
		// Watched again after each page: if the end is still near (a tall
		// screen), the observer reports it at once and the next page follows.
		void remaining;
		if (!el || typeof IntersectionObserver === 'undefined') return;
		const io = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting)) onmore();
			},
			{ rootMargin: '0px 0px 1500px 0px' }
		);
		io.observe(el);
		return () => io.disconnect();
	});
</script>

<div class="more" bind:this={el}>
	<button class="btn" type="button" onclick={onmore}
		>Show {Math.min(remaining, PAGE_SIZE)} more</button
	>
</div>

<style>
	.more {
		display: flex;
		justify-content: center;
		padding: 16px 20px 24px;
	}
</style>
