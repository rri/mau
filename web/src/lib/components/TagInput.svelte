<script lang="ts">
	// Adds tags with autocomplete of existing ones. Tags match exactly as typed;
	// suggestions find existing tags regardless of case.
	let {
		existing,
		current,
		onadd
	}: { existing: string[]; current: string[]; onadd: (tag: string) => void } = $props();

	let value = $state('');
	let index = $state(0);
	let open = $state(false);

	type Item = { tag?: string; label: string; hint?: boolean };
	const items = $derived.by((): Item[] => {
		const v = value.trim().replace(/^#/, '');
		if (!v) return [];
		const lv = v.toLocaleLowerCase();
		const out: Item[] = existing
			.filter((t) => !current.includes(t) && t.toLocaleLowerCase().includes(lv))
			.sort(
				(a, b) =>
					Number(b.toLocaleLowerCase().startsWith(lv)) -
						Number(a.toLocaleLowerCase().startsWith(lv)) || a.localeCompare(b)
			)
			.slice(0, 8)
			.map((t) => ({ tag: t, label: `#${t}` }));
		if (current.includes(v)) out.push({ label: `Already tagged #${v}`, hint: true });
		else if (!existing.includes(v)) out.push({ tag: v, label: `Create new tag #${v}` });
		return out;
	});

	function pick(item: Item | undefined) {
		if (!item?.tag) return;
		onadd(item.tag);
		value = '';
		index = 0;
	}

	function key(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			index = Math.max(0, Math.min(items.length - 1, index + (e.key === 'ArrowDown' ? 1 : -1)));
		} else if (e.key === 'Enter') {
			e.preventDefault();
			pick(items[index]);
		} else if (e.key === 'Escape') {
			value = '';
		}
	}
</script>

<span class="box">
	<input
		bind:value
		oninput={() => {
			index = 0;
			open = true;
		}}
		onkeydown={key}
		onblur={() => (open = false)}
		onfocus={() => (open = true)}
		placeholder="+ add tag"
		aria-label="Add tag"
		autocomplete="off"
		role="combobox"
		aria-expanded={open && items.length > 0}
		aria-controls="tag-suggest"
	/>
	{#if open && items.length}
		<ul id="tag-suggest" role="listbox">
			{#each items as it, i (it.label)}
				<li
					role="option"
					aria-selected={i === index}
					class:hint={it.hint}
					onpointerdown={(e) => {
						e.preventDefault();
						pick(it);
					}}
				>
					{it.label}
				</li>
			{/each}
		</ul>
	{/if}
</span>

<style>
	.box {
		position: relative;
	}
	input {
		width: 130px;
		padding: 3px 10px;
		border: 1px dashed var(--line);
		border-radius: 999px;
		background: transparent;
		font-size: 14px;
	}
	ul {
		position: absolute;
		left: 0;
		top: calc(100% + 4px);
		z-index: 15;
		list-style: none;
		margin: 0;
		padding: 4px;
		min-width: 220px;
		background: var(--surface);
		border: 1px solid var(--line);
		border-radius: 10px;
		box-shadow: 0 8px 24px rgb(0 0 0 / 0.14);
	}
	li {
		padding: 6px 10px;
		border-radius: 7px;
		cursor: pointer;
		font-size: 14px;
	}
	li[aria-selected='true'] {
		background: var(--accent-soft);
		color: var(--accent);
	}
	li.hint {
		color: var(--muted);
		cursor: default;
	}
</style>
