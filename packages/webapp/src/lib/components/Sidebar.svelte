<script lang="ts">
	import {
		Sidebar,
		SidebarGroup,
		SidebarItem,
		SidebarWrapper,
		SidebarBrand,
		CloseButton,
		Button
	} from 'flowbite-svelte';
	import {
		WalletSolid,
		ReceiptSolid,
		ChartMixedSolid,
		FileLinesSolid,
		PlusOutline,
		BarsSolid
	} from 'flowbite-svelte-icons';
	import {
		ButtonDarkMode,
		ButtonScrub,
		ButtonTimeZone,
		IconTelegram,
		IconExternalLink,
		logoDark,
		logoLight,
		WalletConnect,
		TransactionList,
		LocalDbStatusCard
	} from '@rainlanguage/ui-components';
	import { onMount } from 'svelte';
	import { connected, appKitModal } from '$lib/stores/wagmi';
	import { networkStatuses, raindexStatuses } from '$lib/stores/localDbStatus';
	export let colorTheme;
	export let page;

	let sideBarHidden: boolean = false;
	let breakPoint: number = 1024;
	let width: number;
	$: sideBarHidden = width < breakPoint;

	onMount(() => {
		sideBarHidden = width < breakPoint;
	});

	const toggleSide = () => {
		if (width < breakPoint) {
			sideBarHidden = !sideBarHidden;
		}
	};
</script>

<svelte:window bind:innerWidth={width} />
<div class="h-screen overflow-hidden">
	{#if sideBarHidden}
		<Button
			on:click={() => (sideBarHidden = false)}
			color="alternative"
			class="absolute left-2 top-2 flex size-8 items-center p-5 lg:hidden"
			data-testid="sidebar-bars"
			title="Open menu"
			aria-label="Open menu"
		>
			<BarsSolid class="" />
		</Button>
	{/if}
	<Sidebar
		activeUrl={page.url.pathname}
		asideClass="w-64 z-10 fixed h-screen"
		bind:hidden={sideBarHidden}
		data-testid="sidebar"
	>
		{#if !sideBarHidden}
			<CloseButton
				data-testid="close-button"
				class="absolute right-3 top-2 z-20 flex size-8 items-center border dark:border-gray-700 lg:hidden"
				title="Close menu"
				name="Close menu"
				on:click={() => (sideBarHidden = true)}
			/>
		{/if}
		<SidebarWrapper
			divClass="flex flex-col h-full pt-11 pb-4 px-3 bg-gray-100 dark:bg-gray-800 overflow-hidden"
		>
			<SidebarGroup ulClass="list-none">
				<SidebarBrand
					site={{
						name: '',
						href: '/',
						img: $colorTheme === 'light' ? logoLight : logoDark
					}}
					imgClass="m-auto px-4"
					aClass="mb-0"
				></SidebarBrand>
			</SidebarGroup>
			<SidebarGroup border ulClass="list-none">
				<SidebarItem label="Deploy" href="/deploy" data-sveltekit-reload on:click={toggleSide}>
					<svelte:fragment slot="icon">
						<PlusOutline class="h-5 w-5" />
						<span data-testid="sidebar-deploy"></span>
					</svelte:fragment>
				</SidebarItem>
			</SidebarGroup>
			<SidebarGroup border ulClass="list-none">
				<SidebarItem label="Orders" href="/orders" on:click={toggleSide}>
					<svelte:fragment slot="icon">
						<ReceiptSolid class="h-5 w-5" />
						<span data-testid="sidebar-orders"></span>
					</svelte:fragment>
				</SidebarItem>
				<SidebarItem label="Vaults" href="/vaults" on:click={toggleSide}>
					<svelte:fragment slot="icon">
						<WalletSolid class="h-5 w-5" />
						<span data-testid="sidebar-vaults"></span>
					</svelte:fragment>
				</SidebarItem>
				<SidebarItem label="Markets" href="/markets" on:click={toggleSide}>
					<svelte:fragment slot="icon">
						<ChartMixedSolid class="h-5 w-5" />
						<span data-testid="sidebar-markets"></span>
					</svelte:fragment>
				</SidebarItem>
			</SidebarGroup>
			<SidebarGroup border ulClass="list-none">
				<WalletConnect {appKitModal} {connected} classes="w-full" />
			</SidebarGroup>
			<SidebarGroup border ulClass="list-none">
				<LocalDbStatusCard networkStatuses={$networkStatuses} raindexStatuses={$raindexStatuses} />
			</SidebarGroup>
			<SidebarGroup border ulClass="list-none">
				<SidebarItem
					on:click={toggleSide}
					label="Documentation"
					target="_blank"
					rel="noopener noreferrer"
					href="https://docs.rainlang.xyz/raindex/overview"
				>
					<svelte:fragment slot="icon">
						<IconExternalLink />
						<span data-testid="sidebar-documentation"></span>
					</svelte:fragment>
				</SidebarItem>
				<SidebarItem
					on:click={toggleSide}
					label="Ask for help"
					target="_blank"
					rel="noopener noreferrer"
					href="https://t.me/+W0aQ36ptN_E2MjZk"
				>
					<svelte:fragment slot="icon">
						<IconTelegram />
						<span data-testid="sidebar-telegram"></span>
					</svelte:fragment>
				</SidebarItem>
				<SidebarItem on:click={toggleSide} label="License" href="/license">
					<svelte:fragment slot="icon">
						<FileLinesSolid />
						<span data-testid="sidebar-license"></span>
					</svelte:fragment>
				</SidebarItem>
			</SidebarGroup>
			<SidebarGroup border class="flex justify-start gap-2" ulClass="list-none">
				<ButtonDarkMode {colorTheme} />
				<ButtonScrub />
				<ButtonTimeZone />
			</SidebarGroup>
			<SidebarGroup border class="min-h-0 flex-1 overflow-hidden" ulClass="list-none h-full">
				<TransactionList />
			</SidebarGroup>
		</SidebarWrapper>
	</Sidebar>
</div>
