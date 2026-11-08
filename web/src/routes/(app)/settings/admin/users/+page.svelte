<script lang="ts">
	import { onMount } from 'svelte';
	import { del, get, patch, post } from '#lib/api/client.ts';
	import type { Link } from '#lib/api/generated/Link.ts';
	import type { UserRow } from '#lib/api/generated/UserRow.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let users = $state<UserRow[]>([]);
	let error = $state('');
	let confirm = $state<{ kind: 'delete' | 'link'; user: UserRow } | null>(null);
	let link = $state<string | null>(null);

	async function load() {
		try {
			users = await get<UserRow[]>('/admin/users');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	const activeAdmins = $derived(users.filter((u) => u.is_admin && !u.suspended).length);
	const lastAdmin = (u: UserRow) => u.is_admin && !u.suspended && activeAdmins <= 1;

	async function update(u: UserRow, body: object) {
		error = '';
		try {
			await patch(`/admin/users/${u.id}`, body);
			load();
		} catch (e) {
			error = (e as Error).message;
		}
	}
	async function run() {
		if (!confirm) return;
		const { kind, user } = confirm;
		confirm = null;
		try {
			if (kind === 'delete') {
				await del(`/admin/users/${user.id}`);
				toasts.show(`Deleted @${user.username}`);
				load();
			} else {
				link = (await post<Link>(`/admin/users/${user.id}/reset-link`)).url;
			}
		} catch (e) {
			error = (e as Error).message;
		}
	}
	async function copy() {
		try {
			await navigator.clipboard.writeText(link ?? '');
			toasts.show('Link copied');
		} catch {
			// The link is on screen to select.
		}
	}
</script>

<PageHead title="Users" />
<div class="sec">
	{#if link}
		<div class="card">
			<div class="name">One-time password reset link (24 hours)</div>
			<div class="mono" style="overflow-wrap:anywhere;font-size:14px">{link}</div>
			<div class="acts">
				<button class="btn sm primary" type="button" onclick={copy}>Copy</button><button
					class="btn sm"
					type="button"
					onclick={() => (link = null)}>Done</button
				>
			</div>
		</div>
	{/if}
	<div class="cards">
		{#each users as u (u.id)}
			<div class="card">
				<div class="top">
					<span class="name">{u.display_name} <span class="meta">@{u.username}</span></span>
					<span
						>{#if u.is_admin}<span class="pill acc">Admin</span>{/if}
						{#if u.suspended}<span class="pill bad">Suspended</span>{/if}</span
					>
				</div>
				<div class="meta">{u.books} {u.books === 1 ? 'book' : 'books'}</div>
				<div class="acts">
					<button
						class="btn sm"
						type="button"
						disabled={lastAdmin(u)}
						onclick={() => update(u, { is_admin: !u.is_admin })}
						>{u.is_admin ? 'Remove admin' : 'Make admin'}</button
					>
					<button
						class="btn sm"
						type="button"
						disabled={lastAdmin(u) || u.id === auth.me?.id}
						onclick={() => update(u, { suspended: !u.suspended })}
						>{u.suspended ? 'Unsuspend' : 'Suspend'}</button
					>
					<button class="btn sm" type="button" onclick={() => (confirm = { kind: 'link', user: u })}
						>Reset link</button
					>
					<button
						class="btn sm danger-ghost"
						type="button"
						disabled={lastAdmin(u) || u.id === auth.me?.id}
						onclick={() => (confirm = { kind: 'delete', user: u })}>Delete</button
					>
				</div>
			</div>
		{/each}
	</div>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

{#if confirm}
	<div class="dialog-back" role="presentation">
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="c-h">
			{#if confirm.kind === 'delete'}
				<h3 id="c-h">Delete @{confirm.user.username}?</h3>
				<p>
					Deletes their account and all {confirm.user.books} books with their history. This can't be undone.
					Suspending keeps their data.
				</p>
			{:else}
				<h3 id="c-h">Password reset link for @{confirm.user.username}</h3>
				<p>
					For when they've forgotten their password and lost their recovery codes. The link works
					once, for 24 hours. It lets them set a new password and turns off their two-factor. Any
					earlier link stops working. They'll be told, and it's recorded in the audit log.
				</p>
			{/if}
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (confirm = null)}>Cancel</button>
				<button
					class="btn {confirm.kind === 'delete' ? 'danger' : 'primary'}"
					type="button"
					onclick={run}>{confirm.kind === 'delete' ? 'Delete' : 'Create link'}</button
				>
			</div>
		</div>
	</div>
{/if}
