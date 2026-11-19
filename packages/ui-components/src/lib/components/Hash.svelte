<script lang="ts" context="module">
	export enum HashType {
		Identifier,
		Wallet,
		Transaction,
		Address
	}
</script>

<script lang="ts">
	import Tooltip from './Tooltip.svelte';
	import Sensitive from './Sensitive.svelte';
	import {
		WalletOutline,
		FingerprintOutline,
		ClipboardListOutline,
		ClipboardOutline
	} from 'flowbite-svelte-icons';
	import { fade } from 'svelte/transition';

	export let value: string;
	export let type: HashType | undefined = undefined;
	export let shorten = true;
	export let sliceLen = 5;
	export let copyOnClick = true;
	let showCopiedMessage = false;

	let cursorX = 0;
	let cursorY = 0;

	// Ellipsis truncation of an already-short id (e.g. vault `0xfab`) duplicates it as
	// `0xfab...0xfab`. Only shorten when the full value is longer than both slices.
	$: isTruncated = Boolean(shorten && value && value.length > sliceLen * 2);
	$: id = isTruncated ? `hash-${value}` : undefined;
	$: displayValue = isTruncated
		? `${value.slice(0, sliceLen)}...${value.slice(-1 * sliceLen)}`
		: value;

	function copy(e: MouseEvent) {
		if (copyOnClick) {
			e.stopPropagation();
			navigator.clipboard.writeText(value);
			cursorX = e.clientX;
			cursorY = e.clientY;
			showCopiedMessage = true;
			setTimeout(() => {
				showCopiedMessage = false;
			}, 1500);
		}
	}
</script>

<button
	type="button"
	{id}
	class="flex items-center justify-start space-x-2 text-left"
	on:click={copy}
>
	{#if type === HashType.Wallet}
		<WalletOutline size="sm" />
	{:else if type === HashType.Identifier}
		<FingerprintOutline size="sm" />
	{:else if type === HashType.Transaction}
		<ClipboardListOutline size="sm" />
	{:else if type === HashType.Address}
		<ClipboardOutline size="sm" />
	{/if}
	<Sensitive><div>{displayValue}</div></Sensitive>
</button>

{#if showCopiedMessage}
	<div
		out:fade
		class="fixed rounded bg-green-500 px-2 py-1 text-xs text-white"
		style="top: {cursorY + 10}px; left: {cursorX + 10}px"
	>
		Copied to clipboard
	</div>
{/if}

{#if isTruncated}
	<Tooltip triggeredBy={`#${id}`}>
		<div class="flex items-center justify-start space-x-2">
			{#if type === HashType.Wallet}
				<WalletOutline size="sm" />
			{:else if type === HashType.Identifier}
				<FingerprintOutline size="sm" />
			{:else if type === HashType.Transaction}
				<ClipboardListOutline size="sm" />
			{:else if type === HashType.Address}
				<ClipboardOutline size="sm" />
			{/if}
			<Sensitive><div>{value}</div></Sensitive>
		</div>
	</Tooltip>
{/if}
