import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Breadcrumbs from './Breadcrumbs.svelte';

describe('Breadcrumbs', () => {
	it('links earlier steps and marks the current page', () => {
		render(Breadcrumbs, {
			items: [
				{ label: 'Settings', href: '/settings' },
				{ label: 'Admin', href: '/settings#admin' },
				{ label: 'Users' }
			]
		});
		const trail = screen.getByRole('navigation', { name: 'Breadcrumb' });
		const links = trail.querySelectorAll('ol a');
		expect([...links].map((a) => a.getAttribute('href'))).toEqual(['/settings', '/settings#admin']);
		expect(screen.getByText('Users')).toHaveAttribute('aria-current', 'page');
	});

	it('offers the parent as the short form for phones', () => {
		render(Breadcrumbs, { items: [{ label: 'Home', href: '/' }, { label: 'Dune' }] });
		const short = document.querySelector('.short a');
		expect(short).toHaveAttribute('href', '/');
		expect(short).toHaveTextContent('Home');
	});

	it('runs a step handler instead of a link when there is no href', async () => {
		const onclick = vi.fn();
		render(Breadcrumbs, {
			items: [
				{ label: 'Catalog', href: '/catalog' },
				{ label: 'Add a book', onclick },
				{ label: 'Enter a book' }
			]
		});
		await userEvent.click(screen.getAllByRole('button', { name: 'Add a book' })[0]);
		expect(onclick).toHaveBeenCalledOnce();
	});
});
