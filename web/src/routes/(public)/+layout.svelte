<script lang="ts">
	let { children } = $props();
</script>

<main class="public">
	<div class="card">
		<a class="logo" href="/"><img src="/logo.svg" alt="mau" width="112" height="112" /></a>
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
		font-weight: 700;
		text-wrap: balance;
	}
	.logo {
		display: block;
		width: max-content;
		margin: -4px 0 4px;
		border-radius: 22.5%;
	}
	.logo img {
		display: block;
	}
	.card :global(.tagline) {
		margin: -8px 0 0;
		color: var(--muted);
		font-size: 16px;
		font-weight: 500;
		letter-spacing: 0;
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
