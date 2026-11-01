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
	const titleY = $derived(style === 3 ? 238 : style === 2 ? 50 : 232);
</script>

{#if cover && !failed}
	<img
		class="cover"
		src="/covers/{cover}-{size}.jpg"
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
			<path
				d="M0 210 L60 120 L95 165 L140 95 L200 190 L200 300 L0 300Z"
				fill={mid}
				opacity=".8"
			/><circle cx="150" cy="70" r="16" fill={fg} />
		{:else}
			<rect x="22" y="22" width="156" height="256" fill="none" stroke={fg} stroke-width="2" /><path
				d="M100 90 L140 150 L100 210 L60 150Z"
				fill={mid}
			/>
		{/if}
		{#each lines as l, i (i)}
			<text
				x="100"
				y={titleY + i * 20 - (lines.length - 1) * 10}
				text-anchor="middle"
				font-family="Newsreader Variable, Georgia, serif"
				font-weight="600"
				font-size="17"
				letter-spacing="1"
				fill={fg}>{l}</text
			>
		{/each}
		<text
			x="100"
			y={style === 3 ? 262 : 280}
			text-anchor="middle"
			font-family="Newsreader Variable, Georgia, serif"
			font-size="11"
			letter-spacing="1.5"
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
		border-radius: 2px;
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.25);
		background: var(--line);
	}
</style>
