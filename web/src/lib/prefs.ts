// Small per-device preferences kept in localStorage.

export function getPref<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
	const v = localStorage.getItem(`mau.${key}`);
	return (allowed as readonly string[]).includes(v ?? '') ? (v as T) : fallback;
}

export function setPref(key: string, value: string) {
	localStorage.setItem(`mau.${key}`, value);
}
