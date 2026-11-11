// Shared pieces (spec §10): toast, dialogs, tag input, covers, charts, scanner.
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Toast from './Toast.svelte';
import ConfirmDialog from './ConfirmDialog.svelte';
import StartDialog from './StartDialog.svelte';
import TagInput from './TagInput.svelte';
import Cover from './Cover.svelte';
import BarChart from './BarChart.svelte';
import Scanner from './Scanner.svelte';
import { confirmation, withReauth } from '../confirm.svelte.ts';
import { bookInfo } from '../derive.ts';
import { toasts, TOAST_MS } from '../toast.svelte.ts';
import { mockApi, sampleLibrary, signIn } from '../../test/fixtures.ts';

describe('Toast', () => {
	it('UI-01 shows one message at a time with its action, and goes after 10 seconds', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		render(Toast);
		const undo = vi.fn();
		toasts.show('First');
		toasts.undo('Paused Dune', undo);
		expect(await screen.findByRole('status')).toHaveTextContent('Paused Dune');
		expect(screen.queryByText('First')).toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
		expect(undo).toHaveBeenCalledOnce();
		expect(toasts.current).toBeNull();
		toasts.show('Saved');
		vi.advanceTimersByTime(TOAST_MS - 1);
		expect(toasts.current?.message).toBe('Saved');
		vi.advanceTimersByTime(1);
		expect(toasts.current).toBeNull();
		expect(TOAST_MS).toBe(10_000);
	});
});

describe('Dialogs', () => {
	it('UI-10 the password confirmation retries the action once confirmed', async () => {
		signIn();
		let ok = false;
		mockApi({
			'POST /auth/confirm': (b: { password: string }) =>
				b.password === 'right'
					? ((ok = true), { ok: true })
					: new Response('{"error":"bad_password","message":"That password isn’t right."}', {
							status: 401
						})
		});
		render(ConfirmDialog);
		const action = vi.fn(async () => {
			if (!ok) throw Object.assign(new Error('x'), { code: 'reauth_required' });
			return 'done';
		});
		// withReauth only reacts to the API's own errors.
		const { ApiError } = await import('../api/client.ts');
		action.mockImplementation(async () => {
			if (!ok) throw new ApiError(403, 'reauth_required', 'Confirm.');
			return 'done';
		});
		const result = withReauth(action);
		const pw = await screen.findByLabelText('Password');
		expect(pw).toHaveFocus();
		await userEvent.type(pw, 'wrong{Enter}');
		expect(await screen.findByRole('alert')).toHaveTextContent('That password isn’t right.');
		await userEvent.clear(pw);
		await userEvent.type(pw, 'right{Enter}');
		expect(await result).toBe('done');
		expect(action).toHaveBeenCalledTimes(2);
		expect(confirmation.open).toBe(false);
	});

	it('UI-11 Tab stays inside a dialog; Escape closes only the top one; focus returns', async () => {
		const d = sampleLibrary();
		const info = bookInfo(d, d.books.get('se')!);
		const opener = document.createElement('button');
		document.body.append(opener);
		opener.focus();
		const cancel = vi.fn();
		const { unmount } = render(StartDialog, { info, onstart: vi.fn(), oncancel: cancel });
		const total = screen.getByLabelText('How many pages does your copy have?');
		await vi.waitFor(() => expect(total).toHaveFocus());
		const buttons = screen.getAllByRole('button');
		buttons.at(-1)!.focus();
		await userEvent.tab();
		expect(total).toHaveFocus();
		await userEvent.tab({ shift: true });
		expect(buttons.at(-1)).toHaveFocus();
		await userEvent.keyboard('{Escape}');
		expect(cancel).toHaveBeenCalledOnce();
		unmount();
		expect(opener).toHaveFocus();
		opener.remove();
	});
});

