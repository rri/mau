// Hybrid logical clock, matching crates/mau-core/src/hlc.rs. Timestamps are
// "{ms:013}-{counter:04}-{node}", so string order is time order.

const NODE_KEY = 'mau.node';
const LAST_KEY = 'mau.hlc';

function nodeId(): string {
	let id = localStorage.getItem(NODE_KEY);
	if (!id) {
		const alphabet = 'abcdefghijklmnopqrstuvwxyz0123456789';
		const bytes = crypto.getRandomValues(new Uint8Array(10));
		id = Array.from(bytes, (b) => alphabet[b % alphabet.length]).join('');
		localStorage.setItem(NODE_KEY, id);
	}
	return id;
}

export function format(ms: number, counter: number, node: string): string {
	return `${String(ms).padStart(13, '0')}-${String(counter).padStart(4, '0')}-${node}`;
}

export function parse(h: string): { ms: number; counter: number; node: string } {
	const [ms, counter, node] = h.split('-');
	return { ms: Number(ms), counter: Number(counter), node };
}

let lastMs = 0;
let counter = 0;
let loaded = false;

function load() {
	if (loaded) return;
	loaded = true;
	const saved = localStorage.getItem(LAST_KEY);
	if (saved) {
		const p = parse(saved);
		lastMs = p.ms;
		counter = p.counter;
	}
}

/** A timestamp later than any this device has issued or observed. */
export function now(): string {
	load();
	const wall = Date.now();
	if (wall > lastMs) {
		lastMs = wall;
		counter = 0;
	} else {
		counter += 1;
	}
	const h = format(lastMs, counter, nodeId());
	localStorage.setItem(LAST_KEY, h);
	return h;
}

/** Moves the clock past a timestamp seen from another device. */
export function observe(h: string) {
	if (!h) return;
	load();
	const p = parse(h);
	if (p.ms > lastMs) {
		lastMs = p.ms;
		counter = p.counter;
	} else if (p.ms === lastMs) {
		counter = Math.max(counter, p.counter);
	}
}
