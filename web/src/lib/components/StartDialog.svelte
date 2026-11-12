<script lang="ts">
	// Asks how to count a never-started book before its first activation
	// from the Reading grid (or its ⋯ menu). The list asks inline instead.
	import type { Start } from '../actions.ts';
	import type { BookInfo } from '../derive.ts';
	import { modal } from '../modal.ts';
	import StartForm from './StartForm.svelte';

	let {
		info,
		onstart,
		oncancel
	}: { info: BookInfo; onstart: (start: Start) => void; oncancel: () => void } = $props();
</script>

<div class="dialog-back" role="presentation">
	<div
		class="dialog"
		role="dialog"
		aria-modal="true"
		aria-labelledby="start-{info.book.id}-h"
		use:modal={oncancel}
	>
		<StartForm {info} {onstart} {oncancel} heading={info.title} />
	</div>
</div>
