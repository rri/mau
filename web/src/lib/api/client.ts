// Thin JSON client for the mau API. Errors carry the server's code and a
// message that can be shown to the user as is.

export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string
	) {
		super(message);
	}

	/** The request never reached the server (offline or server down). */
	get offline() {
		return this.status === 0;
	}
}

export async function api<T = unknown>(method: string, path: string, body?: unknown): Promise<T> {
	let res: Response;
	try {
		res = await fetch(`/api${path}`, {
			method,
			credentials: 'same-origin',
			headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
			body: body === undefined ? undefined : JSON.stringify(body)
		});
	} catch {
		throw new ApiError(0, 'offline', 'Needs a connection.');
	}
	const text = await res.text();
	const data = text ? JSON.parse(text) : null;
	if (!res.ok) {
		throw new ApiError(
			res.status,
			data?.error ?? 'error',
			data?.message ?? `The server answered ${res.status}.`
		);
	}
	return data as T;
}

export const get = <T>(path: string) => api<T>('GET', path);
export const post = <T>(path: string, body: unknown = {}) => api<T>('POST', path, body);
export const patch = <T>(path: string, body: unknown) => api<T>('PATCH', path, body);
export const put = <T>(path: string, body: unknown) => api<T>('PUT', path, body);
export const del = <T>(path: string, body?: unknown) => api<T>('DELETE', path, body);
