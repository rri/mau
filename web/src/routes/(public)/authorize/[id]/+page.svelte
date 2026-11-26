<script lang="ts">
	// "Sign in with mau": an app sent the reader here (through
	// /oauth/authorize). Signed out, sign in first and come back. Then: which
	// account, and whether to allow exactly what the app asked for. The app
	// is always named by its address, which is where the access goes.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { get, post } from '#lib/api/client.ts';
	import type { AuthAnswer } from '#lib/api/generated/AuthAnswer.ts';
	import type { AuthRequestInfo } from '#lib/api/generated/AuthRequestInfo.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { withReauth } from '#lib/confirm.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import ScopeList from '#lib/components/ScopeList.svelte';

	const id = $derived(page.params.id ?? '');
	const here = $derived(`/authorize/${encodeURIComponent(id)}`);

	let info = $state<AuthRequestInfo | null>(null);
	let step = $state<'account' | 'consent'>('account');
	let error = $state('');
	let gone = $state(false);
	let busy = $state(false);

	onMount(async () => {
		if (!auth.me) {
			goto(`/signin?next=${encodeURIComponent(here)}`, { replaceState: true });
			return;
		}
		try {
			info = await get<AuthRequestInfo>(`/oauth/requests/${encodeURIComponent(id)}`);
		} catch (e) {
			if ((e as { status?: number }).status === 404) gone = true;
			error = (e as Error).message;
		}
	});

	/** Signs out here (keeping the request) and signs in as someone else. */
	async function otherAccount() {
		try {
			await post('/auth/logout');
		} catch {
			// Offline: the request can't be answered anyway.
		}
		await library.wipe();
		auth.clear();
		goto(`/signin?next=${encodeURIComponent(here)}`, { replaceState: true });
	}

	async function answer(kind: 'approve' | 'deny') {
		busy = true;
		error = '';
		try {
			const path = `/oauth/requests/${encodeURIComponent(id)}/${kind}`;
			const r =
				kind === 'approve'
					? await withReauth(() => post<AuthAnswer>(path))
					: await post<AuthAnswer>(path);
			// Back to the app (a full navigation: it's another site).
			location.assign(r.redirect);
		} catch (e) {
			// The request expired (10 minutes) or the server restarted.
			if ((e as { status?: number }).status === 404) gone = true;
			else error = (e as Error).message;
			busy = false;
		}
	}

	/** Notes are never shared anywhere else in mau: say so. */
	const asksForNotes = $derived(info?.scopes.some((s) => s.scope.startsWith('notes:')) ?? false);
</script>

{#if gone}
	<h1>This request has expired</h1>
	<p class="help">
		It was already answered, or it's more than 10 minutes old. Go back to the app and start again.
	</p>
	<div class="links"><a class="link" href="/">Go to Home</a></div>
{:else if info}
	{#if step === 'account'}
		<h1>Connect an app</h1>
		<p class="lead"><b class="host">{info.host}</b> wants to use your mau account.</p>
		<div class="who">
			<span class="label">Signed in as</span>
			<b>{info.me.display_name}</b>
			<span class="help">@{info.me.username}</span>
		</div>
		<button class="btn primary big" type="button" onclick={() => (step = 'consent')}
			>Continue as {info.me.display_name}</button
		>
		<div class="links">
			<button class="link" type="button" onclick={otherAccount}>Use another account</button>
		</div>
	{:else}
		<h1>Allow {info.host}?</h1>
		<p class="lead">
			<b class="host">{info.host}</b>{#if info.name}<span class="help">
					· calls itself “{info.name}”</span
				>{/if} wants access to:
		</p>
		<div class="asked"><ScopeList scopes={info.scopes} markPrivate /></div>
		{#if asksForNotes}
			<p class="help">Your notes are private: mau never shares them anywhere else.</p>
		{/if}
		{#if info.local}
			<p class="notice">
				This app is running on this computer ({info.host}), not on a website.
			</p>
		{/if}
		<p class="help">
			Once you allow access, you'll be redirected to <b>{info.redirect_to}</b>. You can disconnect
			it at any time in Settings › Access Tokens. Only allow apps you trust.
		</p>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="choices">
			<button class="btn" type="button" disabled={busy} onclick={() => answer('deny')}>Deny</button>
			<button class="btn primary" type="button" disabled={busy} onclick={() => answer('approve')}
				>Allow</button
			>
		</div>
		<div class="links">
			<button class="link" type="button" onclick={() => (step = 'account')}>Back</button>
		</div>
	{/if}
{:else if error}
	<h1>Connect an app</h1>
	<p class="error" role="alert">{error}</p>
{:else}
	<h1>Connect an app</h1>
	<p class="help" role="status">Checking the request…</p>
{/if}

<ConfirmDialog />

<style>
	.lead {
		text-align: center;
		text-wrap: balance;
	}
	/* Names and hosts in the app's semibold, like card names elsewhere. */
	.host,
	.who b,
	.help :global(b) {
		font-weight: 600;
	}
	.host {
		overflow-wrap: anywhere;
	}
	.who {
		display: grid;
		justify-items: center;
		gap: 2px;
		padding: 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
		background: var(--bg);
	}
	.who .label {
		font-size: 13px;
		color: var(--muted);
	}
	.asked {
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
	}
	.choices {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px;
	}
</style>
