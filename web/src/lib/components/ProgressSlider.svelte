<script lang="ts">
	// A book's progress as a slim bar you can drag: the position moves with
	// the finger (the page shown alongside follows via `preview`) and is
	// saved on release, with an undo toast. Arrow keys step a page (or 1%),
	// Page Up/Down ten, Home/End the ends; key presses save once they pause.
	//
	// On touch it takes a sideways drag to move it, so scrolling a list past
	// it (or a tap meant for the title above) never changes anything. With a
	// mouse, a click on the bar jumps there.
	import { setPages } from '../actions.ts';
	import { progressText, type BookInfo } from '../derive.ts';

	let {
		info,
		preview = $bindable(null),
		muted = false
	}: {
		info: BookInfo;
		/** The position while dragging (or stepping with keys), else null. */
		preview?: number | null;
		/** Inactive books: grey instead of the accent colour. */
		muted?: boolean;
	} = $props();

	const r = $derived(info.reading);
	const max = $derived(r?.unit === 'percent' ? 100 : (r?.total ?? 0));
	const pos = $derived(preview ?? r?.pos ?? 0);
	const frac = $derived(max ? Math.min(pos, max) / max : 0);
	const disabled = $derived(!r || !!r.finished_on);

	const R = 7; // the thumb's radius, as in the styles
	let el = $state<HTMLDivElement>();
	let dragging = $state(false);

	function at(clientX: number): number {
		// The thumb's centre runs from one radius in to one radius short.
		const box = el!.getBoundingClientRect();
		const f = Math.max(0, Math.min(1, (clientX - box.left - R) / Math.max(1, box.width - 2 * R)));
		return Math.round(f * max);
	}

	function save() {
		clearTimeout(keyTimer);
		const to = preview;
		preview = null;
		if (to !== null && r && to !== r.pos) setPages(info, to);
	}

	function down(e: PointerEvent) {
		if (disabled || e.button > 0) return;
		const startX = e.clientX;
		const startY = e.clientY;
		const touch = e.pointerType !== 'mouse';
		const id = e.pointerId;
		if (!touch) {
			e.preventDefault();
			el!.focus();
			start();
			preview = at(e.clientX);
		}
		function start() {
			dragging = true;
			el!.setPointerCapture(id);
		}
		const move = (ev: PointerEvent) => {
			if (ev.pointerId !== id) return;
			if (!dragging) {
				const dx = Math.abs(ev.clientX - startX);
				const dy = Math.abs(ev.clientY - startY);
				if (dy > 8 && dy > dx) return end(false);
				if (dx < 6) return;
				start();
			}
			preview = at(ev.clientX);
		};
		const up = (ev: PointerEvent) => ev.pointerId === id && end(true);
		const cancel = (ev: PointerEvent) => ev.pointerId === id && end(false);
		function end(keep: boolean) {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			window.removeEventListener('pointercancel', cancel);
			const was = dragging;
			dragging = false;
			if (was && keep) save();
			else preview = null;
		}
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
		window.addEventListener('pointercancel', cancel);
	}

	let keyTimer: ReturnType<typeof setTimeout> | undefined;
	function key(e: KeyboardEvent) {
		if (disabled || !r) return;
		const step = {
			ArrowRight: 1,
			ArrowUp: 1,
			ArrowLeft: -1,
			ArrowDown: -1,
			PageUp: 10,
			PageDown: -10
		}[e.key];
		let to: number;
		if (step !== undefined) to = pos + step;
		else if (e.key === 'Home') to = 0;
		else if (e.key === 'End') to = max;
		else return;
		e.preventDefault();
		preview = Math.max(0, Math.min(max, to));
		clearTimeout(keyTimer);
		keyTimer = setTimeout(save, 700);
	}

	$effect(() => () => clearTimeout(keyTimer));
</script>

<div
	bind:this={el}
	class="slider"
	class:muted
	class:dragging
	class:disabled
	style="--f:{frac}"
	role="slider"
	tabindex={disabled ? -1 : 0}
	aria-label="Progress in {info.title}"
	aria-valuemin={0}
	aria-valuemax={max}
	aria-valuenow={pos}
	aria-valuetext={r ? progressText({ ...r, pos }) : 'Not started'}
	aria-disabled={disabled || undefined}
	onpointerdown={down}
	onkeydown={key}
	onblur={() => preview !== null && !dragging && save()}
>
	<span class="track"><span class="fill"></span></span>
	{#if !disabled}<span class="thumb"></span>{/if}
</div>

<style>
	/* 20px tall to catch a finger; the bar itself is 4px, so with the small
	   margins it keeps clear of the lines above and below. */
	.slider {
		position: relative;
		height: 20px;
		margin: 2px 0;
		display: flex;
		align-items: center;
		cursor: pointer;
		touch-action: pan-y;
		user-select: none;
		-webkit-user-select: none;
		border-radius: 99px;
		outline-offset: 1px;
	}
	.slider.disabled {
		cursor: default;
	}
	.track {
		position: relative;
		width: 100%;
		height: 4px;
		border-radius: 99px;
		background: var(--line);
		overflow: hidden;
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		/* To the thumb's centre, which stays within the bar's ends. */
		width: calc(7px + (100% - 14px) * var(--f));
		border-radius: 99px;
		background: var(--accent);
		transition: width 0.3s;
	}
	.thumb {
		position: absolute;
		top: 50%;
		left: calc(7px + (100% - 14px) * var(--f));
		width: 14px;
		height: 14px;
		margin: -7px 0 0 -7px;
		border-radius: 50%;
		background: var(--accent);
		border: 2px solid var(--surface);
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.25);
		transition:
			left 0.3s,
			transform 0.12s;
	}
	/* Following the finger: no easing, and the thumb grows so it shows
	   around the fingertip. */
	.dragging .fill,
	.dragging .thumb {
		transition: none;
	}
	.dragging .thumb,
	.slider:not(.disabled):active .thumb {
		transform: scale(1.35);
	}
	.muted .fill,
	.muted .thumb {
		background: var(--muted);
	}
	.muted .fill {
		opacity: 0.6;
	}
	@media (hover: hover) {
		.slider:not(.disabled):hover .thumb {
			transform: scale(1.15);
		}
	}
</style>
