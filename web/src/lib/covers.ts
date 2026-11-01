// Uploading a cover image. The server stores it and returns its hash.

import { ApiError } from './api/client.ts';

const MAX_BYTES = 10 * 1024 * 1024;

export async function uploadCover(file: File): Promise<string> {
	if (file.size > MAX_BYTES) throw new ApiError(400, 'too_big', 'Choose an image under 10 MB.');
	let res: Response;
	try {
		res = await fetch('/api/covers', {
			method: 'POST',
			credentials: 'same-origin',
			headers: { 'Content-Type': file.type || 'application/octet-stream' },
			body: file
		});
	} catch {
		throw new ApiError(0, 'offline', 'Uploading a cover needs a connection.');
	}
	const data = await res.json().catch(() => null);
	if (!res.ok)
		throw new ApiError(
			res.status,
			data?.error ?? 'error',
			data?.message ?? 'The cover upload failed.'
		);
	return data.hash as string;
}
