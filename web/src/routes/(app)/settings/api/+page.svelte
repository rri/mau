<script lang="ts">
	// Settings › Access Tokens: apps this reader connected ("Sign in with
	// mau"), and personal access tokens they generated for their own scripts.
	// Both are the same kind of access underneath; they differ in how the
	// token reaches the program.
	import { onMount } from 'svelte';
	import { del, get, post } from '#lib/api/client.ts';
	import type { ApiAccess } from '#lib/api/generated/ApiAccess.ts';
	import type { ConnectedApp } from '#lib/api/generated/ConnectedApp.ts';
	import type { CreatedApiToken } from '#lib/api/generated/CreatedApiToken.ts';
	import type { TokenInfo } from '#lib/api/generated/TokenInfo.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { withReauth } from '#lib/confirm.svelte.ts';
	import { ago, dateOfMs, fmtDate } from '#lib/dates.ts';
	import { library } from '#lib/library.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';
	import ScopeList from '#lib/components/ScopeList.svelte';

	const tz = auth.me?.time_zone ?? 'UTC';
	const today = dateOfMs(Date.now(), tz);

	let data = $state<ApiAccess | null>(null);
	let error = $state('');

	// Making a token
	let making = $state(false);
	let name = $state('');
	let expires = $state('90');
	let chosen = $state<string[]>([]);
	let formError = $state('');
	let busy = $state(false);
	let created = $state<CreatedApiToken | null>(null);
	let saved = $state(false);
	let copied = $state(false);

	async function load() {
		try {
			data = await get<ApiAccess>('/api-access');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	const infosOf = (scopes: string[]) => data?.scopes.filter((s) => scopes.includes(s.scope)) ?? [];
	const reads = $derived(data?.scopes.filter((s) => s.group === 'read') ?? []);
	const changes = $derived(data?.scopes.filter((s) => s.group !== 'read') ?? []);

	function lastUsed(at: number | null) {
		// After a "·", so lowercase (as "Active · page 3 of 9" elsewhere).
		return at === null ? 'never used' : `last used ${ago(dateOfMs(at, tz), today)}`;
	}
	function expiry(t: TokenInfo) {
		return t.expires_at === null
			? 'Never expires'
			: `Expires ${fmtDate(dateOfMs(t.expires_at, tz))}`;
	}

	async function disconnect(a: ConnectedApp) {
		error = '';
		try {
			await del(`/api-access/apps/${a.id}`);
			toasts.show(`Disconnected ${a.host}`);
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
	async function revoke(t: TokenInfo) {
		error = '';
		try {
			await del(`/api-access/tokens/${t.id}`);
			toasts.show(`Revoked “${t.name}”`);
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}

	function startMaking() {
		making = true;
		name = '';
		expires = '90';
		chosen = [];
		formError = '';
	}

	async function make(e: SubmitEvent) {
		e.preventDefault();
		formError = '';
		if (!name.trim()) return void (formError = 'Give the token a name, so you know what uses it.');
		if (!chosen.length) return void (formError = 'Choose at least one thing the token may do.');
		busy = true;
		try {
			created = await withReauth(() =>
				post<CreatedApiToken>('/api-access/tokens', {
					name: name.trim(),
					scopes: chosen,
					expires_in_days: expires === 'never' ? null : Number(expires)
				})
			);
			making = false;
			saved = false;
			copied = false;
		} catch (err) {
			formError = (err as Error).message;
		} finally {
			busy = false;
		}
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(created?.secret ?? '');
			copied = true;
		} catch {
			error = 'Copying was blocked. Select the token and copy it instead.';
		}
	}
	function done() {
		created = null;
		load();
	}
</script>

<PageHead title="Access Tokens" />

{#if !library.online}
	<p class="sec notice">Apps and tokens need a connection.</p>
{:else if created}
	<div class="sec">
		<h2>Your new token</h2>
		<p class="help">
			Copy it now and keep it somewhere safe: it won't be shown again. Anyone who has it can do what
			it allows, as you.
		</p>
		<div class="secret mono" aria-label="The token">{created.secret}</div>
		<button class="btn" type="button" onclick={copy}>{copied ? 'Copied' : 'Copy token'}</button>
		<label class="check"><input type="checkbox" bind:checked={saved} /> I've saved it</label>
		<div class="row-end">
			<button class="btn primary" type="button" disabled={!saved} onclick={done}>Done</button>
		</div>
	</div>
{:else if data}
	<div class="sec">
		<p class="help">
			Third-party apps can read and change your library through mau's API. An app you sign in to
			with mau appears under Connected apps. For your own scripts, generate a personal access token.
		</p>
	</div>

	<div class="sec">
		<h2>Connected apps</h2>
		{#if data.apps.length}
			<div class="cards">
				{#each data.apps as a (a.id)}
					<div class="card">
						<div class="top">
							<span class="name host">{a.host}</span>{#if a.name}<span class="help">{a.name}</span
								>{/if}
						</div>
						<ScopeList scopes={a.scopes} markPrivate />
						<div class="meta">
							Connected {fmtDate(dateOfMs(a.created_at, tz))} · {lastUsed(a.last_used_at)}
						</div>
						<div class="acts">
							<button class="btn sm danger-ghost" type="button" onclick={() => disconnect(a)}
								>Disconnect</button
							>
						</div>
					</div>
				{/each}
			</div>
		{:else}
			<p class="help">
				No apps connected. When an app asks you to sign in with mau and you allow it, it appears
				here.
			</p>
		{/if}
	</div>

	<div class="sec">
		<h2>Personal Access Tokens</h2>
		{#if data.tokens.length}
			<div class="cards">
				{#each data.tokens as t (t.id)}
					<div class="card">
						<div class="top">
							<span class="name">{t.name}</span><span class="mono hint">{t.hint}…</span>
						</div>
						<ScopeList scopes={infosOf(t.scopes)} markPrivate />
						<div class="meta">{expiry(t)} · {lastUsed(t.last_used_at)}</div>
						<div class="acts">
							<button class="btn sm danger-ghost" type="button" onclick={() => revoke(t)}
								>Revoke</button
							>
						</div>
					</div>
				{/each}
			</div>
		{:else if !making}
			<p class="help">No tokens yet.</p>
		{/if}

		{#if making}
			<form class="make" onsubmit={make}>
				<label class="field" for="tk-name"
					>Name<input
						id="tk-name"
						bind:value={name}
						maxlength="60"
						placeholder="e.g. Reading log backup"
						autocomplete="off"
					/></label
				>
				<label class="field" for="tk-exp"
					>Expires<select id="tk-exp" bind:value={expires}>
						<option value="30">In 30 days</option>
						<option value="90">In 90 days</option>
						<option value="365">In a year</option>
						<option value="never">Never</option>
					</select></label
				>
				{#if expires === 'never'}<p class="help">
						This token works until you revoke it. Revoke it here when you stop using it.
					</p>{/if}
				<fieldset>
					<legend>READ</legend>
					{#each reads as s (s.scope)}
						<label class="check"
							><input type="checkbox" value={s.scope} bind:group={chosen} />
							{s.label}</label
						>
					{/each}
				</fieldset>
				<fieldset>
					<legend>READ &amp; MODIFY</legend>
					{#each changes as s (s.scope)}
						<label class="check"
							><input type="checkbox" value={s.scope} bind:group={chosen} />
							{s.label}</label
						>
					{/each}
				</fieldset>
				{#if formError}<p class="error" role="alert">{formError}</p>{/if}
				<div class="row-end">
					<button class="btn" type="button" onclick={() => (making = false)}>Cancel</button>
					<button class="btn primary" type="submit" disabled={busy}>Generate token</button>
				</div>
			</form>
		{:else}
			<div class="row-end">
				<button class="btn primary" type="button" onclick={startMaking}>Generate a token</button>
			</div>
		{/if}
	</div>
{/if}
{#if error}<p class="sec error" role="alert">{error}</p>{/if}

<style>
	.host {
		overflow-wrap: anywhere;
	}
	.hint {
		font-size: 14px;
		color: var(--muted);
	}
	.secret {
		padding: 12px;
		border: 1px dashed var(--line);
		border-radius: 12px;
		overflow-wrap: anywhere;
		font-size: 14px;
		user-select: all;
	}
	.make {
		display: grid;
		gap: 12px;
	}
	fieldset {
		margin: 0;
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
		display: grid;
		gap: 8px;
	}
	/* The same heading as each token's scopes (ScopeList). */
	legend {
		padding: 0 4px;
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	.check {
		align-items: flex-start;
		font-size: 15px;
	}
	.check input {
		margin-top: 2px;
		flex: none;
	}
</style>
