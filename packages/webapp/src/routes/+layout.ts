import { DotrainRegistry, type RaindexClient, type Address, type Hex } from '@rainlanguage/raindex';
import init, { SQLiteWasmDatabase } from '@rainlanguage/sqlite-web';
import type { AppStoresInterface } from '@rainlanguage/ui-components';
import { REGISTRY_URL } from '$lib/constants';
import { updateStatus } from '$lib/stores/localDbStatus';
import { writable } from 'svelte/store';
import type { LayoutLoad } from './$types';

export interface LayoutData {
	errorMessage?: string;
	registryWarning?: string;
	stores: AppStoresInterface | null;
	raindexClient: RaindexClient | null;
	registry: DotrainRegistry | null;
	localDb: SQLiteWasmDatabase | null;
}

/** Remove the persisted custom registry from localStorage and the URL param. */
const clearCustomRegistry = (url: URL): void => {
	if (typeof localStorage !== 'undefined') {
		try {
			localStorage.removeItem('registry');
		} catch {
			// ignore removal failure
		}
	}
	if (typeof window !== 'undefined') {
		try {
			const next = new URL(window.location.href);
			next.searchParams.delete('registry');
			window.history.replaceState({}, '', next.toString());
		} catch {
			// ignore URL update failure
		}
	}
	url.searchParams.delete('registry');
};

/** Build a DotrainRegistry, surfacing a readable message on failure. */
const buildRegistry = async (
	registryUrl: string
): Promise<{ registry: DotrainRegistry | null; error?: string }> => {
	try {
		const registryResult = await DotrainRegistry.new(registryUrl);
		if (registryResult.error) {
			return {
				registry: null,
				error: 'Failed to load registry. ' + registryResult.error.readableMsg
			};
		}
		return { registry: registryResult.value };
	} catch (error: unknown) {
		return { registry: null, error: 'Failed to load registry. ' + (error as Error).message };
	}
};

export const load: LayoutLoad<LayoutData> = async ({ url }) => {
	let errorMessage: string | undefined;
	let registryWarning: string | undefined;

	const registryParam = url.searchParams.get('registry');
	let registryUrl = REGISTRY_URL;

	if (registryParam) {
		registryUrl = registryParam;
		if (typeof localStorage !== 'undefined') {
			try {
				localStorage.setItem('registry', registryParam);
			} catch {
				// ignore persistence failure
			}
		}
	} else {
		if (typeof localStorage !== 'undefined') {
			try {
				registryUrl = localStorage.getItem('registry') || REGISTRY_URL;
			} catch {
				registryUrl = REGISTRY_URL;
			}
		}
	}

	let registry: DotrainRegistry | null = null;
	{
		const result = await buildRegistry(registryUrl);
		registry = result.registry;
		if (result.error) {
			if (registryUrl !== REGISTRY_URL) {
				// A custom registry failed to load. Clear it so the next load uses the
				// default, then retry the default in this load so the app still mounts.
				clearCustomRegistry(url);
				const fallback = await buildRegistry(REGISTRY_URL);
				registry = fallback.registry;
				if (fallback.error) {
					errorMessage = fallback.error;
				} else {
					registryWarning =
						'The custom registry failed to load and has been reset to the default registry.';
				}
			} else {
				errorMessage = result.error;
			}
		}
	}

	let localDb: SQLiteWasmDatabase | null = null;
	if (!errorMessage) {
		try {
			await init();
			const localDbRes = await SQLiteWasmDatabase.new('worker.db');
			if (localDbRes.error) {
				errorMessage = 'Error initializing local database: ' + localDbRes.error.readableMsg;
			} else {
				localDb = localDbRes.value;
			}
		} catch (error: unknown) {
			errorMessage = 'Error initializing local database: ' + (error as Error).message;
		}
	}

	let raindexClient: RaindexClient | null = null;
	try {
		if (!errorMessage && registry) {
			const raindexClientRes = await registry.getRaindexClient(
				localDb
					? {
							localDb,
							statusCallback: updateStatus
						}
					: undefined
			);
			if (raindexClientRes.error) {
				errorMessage = raindexClientRes.error.readableMsg;
			} else {
				raindexClient = raindexClientRes.value;
			}
		}
	} catch (error: unknown) {
		errorMessage = 'Error initializing RaindexClient: ' + (error as Error).message;
	}

	if (errorMessage) {
		return {
			errorMessage,
			stores: null,
			registry,
			localDb,
			raindexClient: null
		};
	}

	return {
		registryWarning,
		stores: {
			selectedChainIds: writable<number[]>([]),
			showInactiveOrders: writable<boolean>(false),
			// @ts-expect-error initially the value is empty
			orderHash: writable<Hex>(''),
			hideZeroBalanceVaults: writable<boolean>(false),
			hideInactiveOrdersVaults: writable<boolean>(false),
			activeTokens: writable<Address[]>([]),
			activeRaindexAddresses: writable<Address[]>([]),
			// @ts-expect-error initially the value is empty
			ownerFilter: writable<Address>('')
		},
		registry,
		localDb,
		raindexClient
	};
};

