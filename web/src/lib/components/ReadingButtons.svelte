<script lang="ts">
	// A book's reading buttons, the same wherever they appear (Home, Reading
	// list rows, the book page): all one size, right-aligned so they line up
	// from row to row.
	//   Active:   +1 (a page, or 1%), finish, pause
	//   Inactive: finish, start (or resume)
	// Finished books have Re-read on their book page instead.
	import type { BookInfo } from '../derive.ts';
	import { finish, pause, plusOne, resume } from '../actions.ts';
	import Icon from './Icon.svelte';

	let {
		info,
		onsetup
	}: {
		info: BookInfo;
		/** Starting a never-read book asks how to count it first. */
		onsetup: () => void;
	} = $props();

	const r = $derived(info.reading);
	const percent = $derived(r?.unit === 'percent');
	const icon = 18;
</script>

{#if info.state !== 'finished'}
	<div class="buttons">
		{#if info.state === 'active'}
			<button
				class="step"
				type="button"
				onclick={() => plusOne(info)}
				aria-label="Add {percent ? '1%' : '1 page'} to {info.title}"
				title={percent ? '+1 percent' : '+1 page'}><Icon name="plusone" size={20} /></button
			>
		{/if}
		<button
			type="button"
			onclick={() => finish(info)}
			aria-label="Finish {info.title}"
			title="Finish"><Icon name="check" size={icon} /></button
		>
		{#if info.state === 'active'}
			<button
				type="button"
				onclick={() => pause(info)}
				aria-label="Pause {info.title}"
				title="Pause"><Icon name="pause" size={icon} /></button
			>
		{:else}
			<button
				type="button"
				onclick={() => (r ? resume(info) : onsetup())}
				aria-label="{r ? 'Resume' : 'Start'} {info.title}"
				title={r ? 'Resume' : 'Start'}><Icon name="play" size={icon} /></button
			>
		{/if}
	</div>
{/if}

<style>
	.buttons {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 6px;
	}
	button {
		display: grid;
		place-items: center;
		width: 40px;
		height: 40px;
		padding: 0;
		border: 1px solid var(--line);
		border-radius: 50%;
		background: var(--surface);
		color: var(--muted);
		cursor: pointer;
	}
	button:hover {
		color: var(--accent);
		border-color: var(--accent);
	}
	button:active {
		transform: scale(0.94);
	}
	/* +1 is the button used most: filled. */
	.step {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: 0 2px 6px color-mix(in srgb, var(--accent) 35%, transparent);
	}
	.step:hover {
		color: var(--on-accent);
	}
</style>
