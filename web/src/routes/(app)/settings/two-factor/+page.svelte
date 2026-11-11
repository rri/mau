<script lang="ts">
	import { del, post } from '#lib/api/client.ts';
	import type { Me } from '#lib/api/generated/Me.ts';
	import type { TotpSetup } from '#lib/api/generated/TotpSetup.ts';
	import { auth } from '#lib/auth.svelte.ts';
	import { withReauth } from '#lib/confirm.svelte.ts';
	import { modal } from '#lib/modal.ts';
	import { toasts } from '#lib/toast.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';

	let setup = $state<TotpSetup | null>(null);
	let code = $state('');
	let busy = $state(false);
	let error = $state('');
	let turningOff = $state(false);

	const on = $derived(auth.me?.two_factor ?? false);
	/** The secret in groups of four, easier to type. */
	const grouped = $derived(setup?.secret.match(/.{1,4}/g)?.join(' ') ?? '');
	const qrUrl = $derived(
		setup ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(setup.qr_svg)}` : ''
	);

	function failed(err: unknown) {
		if ((err as { code?: string }).code !== 'cancelled') error = (err as Error).message;
	}

	async function start() {
		busy = true;
		error = '';
		try {
			setup = await withReauth(() => post<TotpSetup>('/two-factor/setup'));
			code = '';
		} catch (err) {
			failed(err);
		} finally {
			busy = false;
		}
	}

	async function enable(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			auth.setMe(await post<Me>('/two-factor/enable', { ticket: setup?.ticket, code }));
			setup = null;
			toasts.show('Two-factor is on');
		} catch (err) {
			failed(err);
			if ((err as { code?: string }).code === 'ticket_expired') setup = null;
		} finally {
			busy = false;
		}
	}

	async function disable() {
		busy = true;
		error = '';
		try {
			auth.setMe(await withReauth(() => del<Me>('/two-factor')));
			turningOff = false;
			toasts.show('Two-factor is off');
		} catch (err) {
			failed(err);
		} finally {
			busy = false;
		}
	}
</script>

<PageHead title="Two-factor" />

{#if setup}
	<div class="sec">
		<h2>1. Scan this with your authenticator app</h2>
		<p class="help">
			Any app that shows 6-digit codes works, such as Aegis, 2FAS, Google Authenticator, or a
			password manager.
		</p>
		<img class="qr" src={qrUrl} alt="QR code for your authenticator app" width="200" height="200" />
		<p class="help">Can't scan? Enter this key in the app instead (time-based):</p>
		<p class="mono key">{grouped}</p>
	</div>
	<form class="sec" onsubmit={enable}>
		<h2>2. Enter the code it shows</h2>
		<label class="field" for="tf-code"
			>Code<input
				id="tf-code"
				class="mono"
				bind:value={code}
				inputmode="numeric"
				autocomplete="one-time-code"
				placeholder="123456"
				required
			/></label
		>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="row-end">
			<button class="btn" type="button" onclick={() => (setup = null)}>Cancel</button>
			<button class="btn primary" type="submit" disabled={busy}>Turn on</button>
		</div>
	</form>
{:else if on}
	<div class="sec">
		<p><b>Two-factor is on.</b></p>
		<p class="help">
			Signing in asks for a code from your authenticator app after your password. If you lose the
			app, use one of your <a href="/settings/recovery">recovery codes</a> instead of a code.
		</p>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="row-end">
			<button class="btn danger-ghost" type="button" onclick={() => (turningOff = true)}
				>Turn off…</button
			>
		</div>
	</div>
{:else}
	<div class="sec">
		<p><b>Two-factor is off.</b></p>
		<p class="help">
			With two-factor on, signing in needs a 6-digit code from an app on your phone as well as your
			password, so a stolen password isn't enough.
		</p>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="row-end">
			<button class="btn primary" type="button" onclick={start} disabled={busy}
				>Set up two-factor</button
			>
		</div>
	</div>
{/if}

{#if turningOff}
	<div class="dialog-back" role="presentation">
		<div
			class="dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="off-h"
			use:modal={() => (turningOff = false)}
		>
			<h3 id="off-h">Turn off two-factor?</h3>
			<p>Signing in will need only your password. You'll confirm with your password and a code.</p>
			<div class="row-end">
				<button class="btn" type="button" onclick={() => (turningOff = false)}>Cancel</button>
				<button class="btn danger" type="button" onclick={disable} disabled={busy}>Turn off</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.qr {
		display: block;
		background: #fff;
		border-radius: 8px;
		padding: 6px;
	}
	.key {
		font-size: 15px;
		overflow-wrap: anywhere;
	}
	form.sec {
		display: grid;
		gap: 12px;
	}
</style>