export const ssr = false;

if (import.meta.vitest) {
	const { describe, it, expect, beforeEach, vi } = import.meta.vitest;

	const { mockRegistryNew, mockGetRaindexClient, mockInit, mockLocalDbNew } = vi.hoisted(() => ({
		mockRegistryNew: vi.fn(),
		mockGetRaindexClient: vi.fn(),
		mockInit: vi.fn(),
		mockLocalDbNew: vi.fn()
	}));

	vi.mock('@rainlanguage/raindex', async (importOriginal) => {
		const original = (await importOriginal()) as Record<string, unknown>;
		return {
			...original,
			DotrainRegistry: {
				new: mockRegistryNew
			}
		};
	});

	vi.mock('@rainlanguage/sqlite-web', () => ({
		default: mockInit,
		SQLiteWasmDatabase: {
			new: mockLocalDbNew
		}
	}));

	describe('Layout load function', () => {
		beforeEach(() => {
			vi.clearAllMocks();
			// @ts-expect-error mock storage
			global.localStorage = {
				data: {} as Record<string, string>,
				getItem(key: string) {
					return this.data[key] ?? null;
				},
				setItem(key: string, value: string) {
					this.data[key] = value;
				},
				removeItem(key: string) {
					delete this.data[key];
				}
			};
			mockInit.mockResolvedValue(undefined);
			mockLocalDbNew.mockReturnValue({
				value: { db: true, query: vi.fn(), wipeAndRecreate: vi.fn(), transaction: vi.fn() }
			});
		});

		it('should return errorMessage if registry fails to load', async () => {
			mockRegistryNew.mockRejectedValueOnce(new Error('Network error'));

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(result).toHaveProperty('stores', null);
			expect(result.errorMessage).toContain('Failed to load registry');
		});

		it('should return errorMessage if RaindexClient fails to initialize', async () => {
			mockGetRaindexClient.mockResolvedValue({
				error: { readableMsg: 'Malformed settings' }
			});
			const mockRegistry = { getRaindexClient: mockGetRaindexClient };
			mockRegistryNew.mockResolvedValueOnce({
				value: mockRegistry
			});

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(result).toHaveProperty('stores', null);
			expect(result.errorMessage).toContain('Malformed settings');
		});

		it('should return errorMessage if local database fails to initialize', async () => {
			const mockRegistry = { getRaindexClient: mockGetRaindexClient };
			mockRegistryNew.mockResolvedValueOnce({
				value: mockRegistry
			});
			mockLocalDbNew.mockReturnValue({
				error: { readableMsg: 'Database init failed' }
			});

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(result).toHaveProperty('stores', null);
			expect(result.errorMessage).toContain('Error initializing local database');
		});

		it('should initialize when registry and RaindexClient succeed', async () => {
			mockGetRaindexClient.mockResolvedValue({
				value: { client: true }
			});
			const mockRegistry = { getRaindexClient: mockGetRaindexClient };
			mockRegistryNew.mockResolvedValueOnce({
				value: mockRegistry
			});
			const localDb = { db: true, query: vi.fn(), wipeAndRecreate: vi.fn(), transaction: vi.fn() };
			mockLocalDbNew.mockReturnValue({
				value: localDb
			});

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(result.errorMessage).toBeUndefined();
			expect(result.stores).not.toBeNull();
			expect(result.registry).toEqual(mockRegistry);
			expect(mockGetRaindexClient).toHaveBeenCalledWith({
				localDb,
				statusCallback: updateStatus
			});
		});

		it('should reset a failing custom registry from localStorage, retry the default, and warn', async () => {
			localStorage.setItem('registry', 'https://custom.example/registry');
			mockGetRaindexClient.mockResolvedValue({ value: { client: true } });
			const defaultRegistry = { getRaindexClient: mockGetRaindexClient };
			mockRegistryNew
				.mockRejectedValueOnce(new Error('Network error'))
				.mockResolvedValueOnce({ value: defaultRegistry });

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(mockRegistryNew).toHaveBeenNthCalledWith(1, 'https://custom.example/registry');
			expect(mockRegistryNew).toHaveBeenNthCalledWith(2, REGISTRY_URL);
			expect(localStorage.getItem('registry')).toBeNull();
			expect(result.errorMessage).toBeUndefined();
			expect(result.registryWarning).toContain('custom registry');
			expect(result.stores).not.toBeNull();
			expect(result.registry).toEqual(defaultRegistry);
		});

		it('should reset a failing custom registry from the ?registry= param, retry the default, and warn', async () => {
			mockGetRaindexClient.mockResolvedValue({ value: { client: true } });
			const defaultRegistry = { getRaindexClient: mockGetRaindexClient };
			mockRegistryNew
				.mockRejectedValueOnce(new Error('Network error'))
				.mockResolvedValueOnce({ value: defaultRegistry });

			const result = await load({
				url: new URL('http://localhost:3000?registry=https://custom.example/registry')
				// eslint-disable-next-line @typescript-eslint/no-explicit-any
			} as any);

			expect(mockRegistryNew).toHaveBeenNthCalledWith(1, 'https://custom.example/registry');
			expect(mockRegistryNew).toHaveBeenNthCalledWith(2, REGISTRY_URL);
			expect(localStorage.getItem('registry')).toBeNull();
			expect(result.errorMessage).toBeUndefined();
			expect(result.registryWarning).toContain('custom registry');
			expect(result.registry).toEqual(defaultRegistry);
		});

		it('should stay fatal when a failing custom registry resets but the default also fails', async () => {
			localStorage.setItem('registry', 'https://custom.example/registry');
			mockRegistryNew
				.mockRejectedValueOnce(new Error('Custom network error'))
				.mockRejectedValueOnce(new Error('Default network error'));

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(mockRegistryNew).toHaveBeenNthCalledWith(1, 'https://custom.example/registry');
			expect(mockRegistryNew).toHaveBeenNthCalledWith(2, REGISTRY_URL);
			expect(localStorage.getItem('registry')).toBeNull();
			expect(result.registryWarning).toBeUndefined();
			expect(result).toHaveProperty('stores', null);
			expect(result.errorMessage).toContain('Failed to load registry');
		});

		it('should stay fatal without resetting when the default registry fails', async () => {
			mockRegistryNew.mockRejectedValueOnce(new Error('Network error'));

			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const result = await load({ url: new URL('http://localhost:3000') } as any);

			expect(mockRegistryNew).toHaveBeenCalledTimes(1);
			expect(mockRegistryNew).toHaveBeenCalledWith(REGISTRY_URL);
			expect(result.registryWarning).toBeUndefined();
			expect(result).toHaveProperty('stores', null);
			expect(result.errorMessage).toContain('Failed to load registry');
		});
	});
}
