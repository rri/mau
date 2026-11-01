<script lang="ts">
	import { toasts, TOAST_MS } from '../toast.svelte.ts';
</script>

{#if toasts.current}
	{#key toasts.current.id}
		<div class="toast" role="status">
			<span>{toasts.current.message}</span>
			{#if toasts.current.action}
				<button type="button" onclick={() => toasts.runAction()}
					>{toasts.current.action.label}</button
				>
			{/if}
			<div class="timer" style="animation-duration: {TOAST_MS}ms"></div>
		</div>
	{/key}
{/if}

<style>
	.toast {
		position: fixed;
		left: 50%;
		bottom: calc(76px + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		width: min(560px, calc(100% - 24px));
		z-index: 40;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 10px 10px 14px;
		border-radius: 12px;
		background: var(--ink);
		color: var(--surface);
		font-size: 15px;
		overflow: hidden;
	}
	button {
		background: none;
		border: 0;
		color: var(--surface);
		font-weight: 600;
		text-decoration: underline;
		cursor: pointer;
		padding: 4px 8px;
		flex: none;
	}
	.timer {
		position: absolute;
		left: 0;
		bottom: 0;
		height: 3px;
		width: 100%;
		background: var(--accent);
		transform-origin: left;
		animation: drain linear forwards;
	}
	@keyframes drain {
		to {
			transform: scaleX(0);
		}
	}
	@media (min-width: 900px) {
		.toast {
			bottom: 24px;
			left: calc(50% + 100px);
		}
	}
</style>
