import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, get, post } from './client.ts';

const answer = (status: number, body: string, type = 'application/json') =>
	vi.stubGlobal(
		'fetch',
		vi.fn(async () => new Response(body, { status, headers: { 'content-type': type } }))
	);

afterEach(() => vi.unstubAllGlobals());

/** The error a request fails with (and fails the test if it succeeds). */
const failure = (p: Promise<unknown>) =>
	p.then(
		() => {
			throw new Error('expected the request to fail');
		},
		(e: ApiError) => e
	);

describe('api client', () => {
	it('returns the JSON body', async () => {
		answer(200, '{"a":1}');
		expect(await get('/x')).toEqual({ a: 1 });
	});

	it('sends JSON with a content type and the /api prefix', async () => {
		answer(200, '');
		await post('/thing', { b: 2 });
		const [url, init] = vi.mocked(fetch).mock.calls[0];
		expect(url).toBe('/api/thing');
		expect(init?.method).toBe('POST');
		expect(init?.body).toBe('{"b":2}');
		expect((init?.headers as Record<string, string>)['Content-Type']).toBe('application/json');
	});

	it("carries the server's error code and message", async () => {
		answer(409, '{"error":"username_taken","message":"That username is taken."}');
		const e = await failure(get('/x'));
		expect(e).toBeInstanceOf(ApiError);
		expect(e).toMatchObject({
			status: 409,
			code: 'username_taken',
			message: 'That username is taken.'
		});
		expect(e.offline).toBe(false);
	});

	it('turns a non-JSON error page (a proxy) into a plain ApiError', async () => {
		answer(502, '<html>Bad gateway</html>', 'text/html');
		const e = await failure(get('/x'));
		expect(e).toBeInstanceOf(ApiError);
		expect(e).toMatchObject({ status: 502, code: 'error', message: 'The server answered 502.' });
	});

	it('reports a network failure as offline', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => Promise.reject(new TypeError('Failed to fetch')))
		);
		const e = await failure(get('/x'));
		expect(e).toBeInstanceOf(ApiError);
		expect(e.offline).toBe(true);
		expect(e.message).toBe('Needs a connection.');
	});
});
