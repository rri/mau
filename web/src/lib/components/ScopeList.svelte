<script lang="ts">
	// What a token or an app may do, compactly: READ and READ & MODIFY, each
	// with its scopes as small chips. The same groups as when a token is made.
	import type { ScopeInfo } from '#lib/api/generated/ScopeInfo.ts';

	let { scopes, markPrivate = false }: { scopes: ScopeInfo[]; markPrivate?: boolean } = $props();

	const groups = $derived(
		[
			{ name: 'READ', list: scopes.filter((s) => s.group === 'read') },
			{ name: 'READ & MODIFY', list: scopes.filter((s) => s.group !== 'read') }
		].filter((g) => g.list.length)
	);
</script>

<dl class="scopes">
	{#each groups as g (g.name)}
		<div class="grp">
			<dt>{g.name}</dt>
			<dd>
				<ul>
					{#each g.list as s (s.scope)}
						<li class:private={markPrivate && s.scope.startsWith('notes:')}>
							{s.label}{#if markPrivate && s.scope.startsWith('notes:')}<span class="sr-only">
									(private)</span
								>{/if}
						</li>
					{/each}
				</ul>
			</dd>
		</div>
	{/each}
</dl>

<style>
	/* One grid for both rows, so their chips line up. */
	.scopes {
		margin: 0;
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: 4px 10px;
		align-items: baseline;
	}
	.grp {
		display: contents;
	}
	/* The same heading as the checkboxes' when a token is generated. */
	dt {
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	dd {
		margin: 0;
	}
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	/* Like the Catalog's filter chips, smaller; notes like a warning pill. */
	li {
		font-size: 13px;
		padding: 1px 9px;
		border-radius: 999px;
		background: var(--surface);
		border: 1px solid var(--line);
		color: var(--muted);
	}
	li.private {
		background: color-mix(in srgb, var(--warn) 15%, transparent);
		border-color: transparent;
		color: var(--warn);
		font-weight: 600;
	}
</style>
