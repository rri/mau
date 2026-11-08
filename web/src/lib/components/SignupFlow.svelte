<script lang="ts">
	// The three signup steps: choose a name, choose a password, save recovery codes.
	import { goto } from '$app/navigation';
	import { post } from '#lib/api/client.ts';
	import type { RecoveryCodes } from '#lib/api/generated/RecoveryCodes.ts';
	import type { UsernameAvailability } from '#lib/api/generated/UsernameAvailability.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import NewPassword from './NewPassword.svelte';

	let {
		invite = null,
		setup = null,
		intro
	}: { invite?: string | null; setup?: string | null; intro: string } = $props();

	let step = $state<1 | 2 | 3>(1);
	let username = $state('');
	let displayName = $state('');
	let availability = $state<UsernameAvailability | null>(null);
	let password = $state('');
	let passwordOk = $state(false);
	let codes = $state<string[]>([]);
	let saved = $state(false);
	let busy = $state(false);
	let error = $state('');
	let copied = $state(false);

	let checkTimer: ReturnType<typeof setTimeout>;
	function onUsernameInput() {
		clearTimeout(checkTimer);
		availability = null;
		const name = username;
		if (!name.trim()) return;
		checkTimer = setTimeout(async () => {
			try {
				const a = await post<UsernameAvailability>('/auth/username-available', { username: name });
				if (name === username) availability = a;
			} catch {
				// Offline or server error: the check runs again on submit.
			}
		}, 300);
	}

	function toPassword(e: SubmitEvent) {
		e.preventDefault();
		if (!displayName.trim()) displayName = username;
		step = 2;
	}

	async function create(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const res = await post<RecoveryCodes>('/auth/register', {
				username,
				display_name: displayName,
				time_zone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
				password,
				invite,
				setup
			});
			codes = res.codes;
			step = 3;
		} catch (e) {
			error = (e as Error).message;
			if ((e as { code?: string }).code === 'username_taken') step = 1;
		} finally {
			busy = false;
		}
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(codes.join('\n'));
			copied = true;
		} catch {
			error = 'Copying was blocked. Select the codes and copy them instead.';
		}
	}

	async function finish() {
		await auth.refresh();
		goto('/', { replaceState: true });
	}
</script>

<div class="steps" aria-label="Step {step} of 3">
	<span class:on={step >= 1}></span><span class:on={step >= 2}></span><span class:on={step >= 3}
	></span>
</div>

{#if step === 1}
	<p class={setup ? 'notice' : 'help'}>{intro}</p>
	<h1>Choose your name</h1>
	<form onsubmit={toPassword} style="display:grid;gap:14px">
		<label class="field" for="su-user"
			>Username<input
				id="su-user"
				bind:value={username}
				oninput={onUsernameInput}
				autocapitalize="none"
				autocomplete="username"
				spellcheck="false"
				required
			/></label
		>
		<p class="help" aria-live="polite">
			{#if availability}
				<span class={availability.available ? 'ok' : 'error'}>{availability.message}</span>
			{:else}
				3–30 characters: a–z, 0–9, _ and -. Shown in your profile address.
			{/if}
		</p>
		<label class="field" for="su-display"
			>Display name<input
				id="su-display"
				bind:value={displayName}
				placeholder="How your name appears"
			/></label
		>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn primary" type="submit" disabled={!availability?.available}>Continue</button>
	</form>
{:else if step === 2}
	<h1>Choose a password</h1>
	<form onsubmit={create} style="display:grid;gap:14px">
		<input type="text" name="username" autocomplete="username" value={username} hidden />
		<NewPassword bind:password bind:valid={passwordOk} id="su-pw" />
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button class="btn primary" type="submit" disabled={busy || !passwordOk}
			>{busy ? 'Creating your account…' : 'Create account'}</button
		>
	</form>
	<button class="link" type="button" onclick={() => (step = 1)}>Back</button>
{:else}
	<h1>Save your recovery codes</h1>
	<p class="help">
		If you forget your password or lose your authenticator app, one of these gets you back in. Each
		works once. They won't be shown again.
	</p>
	<div class="codes mono">
		{#each codes as c (c)}<span>{c}</span>{/each}
	</div>
	<button class="btn" type="button" onclick={copy}>{copied ? 'Copied' : 'Copy all'}</button>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<label class="check" for="su-saved"
		><input type="checkbox" id="su-saved" bind:checked={saved} /> I've saved these codes somewhere safe</label
	>
	<button class="btn primary" type="button" onclick={finish} disabled={!saved}
		>Start using mau</button
	>
{/if}

<style>
	.steps {
		display: flex;
		gap: 6px;
	}
	.steps span {
		flex: 1;
		height: 4px;
		border-radius: 2px;
		background: var(--line);
	}
	.steps span.on {
		background: var(--accent);
	}
	.codes {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 6px 14px;
		padding: 12px;
		border: 1px dashed var(--line);
		border-radius: 12px;
		font-size: 15px;
	}
	.check {
		display: flex;
		gap: 8px;
		align-items: center;
		cursor: pointer;
	}
	.check input {
		accent-color: var(--accent);
		width: 17px;
		height: 17px;
		margin: 0;
	}
</style>
