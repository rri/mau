<script lang="ts">
	let { children } = $props();
</script>

<main class="public">
	<div class="card">
		{@render children()}
	</div>
</main>

<style>
	.public {
		min-height: 100%;
		display: grid;
		place-items: center;
		padding: 24px 16px;
	}
	.card {
		width: 100%;
		max-width: 420px;
		display: grid;
		gap: 16px;
		padding: 28px 24px;
		background: var(--surface);
		border: 1px solid var(--line);
		border-radius: 18px;
	}
	.card :global(h1) {
		margin: 0;
		font-size: 24px;
		font-weight: 600;
		text-wrap: balance;
	}
	.card :global(.brand) {
		margin: 0;
		font-size: 40px;
		font-weight: 600;
		line-height: 1;
		letter-spacing: -0.01em;
	}
	.card :global(.tagline) {
		margin: -8px 0 0;
		color: var(--muted);
	}
	.card :global(p) {
		margin: 0;
	}
	.card :global(.links) {
		display: grid;
		gap: 8px;
		justify-items: center;
		font-size: 15px;
	}
</style>
