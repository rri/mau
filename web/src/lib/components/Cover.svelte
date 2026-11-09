<script lang="ts">
	// A book cover: the real image when there is one, otherwise a generated
	// cover with the title and author.
	let {
		title,
		author = '',
		cover = null,
		src = null,
		size = 's'
	}: {
		title: string;
		author?: string;
		cover?: string | null;
		src?: string | null;
		size?: 's' | 'm' | 'l';
	} = $props();

	let failed = $state(false);

	function hue(s: string) {
		let h = 0;
		for (const c of s) h = (h * 31 + (c.codePointAt(0) ?? 0)) % 360;
		return h;
	}
	function wrap(text: string, max: number) {
		const lines: string[] = [];
		let cur = '';
		for (const w of text.split(' ')) {
			if ((cur + ' ' + w).trim().length > max && cur) {
				lines.push(cur);
				cur = w;
			} else cur = (cur + ' ' + w).trim();
		}
		return [...lines, cur].slice(0, 5);
	}
	const h = $derived(hue(title));
	const style = $derived(h % 4);
	const bg = $derived(`hsl(${h} 42% 30%)`);
	const fg = $derived(`hsl(${(h + 40) % 360} 70% 86%)`);
	const mid = $derived(`hsl(${(h + 180) % 360} 55% 58%)`);
	const lines = $derived(wrap(title.toUpperCase(), 11));
	const authorY = $derived(style === 3 ? 262 : 280);
	// Lines are 20 units apart, centered on the style's title position, but
	// the last line always stays 22 units above the author.
	const firstY = $derived(
		Math.min(
			(style === 3 ? 238 : style === 2 ? 50 : 232) - (lines.length - 1) * 10,
			authorY - 22 - (lines.length - 1) * 20
		)
	);
	const sunY = $derived(firstY + (lines.length - 1) * 20 + 30);
	// The diamond (style 3) ends above the title's first line.
	const gemBottom = $derived(Math.min(210, firstY - 26));
	const gemMid = $derived((90 + gemBottom) / 2);
</script>

{#if (src || cover) && !failed}
	<img
		class="cover"
		src={src ?? `/covers/${cover}-${size}.jpg`}
		alt=""
		loading="lazy"
		onerror={() => (failed = true)}
	/>
{:else}
	<svg class="cover" viewBox="0 0 200 300" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
		<rect width="200" height="300" fill={bg} />
		<rect x="0" y="0" width="7" height="300" fill="#000" opacity=".18" />
		{#if style === 0}
			<circle cx="100" cy="128" r="62" fill={mid} opacity=".85" /><circle
				cx="100"
				cy="128"
				r="38"
				fill={bg}
			/>
		{:else if style === 1}
			{#each [0, 1, 2, 3, 4, 5] as i (i)}
				<rect x="0" y={98 + i * 18} width="200" height="9" fill={mid} opacity={0.9 - i * 0.13} />
			{/each}
		{:else if style === 2}
			<!-- The sun sits below the title, behind the mountains. -->
			<circle cx="150" cy={sunY} r="16" fill={fg} /><path
				d="M0 210 L60 120 L95 165 L140 95 L200 190 L200 300 L0 300Z"
				fill={mid}
			/>
		{:else}
			<rect x="22" y="22" width="156" height="256" fill="none" stroke={fg} stroke-width="2" /><path
				d="M100 90 L140 {gemMid} L100 {gemBottom} L60 {gemMid}Z"
				fill={mid}
			/>
		{/if}
		{#each lines as l, i (i)}
			<text
				x="100"
				y={firstY + i * 20}
				text-anchor="middle"
				font-family="Geist Variable, system-ui, sans-serif"
				font-weight="700"
				font-size="16"
				letter-spacing="0.5"
				fill={fg}>{l}</text
			>
		{/each}
		<text
			x="100"
			y={authorY}
			text-anchor="middle"
			font-family="Geist Variable, system-ui, sans-serif"
			font-size="10"
			font-weight="500"
			letter-spacing="1.2"
			fill={fg}
			opacity=".85">{author.toUpperCase()}</text
		>
	</svg>
{/if}

<style>
	.cover {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		border-radius: 4px;
		box-shadow:
			0 1px 2px rgb(0 0 0 / 0.18),
			0 0 0 1px rgb(0 0 0 / 0.06);
		background: var(--line);
	}
</style>
