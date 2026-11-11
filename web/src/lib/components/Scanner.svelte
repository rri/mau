<script lang="ts">
	// Live barcode scanning with the camera. The decoder (zxing-wasm) is
	// bundled with mau and loaded only when the scanner opens.
	import { onDestroy, onMount } from 'svelte';
	import { normalizeIsbn } from '../isbn.ts';
	import { modal } from '../modal.ts';

	let { onfound, onclose }: { onfound: (isbn: string) => void; onclose: () => void } = $props();

	let video = $state<HTMLVideoElement>();
	let error = $state('');
	let stream: MediaStream | null = null;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let stopped = false;

	async function start() {
		if (!navigator.mediaDevices?.getUserMedia) {
			error = "This browser can't use the camera here. Type the ISBN instead.";
			return;
		}
		try {
			stream = await navigator.mediaDevices.getUserMedia({
				video: { facingMode: 'environment' },
				audio: false
			});
		} catch {
			error = 'mau couldn’t open a camera. Allow camera access, or type the ISBN instead.';
			return;
		}
		if (!video) return;
		video.srcObject = stream;
		await video.play().catch(() => {});
		const [{ prepareZXingModule, readBarcodes }, wasm] = await Promise.all([
			import('zxing-wasm/reader'),
			import('zxing-wasm/reader/zxing_reader.wasm?url')
		]);
		prepareZXingModule({
			overrides: {
				locateFile: (path: string, prefix: string) =>
					path.endsWith('.wasm') ? wasm.default : prefix + path
			}
		});
		const canvas = document.createElement('canvas');
		const ctx = canvas.getContext('2d', { willReadFrequently: true });
		const tick = async () => {
			if (stopped || !video || !ctx) return;
			if (video.videoWidth) {
				canvas.width = video.videoWidth;
				canvas.height = video.videoHeight;
				ctx.drawImage(video, 0, 0);
				const results = await readBarcodes(ctx.getImageData(0, 0, canvas.width, canvas.height), {
					formats: ['EAN-13'],
					tryHarder: true,
					maxNumberOfSymbols: 1
				}).catch(() => []);
				const isbn = results.map((r) => normalizeIsbn(r.text)).find((x) => x && /^97[89]/.test(x));
				if (isbn) {
					navigator.vibrate?.(60);
					stop();
					onfound(isbn);
					return;
				}
			}
			timer = setTimeout(tick, 250);
		};
		tick();
	}

	function stop() {
		stopped = true;
		clearTimeout(timer);
		stream?.getTracks().forEach((t) => t.stop());
		stream = null;
	}

	onMount(start);
	onDestroy(stop);
</script>

<div
	class="scanner"
	role="dialog"
	aria-modal="true"
	aria-label="Scan a barcode"
	use:modal={() => (stop(), onclose())}
>
	<div class="top">
		<span>Scan the barcode on the back cover</span>
		<button type="button" aria-label="Close scanner" onclick={() => (stop(), onclose())}>✕</button>
	</div>
	<div class="view">
		<!-- svelte-ignore a11y_media_has_caption -->
		<video bind:this={video} playsinline muted></video>
		<div class="frame"></div>
	</div>
	<div class="bottom">
		{#if error}<p>{error}</p>{:else}<p>Hold the barcode inside the frame.</p>{/if}
		<button class="link light" type="button" onclick={() => (stop(), onclose())}
			>Type the ISBN instead</button
		>
	</div>
</div>

<style>
	.scanner {
		position: fixed;
		inset: 0;
		z-index: 60;
		background: #0b0d10;
		color: #fff;
		display: grid;
		grid-template-rows: auto 1fr auto;
	}
	.top {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 12px 16px;
	}
	.top button {
		background: none;
		border: 0;
		color: #fff;
		font-size: 18px;
		cursor: pointer;
	}
	.view {
		position: relative;
		display: grid;
		place-items: center;
		overflow: hidden;
	}
	video {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.frame {
		position: absolute;
		width: min(80%, 340px);
		aspect-ratio: 3 / 2;
		border: 2px solid rgb(255 255 255 / 0.85);
		border-radius: 14px;
		box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.35);
	}
	.bottom {
		padding: 16px 18px 26px;
		display: grid;
		gap: 10px;
		justify-items: center;
		text-align: center;
	}
	.bottom p {
		margin: 0;
		color: rgb(255 255 255 / 0.85);
	}
	.light {
		color: #fff;
	}
</style>
