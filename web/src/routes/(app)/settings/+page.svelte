<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from '#lib/api/client.ts';
	import type { RecoveryStatus } from '#lib/api/generated/RecoveryStatus.ts';
	import type { Invites } from '#lib/api/generated/Invites.ts';
	import type { Share } from '#lib/api/generated/Share.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import { modal } from '#lib/modal.ts';
	import { signOut } from '#lib/signout.ts';

	let codesLeft = $state<number | null>(null);
	let shares = $state<number | null>(null);
	let canInvite = $state(false);
	let confirmSignOut = $state(false);

	onMount(() => {
		get<RecoveryStatus>('/recovery-codes')
			.then((r) => (codesLeft = r.left))
			.catch(() => {});
		get<Share[]>('/shares')
			.then((s) => (shares = s.length))
			.catch(() => {});
		if (!auth.me?.is_admin)
			get<Invites>('/invites')
				.then((i) => (canInvite = i.can_create || i.pending.length > 0))
				.catch(() => {});
	});

	function startSignOut() {
		if (library.pending > 0) confirmSignOut = true;
		else signOut();
	}

	type Item = { href: string; label: string; sub?: string; value?: string };
	const items = $derived<Item[]>([
		{
			href: '/settings/account',
			label: auth.me?.display_name ?? 'Account',
			sub: `@${auth.me?.username}`
		},
		{ href: '/settings/password', label: 'Password' },
		{
			href: '/settings/two-factor',
			label: 'Two-factor',
			value: auth.me?.two_factor ? 'On' : 'Off'
		},
		{ href: '/settings/sessions', label: 'Sessions' },
		{
			href: '/settings/recovery',
			label: 'Recovery codes',
			value: codesLeft === null ? '' : codesLeft === 0 ? 'None left' : `${codesLeft} of 10 left`
		},
		{
			href: '/settings/sharing',
			label: 'Sharing',
			value: shares === null ? '' : shares ? `${shares} shared` : 'Nothing shared'
		},
		{
			href: '/settings/google-books',
			label: 'Google Books',
			value: auth.me?.google_books ? 'Own key' : 'Off'
		},
		{ href: '/settings/import', label: 'Import & export' },
		...(canInvite ? [{ href: '/settings/invites', label: 'Invites' }] : [])
	]);
	const adminItems: Item[] = [
		{ href: '/settings/admin/users', label: 'Users' },
		{ href: '/settings/admin/signup', label: 'Signup & invites' },
		{ href: '/settings/admin/providers', label: 'Catalog providers' },
		{ href: '/settings/admin/queue', label: 'Review queue' },
		{ href: '/settings/admin/catalog', label: 'Shared catalog', sub: 'Fix records for everyone' },
		{ href: '/settings/admin/audit', label: 'Audit log' },
		{ href: '/settings/admin/stats', label: 'Instance stats' }
	];
</script>

<header class="page-head"><h1>Settings</h1></header>

<ul class="list">
	{#each items as it (it.href)}
		<li>
			<a class="item" href={it.href}
				><span
					>{it.label}{#if it.sub}<small>{it.sub}</small>{/if}</span
				><span class="val">{it.value ?? ''}</span><span class="chev">›</span></a
			>
		</li>
	{/each}
</ul>

{#if auth.me?.is_admin}
	<div class="group" id="admin">Admin</div>
	<ul class="list">
		{#each adminItems as it (it.href)}
			<li>
				<a class="item" href={it.href}
					><span
						>{it.label}{#if it.sub}<small>{it.sub}</small>{/if}</span
					><span class="val"></span><span class="chev">›</span></a
				>
			</li>
		{/each}
	</ul>
{/if}

<div class="sec"><button class="btn" type="button" onclick={startSignOut}>Sign out</button></div>

{#if confirmSignOut}
	<div class="dialog-back" role="presentation">
		<div
			class="dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="so-h"
			use:modal={() => (confirmSignOut = false)}
		>
			<h3 id="so-h">Sign out with unsynced changes?</h3>
			<p>
				{library.pending}
				{library.pending === 1 ? 'change' : 'changes'} on this device haven't reached the server yet.
				Signing out deletes them. Connect to the internet first to keep them.
			</p>
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (confirmSignOut = false)}>Cancel</button>
				<button class="btn danger" type="button" onclick={signOut}>Sign out anyway</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--line);
	}
	.item {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto auto;
		gap: 10px;
		align-items: center;
		padding: 13px 18px;
		border-bottom: 1px solid var(--line);
		color: var(--ink);
		text-decoration: none;
	}
	.item:hover {
		background: var(--surface);
	}
	small {
		display: block;
		color: var(--muted);
		font-size: 13px;
	}
	.val,
	.chev {
		color: var(--muted);
		font-size: 14px;
	}
	.group {
		padding: 20px 18px 6px;
		font-size: 13px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--muted);
	}
</style>
