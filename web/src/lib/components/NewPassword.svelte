<script lang="ts">
	// A new password, typed twice. `valid` is true once both match and are long
	// enough; the server also refuses very common passwords.
	let {
		password = $bindable(''),
		valid = $bindable(false),
		label = 'Password',
		id = 'np'
	}: { password?: string; valid?: boolean; label?: string; id?: string } = $props();

	const MIN = 10;
	let again = $state('');
	let show = $state(false);
	const length = $derived([...password].length);
	const mismatch = $derived(again.length > 0 && again !== password);

	$effect(() => {
		valid = length >= MIN && length <= 256 && again === password;
	});
</script>

<label class="field" for="{id}-1"
	>{label}<input
		id="{id}-1"
		type={show ? 'text' : 'password'}
		autocomplete="new-password"
		bind:value={password}
		required
	/></label
>
<label class="field" for="{id}-2"
	>Type it again<input
		id="{id}-2"
		type={show ? 'text' : 'password'}
		autocomplete="new-password"
		bind:value={again}
		required
	/></label
>
<p class="help" aria-live="polite">
	{#if mismatch}
		<span class="error">The two don't match.</span>
	{:else if length > 0 && length < MIN}
		{MIN - length} more {MIN - length === 1 ? 'character' : 'characters'} needed.
	{:else}
		At least {MIN} characters. A few unrelated words make a good one.
	{/if}
</p>
<label class="check" for="{id}-show"
	><input type="checkbox" id="{id}-show" bind:checked={show} /> Show password</label
>

<style>
	.check {
		display: flex;
		gap: 8px;
		align-items: center;
		cursor: pointer;
		font-size: 15px;
	}
	.check input {
		accent-color: var(--accent);
		width: 17px;
		height: 17px;
		margin: 0;
	}
</style>
