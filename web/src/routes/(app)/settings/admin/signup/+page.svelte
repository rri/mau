<script lang="ts">
	import { onMount } from 'svelte';
	import { get, put } from '#lib/api/client.ts';
	import type { SignupSettings } from '#lib/api/generated/SignupSettings.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import InviteList from '#lib/components/InviteList.svelte';
	import PageHead from '#lib/components/PageHead.svelte';

	let s = $state<SignupSettings | null>(null);
	let error = $state('');
	onMount(async () => {
		try {
			s = await get<SignupSettings>('/admin/signup');
		} catch (e) {
			error = (e as Error).message;
		}
	});
	async function save() {
		if (!s) return;
		try {
			s = await put<SignupSettings>('/admin/signup', s);
			toasts.show('Saved');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	const modes = [
		{ v: 'open', l: 'Open', d: 'Anyone who can reach this site can create an account.' },
		{ v: 'invite', l: 'Invite only', d: 'People need an invite link.' },
		{ v: 'closed', l: 'Closed', d: 'No new accounts, even with an invite.' }
	] as const;
</script>

<PageHead title="Signup & invites" />
{#if s}
	<div class="sec">
		<h2>Who can sign up</h2>
		<div class="radio-list">
			{#each modes as m (m.v)}
				<label class="radio"
					><input type="radio" name="mode" value={m.v} bind:group={s.mode} onchange={save} /><span
						>{m.l}<small>{m.d}</small></span
					></label
				>
			{/each}
		</div>
	</div>
	<div class="sec">
		<h2>Who can invite</h2>
		<div class="radio-list">
			<label class="radio"
				><input
					type="radio"
					name="who"
					value="admins"
					bind:group={s.invite_who}
					onchange={save}
				/><span>Admins only</span></label
			>
			<label class="radio"
				><input
					type="radio"
					name="who"
					value="all"
					bind:group={s.invite_who}
					onchange={save}
				/><span
					>Every user<small
						>Each can have up to <input
							class="limit"
							type="number"
							min="1"
							max="100"
							bind:value={s.invite_limit}
							onchange={save}
							aria-label="Invite limit per user"
						/> pending invites.</small
					></span
				></label
			>
		</div>
	</div>
{/if}
<div class="sec">
	<h2>Invites</h2>
	<InviteList />
</div>
{#if error}<p class="sec error">{error}</p>{/if}

<style>
	.limit {
		width: 56px;
		padding: 1px 6px;
		border: 1px solid var(--line);
		border-radius: 6px;
		background: var(--surface);
	}
</style>