describe('Tag input', () => {
	it('UI-20 suggests existing tags (any case) first by prefix, offers a new one, and knows current tags', async () => {
		const add = vi.fn();
		render(TagInput, {
			existing: ['Sci-fi', 'classics', 'scifi-old', 'fantasy'],
			current: ['fantasy'],
			onadd: add
		});
		const box = screen.getByRole('combobox', { name: 'Add tag' });
		await userEvent.type(box, 'sci');
		expect(screen.getAllByRole('option').map((o) => o.textContent?.trim())).toEqual([
			'#Sci-fi',
			'#scifi-old',
			'Create new tag #sci'
		]);
		expect(box).toHaveAttribute('aria-expanded', 'true');
		await userEvent.keyboard('{ArrowDown}{Enter}');
		expect(add).toHaveBeenCalledWith('scifi-old');
		expect(box).toHaveValue('');
		await userEvent.type(box, '#fantasy');
		expect(screen.getByRole('option', { name: 'Already tagged #fantasy' })).toBeInTheDocument();
		await userEvent.keyboard('{Enter}');
		expect(add).toHaveBeenCalledTimes(1);
		await userEvent.keyboard('{Escape}');
		expect(box).toHaveValue('');
	});
});

describe('Covers and charts', () => {
	it('UI-30 a cover image, or a generated cover when there is none or it fails to load', async () => {
		const { container, unmount } = render(Cover, {
			title: 'Dune',
			author: 'Frank Herbert',
			cover: 'abc',
			size: 'm'
		});
		const img = container.querySelector('img')!;
		expect(img).toHaveAttribute('src', '/covers/abc-m.jpg');
		expect(img).toHaveAttribute('alt', '');
		await fireEvent.error(img);
		expect(container.querySelector('img')).toBeNull();
		expect(container.querySelector('svg')).toHaveTextContent('DUNE');
		unmount();
		const gen = render(Cover, {
			title: 'A Very Long Title That Wraps Over Several Lines',
			author: 'X'
		});
		expect(gen.container.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
		expect(gen.container.querySelectorAll('svg text').length).toBeGreaterThan(2);
	});

	it('UI-31 bars scale to the largest; zero is flat; hovering shows the tip', async () => {
		const { container } = render(BarChart, {
			caption: 'Pages',
			bars: [
				{ label: 'a', value: 0, tip: 'a: none' },
				{ label: 'b', value: 50, tip: 'b: 50' },
				{ label: 'c', value: 100, tip: 'c: 100', current: true }
			]
		});
		const bars = [...container.querySelectorAll<HTMLElement>('.plot .bar')];
		expect(bars.map((b) => b.style.height)).toEqual(['0%', '50%', '100%']);
		expect(bars[0]).toHaveClass('zero');
		expect(bars[2]).toHaveClass('current');
		await fireEvent.pointerEnter(bars[1].parentElement!);
		expect(screen.getAllByText('b: 50')).toHaveLength(2);
	});
});

describe('Scanner', () => {
	it('UI-40 without camera access it says to type the ISBN; it closes with ✕ or Escape', async () => {
		vi.stubGlobal('navigator', { ...navigator, mediaDevices: undefined });
		const close = vi.fn();
		render(Scanner, { onfound: vi.fn(), onclose: close });
		expect(
			await screen.findByText("This browser can't use the camera here. Type the ISBN instead.")
		).toBeInTheDocument();
		await userEvent.click(screen.getByRole('button', { name: 'Close scanner' }));
		await userEvent.keyboard('{Escape}');
		await userEvent.click(screen.getByRole('button', { name: 'Type the ISBN instead' }));
		expect(close).toHaveBeenCalledTimes(3);
	});

	it('UI-41 a refused camera says how to allow it', async () => {
		vi.stubGlobal('navigator', {
			...navigator,
			mediaDevices: { getUserMedia: () => Promise.reject(new Error('NotAllowedError')) }
		});
		render(Scanner, { onfound: vi.fn(), onclose: vi.fn() });
		expect(
			await screen.findByText(/Allow camera access, or type the ISBN instead/)
		).toBeInTheDocument();
	});
});
