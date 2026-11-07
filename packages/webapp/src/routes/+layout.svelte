<script lang="ts">
	import '../app.css';
	import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { colorTheme } from '$lib/darkMode';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import Homepage from '$lib/components/Homepage.svelte';
	import LoadingWrapper from '$lib/components/LoadingWrapper.svelte';
	import {
		ToastProvider,
		WalletProvider,
		FixedBottomTransaction,
		RaindexClientProvider,
		LocalDbProvider,
		DotrainRegistryProvider,
		RegistryManager
	} from '@rainlanguage/ui-components';
	import { signerAddress } from '$lib/stores/wagmi';
	import { validChainIds } from '$lib/stores/settings';
	import ErrorPage from '$lib/components/ErrorPage.svelte';
	import TransactionProviderWrapper from '$lib/components/TransactionProviderWrapper.svelte';
	import { initWallet } from '$lib/services/handleWalletInitialization';
	import { REGISTRY_URL } from '$lib/constants';
	import { onMount } from 'svelte';
	import type { RaindexClient } from '@rainlanguage/raindex';
	import { seedLocalDbSyncSnapshot } from '$lib/stores/localDbStatus';

	const { errorMessage, registryWarning, localDb, raindexClient, registry } = $page.data;
	const registryManager = new RegistryManager(REGISTRY_URL);

	const queryClient = new QueryClient({
		defaultOptions: {
			queries: {
				staleTime: Infinity
			}
		}
	});

	let walletInitError: string | null = null;

	onMount(() => {
		if (!browser || !raindexClient || !registry) return;
		const client = raindexClient as RaindexClient;

		const uniqueChainIds = client.getUniqueChainIds();
		if (!uniqueChainIds.error) {
			validChainIds.set(uniqueChainIds.value);
		}

		client
			.getLocalDbSyncSnapshot()
			.then((snapshotResult) => {
				if (!snapshotResult.error) {
					seedLocalDbSyncSnapshot(snapshotResult.value);
				}
			})
			.catch(() => {
				// The live status callback will continue to update the sidebar and data gate.
			});
	});

	$: if (browser && window.navigator) {
		initWallet().then((error) => {
			walletInitError = error;
		});
	}
</script>

{#if walletInitError}
	<div
		class="fixed bottom-4 left-1/2 z-[100] -translate-x-1/2 transform rounded-lg bg-red-500 px-6 py-3 text-white shadow-md"
	>
		{walletInitError}
	</div>
{/if}

{#if registryWarning}
	<div
		data-testid="registry-warning"
		class="fixed bottom-4 left-1/2 z-[100] -translate-x-1/2 transform rounded-lg bg-yellow-500 px-6 py-3 text-white shadow-md"
	>
		{registryWarning}
	</div>
{/if}

<ToastProvider>
	<WalletProvider account={signerAddress}>
		<QueryClientProvider client={queryClient}>
			<TransactionProviderWrapper>
				<LoadingWrapper>
					{#if $page.url.pathname === '/'}
						<Homepage {colorTheme} />
					{:else if errorMessage}
						<ErrorPage />
					{:else}
						<DotrainRegistryProvider {registry} error={errorMessage} manager={registryManager}>
							<LocalDbProvider {localDb}>
								<RaindexClientProvider {raindexClient}>
									<div
										data-testid="layout-container"
										class="flex min-h-screen w-full justify-start bg-white dark:bg-gray-900 dark:text-gray-400"
									>
										<Sidebar {colorTheme} page={$page} />
										<main
											class="mx-auto h-full w-full grow overflow-x-auto px-4 pt-14 lg:ml-64 lg:p-8"
										>
											<slot />
										</main>
									</div>
								</RaindexClientProvider>
							</LocalDbProvider>
						</DotrainRegistryProvider>
					{/if}
					<FixedBottomTransaction />
				</LoadingWrapper>
			</TransactionProviderWrapper>
		</QueryClientProvider>
	</WalletProvider>
</ToastProvider>
