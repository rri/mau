<script lang="ts">
	import { onMount } from 'svelte';
	import { get, post } from '#lib/api/client.ts';
	import type { RecoveryCodes } from '#lib/api/generated/RecoveryCodes.ts';
	import type { RecoveryStatus } from '#lib/api/generated/RecoveryStatus.ts';
	import { withReauth } from '#lib/confirm.svelte.ts';
	import { dateOfMs, fmtDate } from '#lib/dates.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let status = $state<RecoveryStatus | null>(null);
	let codes = $state<string[] | null>(null);
	let saved = $state(false);
	let copied = $state(false);
	let error = $state('');
	const tz = auth.me?.time_zone ?? 'UTC';

	async function load() {
		try {
			status = await get<RecoveryStatus>('/recovery-codes');
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);

	async function regenerate() {
		error = '';
		try {
			const r = await withReauth(() => post<RecoveryCodes>('/recovery-codes/regenerate'));
			codes = r.codes;
			saved = false;
			copied = false;
		} catch (e) {
			error = (e as Error).message;
		}
	}
	async function copy() {
		try {
			await navigator.clipboard.writeText((codes ?? []).join('\n'));
			copied = true;
		} catch {
			error = 'Copying was blocked. Select the codes and copy them instead.';
		}
	}
	function done() {
		codes = null;
		load();
	}
</script>

<PageHead title="Recovery codes" />

{#if codes}
	<div class="sec">
		<h2>Your new recovery codes</h2>
		<p class="help">
			Save these somewhere safe now. They won't be shown again. Each works once. Your old codes no
			longer work.
		</p>
		<div class="codes">
			{#each codes as c (c)}<span>{c}</span>{/each}
		</div>
		<button class="btn" type="button" onclick={copy}>{copied ? 'Copied' : 'Copy all'}</button>
		<label class="check"><input type="checkbox" bind:checked={saved} /> I've saved them</label>
		<div class="row-end">
			<button class="btn primary" type="button" disabled={!saved} onclick={done}>Done</button>
		</div>
	</div>
{:else if status}
	<div class="sec">
		<p class="help">
			If you forget your password, a recovery code lets you set a new one. If you lose your
			authenticator app, a recovery code works in place of its code.
		</p>
		<div><b class="big">{status.left}</b> <span class="help">of 10 codes left</span></div>
		<div class="meter"><span style="width:{status.left * 10}%"></span></div>
		{#if status.left === 0}<p class="notice">
				No codes left. Generate a new set now: without one, a forgotten password needs an admin's
				help.
			</p>
		{:else if status.left <= 3}<p class="notice">
				Only {status.left} left. Generate a new set soon.
			</p>{/if}
		<p class="help">
			Codes are shown only when they're created. Generating a new set replaces all remaining codes.
		</p>
		<div class="row-end">
			<button class="btn primary" type="button" onclick={regenerate}>Generate new codes</button>
		</div>
	</div>
	<div class="sec">
		<h2>Recovery activity</h2>
		{#if status.events.length}
			<ul class="log">
				{#each status.events as e, i (i)}<li>
						<time>{fmtDate(dateOfMs(Number(e.at), tz))}</time><span>{e.message}</span>
					</li>{/each}
			</ul>
		{:else}
			<p class="help">None.</p>
		{/if}
	</div>
{/if}
{#if error}<p class="sec error" role="alert">{error}</p>{/if}

<style>
	.big {
		font-size: 26px;
	}
	.meter {
		height: 8px;
		border-radius: 4px;
		background: var(--line);
		overflow: hidden;
	}
	.meter span {
		display: block;
		height: 100%;
		background: var(--accent);
	}
	.log {
		list-style: none;
		margin: 0;
		padding: 0;
		font-size: 15px;
	}
	.log li {
		display: grid;
		grid-template-columns: 110px minmax(0, 1fr);
		gap: 10px;
		padding: 8px 0;
		border-bottom: 1px solid var(--line);
	}
	time {
		color: var(--muted);
	}
</style>
