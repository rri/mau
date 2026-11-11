// Dialog behavior shared by every modal: focus moves into it when it opens
// (unless a field already took focus), Tab stays inside it, Escape cancels,
// and focus goes back where it was when it closes.

const FOCUSABLE =
	'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Open dialogs, newest last: Escape closes only the one on top.
const stack: HTMLElement[] = [];

// The element focused before the current one: a field inside a dialog can
// take focus before the dialog itself is set up.
let previous: HTMLElement | null = null;
let current: HTMLElement | null = null;
if (typeof document !== 'undefined')
	document.addEventListener('focusin', (e) => {
		previous = current;
		current = e.target as HTMLElement;
	});

export function modal(node: HTMLElement, oncancel: () => void) {
	let cancel = oncancel;
	stack.push(node);
	const active = document.activeElement as HTMLElement | null;
	const before = active && node.contains(active) ? previous : active;
	const items = () => [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.hidden);

	queueMicrotask(() => {
		if (node.contains(document.activeElement)) return;
		const first = items()[0];
		if (first) first.focus();
		else {
			node.tabIndex = -1;
			node.focus();
		}
	});

	function escape(e: KeyboardEvent) {
		if (e.key !== 'Escape' || stack.at(-1) !== node) return;
		e.preventDefault();
		cancel();
	}
	function key(e: KeyboardEvent) {
		if (e.key === 'Tab') {
			const list = items();
			if (!list.length) return e.preventDefault();
			const i = list.indexOf(document.activeElement as HTMLElement);
			if (e.shiftKey && i <= 0) {
				e.preventDefault();
				list.at(-1)!.focus();
			} else if (!e.shiftKey && i === list.length - 1) {
				e.preventDefault();
				list[0].focus();
			}
		}
	}
	node.addEventListener('keydown', key);
	window.addEventListener('keydown', escape);

	return {
		update(next: () => void) {
			cancel = next;
		},
		destroy() {
			node.removeEventListener('keydown', key);
			window.removeEventListener('keydown', escape);
			stack.splice(stack.indexOf(node), 1);
			if (before && document.contains(before)) before.focus();
		}
	};
}
