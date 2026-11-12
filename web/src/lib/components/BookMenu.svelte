<script lang="ts">
	// The ⋯ beside a book's reading controls: the same choices wherever a
	// book has them (Home, Reading rows and tiles, the book page). Active:
	// Finish, Pause. Inactive: Start or Resume, Finish. Finished: Re-read.
	import type { BookInfo } from '../derive.ts';
	import { finish, pause, resume, startLike, startReading } from '../actions.ts';
	import Icon from './Icon.svelte';

	let {
		info,
		onsetup,
		openLink = true,
		size = 'md'
	}: {
		info: BookInfo;
		/** Starting a never-read book asks for its pages first. */
		onsetup: () => void;
		/** "Open book" (everywhere but the book page itself). */
		openLink?: boolean;
		size?: 'sm' | 'md';
	} = $props();

	let open = $state(false);
	let up = $state(false);
	let button = $state<HTMLButtonElement>();
	let menu = $state<HTMLDivElement>();

	type Item = { label: string; run: () => void } | { label: string; href: string };
	const items = $derived.by((): Item[] => {
		const r = info.reading;
		const list: Item[] = [];
		if (info.state === 'active') {
			list.push({ label: 'Finish', run: () => finish(info) });
			list.push({ label: 'Pause', run: () => pause(info) });
		} else if (info.state === 'inactive') {
			list.push(
				r ? { label: 'Resume', run: () => resume(info) } : { label: 'Start', run: () => onsetup() }
			);
			list.push({ label: 'Finish', run: () => finish(info) });
		} else {
			list.push({ label: 'Re-read', run: () => startReading(info, startLike(info)) });
		}
		if (openLink) list.push({ label: 'Open book', href: `/catalog/${info.book.id}` });
		return list;
	});

	function toggle(e: MouseEvent) {
		e.stopPropagation();
		if (open) return close();
		// Near the bottom of the screen (above the tab bar), open upwards.
		const rect = button?.getBoundingClientRect();
		up = !!rect && rect.bottom > window.innerHeight - 200;
		open = true;
		requestAnimationFrame(() => menu?.querySelector<HTMLElement>('[role=menuitem]')?.focus());
	}

	function close(refocus = false) {
		open = false;
		if (refocus) button?.focus();
	}

	function keys(e: KeyboardEvent) {
		const all = [...(menu?.querySelectorAll<HTMLElement>('[role=menuitem]') ?? [])];
		const i = all.indexOf(document.activeElement as HTMLElement);
		if (e.key === 'Escape') {
			e.preventDefault();
			close(true);
		} else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			const n = all.length;
			all[(i + (e.key === 'ArrowDown' ? 1 : n - 1)) % n]?.focus();
		} else if (e.key === 'Tab') close();
	}
</script>

<svelte:window onclick={() => open && close()} />

<div class="wrap">
	<button
		bind:this={button}
		class="more {size}"
		type="button"
		aria-label="More for {info.title}"
		title="More"
		aria-haspopup="menu"
		aria-expanded={open}
		onclick={toggle}><Icon name="more" size={size === 'sm' ? 16 : 18} /></button
	>
	{#if open}
		<!-- svelte-ignore a11y_interactive_supports_focus -->
		<div
			class="menu"
			class:up
			role="menu"
			tabindex="-1"
			aria-label="{info.title}: reading"
			bind:this={menu}
			onkeydown={keys}
		>
			{#each items as it (it.label)}
				{#if 'href' in it}
					<a role="menuitem" href={it.href} onclick={() => close()}>{it.label}</a>
				{:else}
					<button
						type="button"
						role="menuitem"
						onclick={(e) => {
							e.stopPropagation();
							close(true);
							it.run();
						}}>{it.label}</button
					>
				{/if}
			{/each}
		</div>
	{/if}
</div>

<style>
	.wrap {
		position: relative;
	}
	.more {
		display: grid;
		place-items: center;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: none;
		color: var(--muted);
		cursor: pointer;
	}
	.more.md {
		width: 34px;
		height: 34px;
	}
	.more.sm {
		width: 28px;
		height: 28px;
	}
	.more:hover,
	.more[aria-expanded='true'] {
		background: var(--accent-soft);
		color: var(--accent);
	}
	.menu {
		position: absolute;
		right: 0;
		top: calc(100% + 4px);
		z-index: 30;
		display: grid;
		min-width: 170px;
		padding: 4px;
		background: var(--surface);
		border: 1px solid var(--line);
		border-radius: 12px;
		box-shadow: 0 8px 28px rgb(0 0 0 / 0.16);
	}
	.menu.up {
		top: auto;
		bottom: calc(100% + 4px);
	}
	.menu button,
	.menu a {
		text-align: left;
		background: none;
		border: 0;
		padding: 10px 12px;
		border-radius: 8px;
		color: var(--ink);
		font: inherit;
		font-size: 15px;
		text-decoration: none;
		cursor: pointer;
	}
	.menu button:hover,
	.menu a:hover,
	.menu button:focus-visible,
	.menu a:focus-visible {
		background: var(--bg);
	}
</style>
