<script lang="ts">
	type Health = { status: string; version: string };

	let health = $state<Health | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		fetch('/api/health')
			.then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
			.then((h: Health) => (health = h))
			.catch((e: Error) => (error = e.message));
	});
</script>

<main>
	<p class="brand">mau</p>
	<h1>Reading</h1>
	{#if health}
		<p>Server is running, version {health.version}.</p>
	{:else if error}
		<p class="error">Can't reach the server: {error}</p>
	{:else}
		<p>Checking the server…</p>
	{/if}
</main>

<style>
	main {
		max-width: 420px;
		margin: 0 auto;
		padding: 24px 18px;
	}
	.brand {
		margin: 0;
		color: var(--muted);
		font-size: 12px;
		letter-spacing: 0.14em;
		text-transform: uppercase;
	}
	h1 {
		margin: 0 0 12px;
		font-weight: 600;
		font-size: 23px;
	}
	.error {
		color: var(--danger);
	}
</style>
