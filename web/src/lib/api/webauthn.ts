// Bridges the server's WebAuthn JSON (binary fields as base64url) and the
// browser's credential API.

import { ApiError, post } from './client.ts';
import type { CeremonyStart } from './generated/CeremonyStart.ts';

function fromB64u(s: string): ArrayBuffer {
	const b64 = s.replace(/-/g, '+').replace(/_/g, '/');
	const bin = atob(b64 + '='.repeat((4 - (b64.length % 4)) % 4));
	return Uint8Array.from(bin, (c) => c.charCodeAt(0)).buffer;
}

function toB64u(buf: ArrayBuffer | ArrayBufferView): string {
	const bytes =
		buf instanceof ArrayBuffer
			? new Uint8Array(buf)
			: new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
	let bin = '';
	for (const b of bytes) bin += String.fromCharCode(b);
	return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

type Desc = { id: string; type: string; transports?: string[] };

function creationOptions(json: any): CredentialCreationOptions {
	const pk = json.publicKey;
	return {
		publicKey: {
			...pk,
			challenge: fromB64u(pk.challenge),
			user: { ...pk.user, id: fromB64u(pk.user.id) },
			excludeCredentials: (pk.excludeCredentials ?? []).map((c: Desc) => ({
				...c,
				id: fromB64u(c.id)
			}))
		}
	};
}

function requestOptions(json: any): CredentialRequestOptions {
	const pk = json.publicKey;
	return {
		publicKey: {
			...pk,
			challenge: fromB64u(pk.challenge),
			allowCredentials: (pk.allowCredentials ?? []).map((c: Desc) => ({ ...c, id: fromB64u(c.id) }))
		}
	};
}

function credentialJson(cred: PublicKeyCredential): unknown {
	const r = cred.response as AuthenticatorAttestationResponse & AuthenticatorAssertionResponse;
	const response: Record<string, string | null> = { clientDataJSON: toB64u(r.clientDataJSON) };
	if ('attestationObject' in r) response.attestationObject = toB64u(r.attestationObject);
	if ('signature' in r) {
		response.authenticatorData = toB64u(r.authenticatorData);
		response.signature = toB64u(r.signature);
		response.userHandle = r.userHandle ? toB64u(r.userHandle) : null;
	}
	return {
		id: cred.id,
		rawId: toB64u(cred.rawId),
		type: cred.type,
		response,
		extensions: cred.getClientExtensionResults()
	};
}

/** Explains a failed browser passkey prompt in plain words. */
function promptError(e: unknown): ApiError {
	const name = e instanceof DOMException ? e.name : '';
	if (name === 'NotAllowedError' || name === 'AbortError')
		return new ApiError(400, 'cancelled', 'The passkey prompt was closed or timed out. Try again.');
	if (name === 'InvalidStateError')
		return new ApiError(
			400,
			'exists',
			'That authenticator already has a passkey for this account.'
		);
	if (name === 'SecurityError')
		return new ApiError(
			400,
			'security',
			"This site's address doesn't match the server's public_url setting."
		);
	return new ApiError(400, 'webauthn', `The passkey prompt failed${name ? ` (${name})` : ''}.`);
}

export function passkeysSupported(): boolean {
	return typeof window !== 'undefined' && 'PublicKeyCredential' in window;
}

/** Runs a registration ceremony: `start` returns options, then the browser
 *  creates a credential, then `finish` is called with it. */
export async function createPasskey<T>(
	start: () => Promise<CeremonyStart>,
	finish: (challenge: string, credential: unknown) => Promise<T>
): Promise<T> {
	const { challenge, options } = await start();
	let cred: Credential | null;
	try {
		cred = await navigator.credentials.create(creationOptions(options));
	} catch (e) {
		throw promptError(e);
	}
	if (!cred) throw new ApiError(400, 'cancelled', 'No passkey was created.');
	return finish(challenge, credentialJson(cred as PublicKeyCredential));
}

/** Runs an assertion ceremony (sign-in or confirmation). */
export async function usePasskey<T>(
	start: () => Promise<CeremonyStart>,
	finish: (challenge: string, credential: unknown) => Promise<T>
): Promise<T> {
	const { challenge, options } = await start();
	let cred: Credential | null;
	try {
		cred = await navigator.credentials.get(requestOptions(options));
	} catch (e) {
		throw promptError(e);
	}
	if (!cred) throw new ApiError(400, 'cancelled', 'No passkey was used.');
	return finish(challenge, credentialJson(cred as PublicKeyCredential));
}

/** Confirms a recent passkey for sensitive actions. */
export function confirmWithPasskey(): Promise<unknown> {
	return usePasskey(
		() => post<CeremonyStart>('/auth/reauth/start'),
		(challenge, credential) => post('/auth/reauth/finish', { challenge, credential })
	);
}

/** Runs `action`; if the server asks for a fresh passkey, confirms and retries once. */
export async function withReauth<T>(action: () => Promise<T>): Promise<T> {
	try {
		return await action();
	} catch (e) {
		if (e instanceof ApiError && e.code === 'reauth_required') {
			await confirmWithPasskey();
			return action();
		}
		throw e;
	}
}

/** A default passkey name: "Firefox on Linux". */
export function deviceName(): string {
	const ua = navigator.userAgent;
	const browser = /Firefox\//.test(ua)
		? 'Firefox'
		: /Edg\//.test(ua)
			? 'Edge'
			: /Chrome\/|Chromium\//.test(ua)
				? 'Chrome'
				: /Safari\//.test(ua)
					? 'Safari'
					: 'Browser';
	const os = /Android/.test(ua)
		? 'Android'
		: /iPhone|iPad/.test(ua)
			? 'iOS'
			: /Mac OS X/.test(ua)
				? 'macOS'
				: /Windows/.test(ua)
					? 'Windows'
					: /Linux|X11/.test(ua)
						? 'Linux'
						: 'unknown OS';
	return `${browser} on ${os}`;
}
