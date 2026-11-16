<!-- The same footer on every page: where the source is, and its license.
     (The server-rendered profile and share pages draw the same line in
     public.rs.) Colours follow the page's theme; the landing page, on its
     dark background, sets the --footer-* colours. -->
<footer class="site-footer">
	<a href="https://github.com/rri/mau" rel="noopener">github.com/rri/mau</a>
	<span aria-hidden="true">·</span>
	<span>Source code licensed under AGPL-3.0</span>
</footer>

<style>
	.site-footer {
		display: flex;
		flex-wrap: nowrap;
		justify-content: center;
		align-items: center;
		gap: 4px 12px;
		padding: 20px max(16px, env(safe-area-inset-right)) calc(20px + env(safe-area-inset-bottom))
			max(16px, env(safe-area-inset-left));
		/* Small enough on phones that both parts and the dot share one line. */
		font-size: clamp(12px, 3.2vw, 13.5px);
		white-space: nowrap;
		color: var(--footer-ink, var(--muted));
	}
	@media (max-width: 359px) {
		.site-footer {
			flex-direction: column;
			white-space: normal;
		}
		.site-footer span[aria-hidden] {
			display: none;
		}
	}
	a {
		color: var(--footer-link, var(--muted));
		text-underline-offset: 3px;
	}
	a:focus-visible {
		outline-color: var(--footer-focus, var(--accent));
		outline-offset: 3px;
	}
	@media (hover: hover) {
		a:hover {
			color: var(--footer-link-hover, var(--ink));
		}
	}
</style>
