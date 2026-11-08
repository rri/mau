<script lang="ts">
	import { goto } from '$app/navigation';
	import { del, patch } from '#lib/api/client.ts';
	import type { Me } from '#lib/api/generated/Me.ts';
	import { withReauth } from '#lib/confirm.svelte.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { library } from '#lib/library.svelte.ts';
	import { setAppearance, storedAppearance, type Appearance } from '#lib/theme.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	const me = auth.me!;
	let displayName = $state(me.display_name);
	let bio = $state(me.bio);
	let username = $state(me.username);
	let timeZone = $state(me.time_zone);
	let appearance = $state<Appearance>(storedAppearance());
	let error = $state('');
	let busy = $state(false);

	const zones = (() => {
		try {
			return Intl.supportedValuesOf('timeZone');
		} catch {
			return [me.time_zone];
		}
	})();
	const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone;

	async function save(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			const old = auth.me!.username;
			const updated = await withReauth(() =>
				patch<Me>('/account', { display_name: displayName, bio, time_zone: timeZone, username })
			);
			auth.setMe(updated);
			toasts.show(
				updated.username !== old
					? `Username changed. @${old} stays reserved for you for 30 days.`
					: 'Saved'
			);
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}

	// ---- Delete account -----------------------------------------------------
	let deleting = $state(false);
	let confirmName = $state('');
	let deleteError = $state('');

	async function deleteAccount() {
		deleteError = '';
		if (confirmName.trim().toLowerCase() !== me.username) {
			deleteError = "The username didn't match. Nothing was deleted.";
			return;
		}
		try {
			await withReauth(() => del('/account', { username: confirmName }));
			await library.wipe();
			auth.clear();
			goto('/signin', { replaceState: true });
		} catch (err) {
			deleteError = (err as Error).message;
		}
	}
</script>

<PageHead title="Account" />

<form class="sec" onsubmit={save}>
	<h2>Profile</h2>
	<label class="field">Display name<input bind:value={displayName} /></label>
	<label class="field"
		>Short bio (shown on your public profile)<textarea bind:value={bio} rows="3" maxlength="280"
		></textarea></label
	>
	<label class="field"
		>Username<input bind:value={username} autocapitalize="none" spellcheck="false" /></label
	>
	<p class="help">
		3–30 characters: a–z, 0–9, _ and -. If you change it, your profile moves to the new address
		right away and the old address stops working. Share links keep working. Your old username stays
		reserved for you for 30 days.
	</p>
	<label class="field"
		>Time zone (decides what counts as "today")
		<select bind:value={timeZone}
			>{#each zones as z (z)}<option>{z}</option>{/each}</select
		></label
	>
	{#if deviceZone && deviceZone !== timeZone}
		<p class="help">
			This device is set to {deviceZone}.
			<button class="link" type="button" onclick={() => (timeZone = deviceZone)}
				>Use {deviceZone}</button
			>
		</p>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<div class="row-end"><button class="btn primary" type="submit" disabled={busy}>Save</button></div>
</form>

<div class="sec">
	<h2>Appearance (this device)</h2>
	<div class="radio-list">
		{#each [['system', 'Match the system'], ['light', 'Light'], ['dark', 'Dark']] as [v, label] (v)}
			<label class="radio"
				><input
					type="radio"
					name="appearance"
					value={v}
					checked={appearance === v}
					onchange={() => {
						appearance = v as Appearance;
						setAppearance(appearance);
					}}
				/><span>{label}</span></label
			>
		{/each}
	</div>
</div>

<div class="sec">
	<h2>Delete account</h2>
	<p class="help">
		Deletes your account and everything in it: books, readings, ratings, notes, copies, tags, and
		shares. This can't be undone.
	</p>
	<div class="row-end">
		<a class="btn" href="/settings/import">Export my data first</a>
		<button class="btn danger-ghost" type="button" onclick={() => (deleting = true)}
			>Delete account…</button
		>
	</div>
</div>

{#if deleting}
	<div class="dialog-back" role="presentation">
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="del-h">
			<h3 id="del-h">Delete your account?</h3>
			<p>
				Everything in it is deleted right away and can't be recovered. You'll confirm with your
				password.
			</p>
			<label class="field"
				>Type your username, {me.username}, to confirm<input
					bind:value={confirmName}
					autocapitalize="none"
				/></label
			>
			{#if deleteError}<p class="error" role="alert">{deleteError}</p>{/if}
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (deleting = false)}>Cancel</button>
				<button class="btn danger" type="button" onclick={deleteAccount}>Delete</button>
			</div>
		</div>
	</div>
{/if}

<style>
	textarea {
		resize: vertical;
	}
</style>
