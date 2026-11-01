// Appearance: follow the system, or pin light/dark on this device.

export type Appearance = 'system' | 'light' | 'dark';
const KEY = 'mau.appearance';

export function storedAppearance(): Appearance {
	const v = localStorage.getItem(KEY);
	return v === 'light' || v === 'dark' ? v : 'system';
}

export function setAppearance(a: Appearance) {
	if (a === 'system') localStorage.removeItem(KEY);
	else localStorage.setItem(KEY, a);
	applyStoredTheme();
}

export function applyStoredTheme() {
	const a = storedAppearance();
	if (a === 'system') document.documentElement.removeAttribute('data-theme');
	else document.documentElement.dataset.theme = a;
}
