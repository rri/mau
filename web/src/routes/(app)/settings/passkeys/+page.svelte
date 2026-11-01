<script lang="ts">
	import { onMount } from 'svelte';
	import { del, get, patch, post } from '#lib/api/client.ts';
	import type { CeremonyStart } from '#lib/api/generated/CeremonyStart.ts';
	import type { PasskeyInfo } from '#lib/api/generated/PasskeyInfo.ts';
	import { createPasskey, deviceName, withReauth } from '#lib/api/webauthn.ts';
	import { fmtDate, ago, dateOfMs } from '#lib/dates.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let keys = $state<PasskeyInfo[]>([]);
	let error = $state('');
	let renaming = $state<string | null>(null);
	let newName = $state('');
	let adding = $state(false);
	let addName = $state(deviceName());

	const tz = auth.me?.time_zone ?? 'UTC';
	const today = dateOfMs(Date.now(), tz);
	const day = (ms: number) => dateOfMs(ms, tz);

	async function load() {
		try {
			keys = await get<PasskeyInfo[]>('/passkeys');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	async function add() {
		error = '';
		try {
			await withReauth(() =>
				createPasskey(
					() => post<CeremonyStart>('/passkeys/start'),
					(challenge, credential) =>
						post('/passkeys/finish', { challenge, credential, name: addName })
				)
			);
			adding = false;
			toasts.show('Passkey added');
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}

	async function rename(id: string) {
		try {
			await patch(`/passkeys/${id}`, { name: newName });
			renaming = null;
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}

	async function remove(k: PasskeyInfo) {
		error = '';
		try {
			await withReauth(() => del(`/passkeys/${k.id}`));
			toasts.show(`Removed ${k.name}`);
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
</script>

<PageHead title="Passkeys" />

<div class="sec">
	<p class="help">
		Passkeys are how you sign in. Add one for each device, password manager, or security key you
		use.
	</p>
	<div class="cards">
		{#each keys as k (k.id)}
			<div class="card">
				{#if renaming === k.id}
					<form
						class="rename"
						onsubmit={(e) => {
							e.preventDefault();
							rename(k.id);
						}}
					>
						<input bind:value={newName} aria-label="Passkey name" />
						<button class="btn sm primary" type="submit">Save</button>
						<button class="btn sm" type="button" onclick={() => (renaming = null)}>Cancel</button>
					</form>
				{:else}
					<div class="top"><span class="name">{k.name}</span></div>
				{/if}
				<div class="meta">
					Added {fmtDate(day(k.created_at))} · Last used {k.last_used_at
						? ago(day(k.last_used_at), today)
						: 'never'}
				</div>
				<div class="acts">
					<button
						class="btn sm"
						type="button"
						onclick={() => {
							renaming = k.id;
							newName = k.name;
						}}>Rename</button
					>
					<button
						class="btn sm danger-ghost"
						type="button"
						disabled={keys.length === 1}
						onclick={() => remove(k)}>Remove</button
					>
				</div>
			</div>
		{/each}
	</div>
	{#if keys.length === 1}<p class="help">
			This is your only passkey, so it can't be removed. Add another first.
		</p>{/if}
	{#if adding}
		<div class="card">
			<label class="field">Name for the new passkey<input bind:value={addName} /></label>
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (adding = false)}>Cancel</button>
				<button class="btn primary" type="button" onclick={add}>Create passkey</button>
			</div>
		</div>
	{:else}
		<div class="row-end">
			<button class="btn primary" type="button" onclick={() => (adding = true)}
				>Add a passkey</button
			>
		</div>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
	.rename {
		display: flex;
		gap: 6px;
	}
	.rename input {
		flex: 1;
		min-width: 0;
		padding: 5px 8px;
		border: 1px solid var(--line);
		border-radius: 8px;
	}
</style>
