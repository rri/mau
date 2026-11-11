<script lang="ts">
	import { onMount } from 'svelte';
	import { get, put } from '#lib/api/client.ts';
	import type { ProviderInfo } from '#lib/api/generated/ProviderInfo.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import PageHead from '#lib/components/PageHead.svelte';

	let list = $state<(ProviderInfo & { key: string })[]>([]);
	let error = $state('');

	onMount(async () => {
		try {
			list = (await get<ProviderInfo[]>('/admin/providers')).map((p) => ({ ...p, key: '' }));
		} catch (e) {
			error = (e as Error).message;
		}
	});

	function move(i: number, d: number) {
		const j = i + d;
		[list[i], list[j]] = [list[j], list[i]];
	}

	async function save() {
		error = '';
		try {
			const res = await put<ProviderInfo[]>(
				'/admin/providers',
				list.map((p) => ({
					name: p.name,
					enabled: p.enabled,
					key: p.key.trim() ? p.key.trim() : null
				}))
			);
			list = res.map((p) => ({ ...p, key: '' }));
			toasts.show('Providers saved');
		} catch (e) {
			error = (e as Error).message;
		}
	}
</script>

<PageHead title="Catalog providers" />
<div class="sec">
	<p class="help">
		mau asks enabled providers in this order, moving to the next only when results are missing or
		thin. Keys are stored on the server and never shown again.
	</p>
	<div class="cards">
		{#each list as p, i (p.name)}
			<div class="prov card">
				<div class="ord">
					<button
						type="button"
						aria-label="Move {p.label} up"
						disabled={i === 0}
						onclick={() => move(i, -1)}><Icon name="up" size={14} /></button
					>
					<button
						type="button"
						aria-label="Move {p.label} down"
						disabled={i === list.length - 1}
						onclick={() => move(i, 1)}><Icon name="down" size={14} /></button
					>
				</div>
				<div>
					<b>{p.label}</b>
					{#if p.needs_key}
						<input
							type="password"
							bind:value={p.key}
							placeholder={p.has_key ? 'Key saved (type to replace)' : 'API key'}
							aria-label="API key for {p.label}"
							autocomplete="off"
						/>
					{:else}
						<div class="meta">No API key needed</div>
					{/if}
				</div>
				<label class="check"
					><input
						type="checkbox"
						bind:checked={p.enabled}
						disabled={p.needs_key && !p.has_key && !p.key.trim()}
					/> On</label
				>
			</div>
		{/each}
	</div>
	<p class="help">A provider that needs a key can be turned on once its key is entered.</p>
	<div class="row-end"><button class="btn primary" type="button" onclick={save}>Save</button></div>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
	.prov {
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: 10px;
	}
	.ord {
		display: grid;
		gap: 2px;
	}
	.ord button {
		background: none;
		border: 1px solid var(--line);
		border-radius: 6px;
		width: 28px;
		height: 22px;
		padding: 0;
		display: grid;
		place-items: center;
		color: var(--muted);
		cursor: pointer;
	}
	.ord button:disabled {
		opacity: 0.35;
		cursor: default;
	}
	input[type='password'] {
		width: 100%;
		margin-top: 6px;
		padding: 6px 8px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--surface);
	}
</style>
