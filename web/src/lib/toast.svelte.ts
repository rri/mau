// One toast at a time, with an optional action (usually Undo). 10 seconds.

type Toast = { id: number; message: string; action?: { label: string; run: () => void } };

const DURATION_MS = 10_000;

class Toasts {
	current = $state<Toast | null>(null);
	private timer: ReturnType<typeof setTimeout> | undefined;
	private next = 1;

	show(message: string, action?: Toast['action']) {
		clearTimeout(this.timer);
		this.current = { id: this.next++, message, action };
		this.timer = setTimeout(() => (this.current = null), DURATION_MS);
	}

	undo(message: string, run: () => void) {
		this.show(message, { label: 'Undo', run });
	}

	runAction() {
		const t = this.current;
		this.current = null;
		clearTimeout(this.timer);
		t?.action?.run();
	}

	dismiss() {
		this.current = null;
	}
}

export const toasts = new Toasts();
export const TOAST_MS = DURATION_MS;
