import { writable, type Readable, type Writable } from "svelte/store";
import type { QueryClient } from "@tanstack/svelte-query";
import { TransactionStore, type Transaction } from "$lib/models/Transaction";
import type {
  InternalTransactionArgs,
  TransactionArgs,
  IndexingContext,
} from "$lib/types/transaction";
import {
  TransactionName,
  TransactionStatusMessage,
  TransactionStoreErrorMessage,
} from "$lib/types/transaction";
import type { Config } from "@wagmi/core";
import type { ToastLink, ToastProps } from "$lib/types/toast";
import { getExplorerLink } from "$lib/services/getExplorerLink";
import {
  type RaindexVault,
  type RaindexOrder,
  RaindexClient,
  type Address,
  Float,
  type WasmEncodedResult,
} from "@rainlanguage/raindex";

/**
 * Function type for adding toast notifications to the UI.
 * @param toast - The toast notification configuration object.
 * @param toast.message - The message text to display in the toast.
 * @param toast.type - The type of toast notification ('success', 'error', or 'info').
 * @param toast.color - The color theme of the toast.
 * @param toast.links - Optional array of links to display in the toast.
 */
export type AddToastFunction = (toast: Omit<ToastProps, "id">) => void;

const ADD_ORDER_INDEXING_MAX_ATTEMPTS = 12;
const ADD_ORDER_INDEXING_INTERVAL_MS = 5_000;

/**
 * Creates an indexing function that wraps SDK-based polling logic.
 * The SDK handles the retry loop internally, so we only need to call it once.
 *
 * @param options Configuration for SDK-based indexing
 * @param options.call Function that calls the SDK method (e.g. getAddOrdersForTransaction)
 * @param options.isSuccess Function to determine if the result indicates success
 * @param options.buildLinks Optional function to generate toast links from the result
 * @returns An indexing function compatible with TransactionStore
 */
export function createSdkIndexingFn<T>(options: {
  call: () => Promise<WasmEncodedResult<T>>;
  isSuccess: (value: T) => boolean;
  buildLinks?: (value: T) => ToastLink[];
}) {
  return async (ctx: IndexingContext): Promise<void> => {
    ctx.updateState({ status: TransactionStatusMessage.PENDING_SUBGRAPH });

    try {
      const result = await options.call();

      if (result.error) {
        const errorMsg = result.error.readableMsg?.toLowerCase() ?? "";
        if (errorMsg.includes("timeout")) {
          // Chain receipt already succeeded before indexing. A subgraph/LocalDB
          // timeout means app data is catching up — do not toast failure.
          ctx.updateState({
            status: TransactionStatusMessage.SUCCESS,
            errorDetails: TransactionStoreErrorMessage.INDEXING_CATCHING_UP,
          });
          return ctx.onSuccess();
        }
        ctx.updateState({
          status: TransactionStatusMessage.ERROR,
          errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
        });
        return ctx.onError();
      }

      const value = result.value;
      if (value && options.isSuccess(value)) {
        const extraLinks = options.buildLinks?.(value) ?? [];
        if (extraLinks.length > 0) {
          ctx.updateState({ links: [...extraLinks, ...ctx.links] });
        }
        ctx.updateState({ status: TransactionStatusMessage.SUCCESS });
        return ctx.onSuccess();
      }

      // No valid data after polling
      ctx.updateState({
        status: TransactionStatusMessage.ERROR,
        errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
      });
      return ctx.onError();
    } catch {
      ctx.updateState({
        status: TransactionStatusMessage.ERROR,
        errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
      });
      return ctx.onError();
    }
  };
}

/**
 * Manages blockchain transactions with toast notifications and query invalidation.
 * Handles transaction lifecycle, status updates, and UI feedback.
 * Provides functionality for creating, tracking, and managing blockchain transactions.
 */
export class TransactionManager {
  /** Writable store tracking all active transactions. */
  private transactions: Writable<Transaction[]>;
  /** Query client for cache invalidation after successful transactions. */
  private queryClient: QueryClient;
  /** Function to display toast notifications in the UI. */
  private addToast: AddToastFunction;
  /** Wagmi configuration for blockchain interactions. */
  private wagmiConfig: Config;

  /**
   * Initializes a new TransactionManager instance.
   * @param queryClient - Query client for cache invalidation.
   * @param addToast - Function to display toast notifications.
   * @param wagmiConfig - Wagmi configuration for blockchain interactions.
   */
  constructor(
    queryClient: QueryClient,
    addToast: AddToastFunction,
    wagmiConfig: Config,
  ) {
    this.queryClient = queryClient;
    this.addToast = addToast;
    this.wagmiConfig = wagmiConfig;
    this.transactions = writable<Transaction[]>([]);
  }

  /**
   * Creates and initializes a new transaction for removing an order from the raindex.
   * @param args - Configuration for the remove order transaction.
   * @param args.txHash - Hash of the transaction to track.
   * @param args.chainId - Chain ID where the transaction is being executed.
   * @param args.queryKey - The hash of the order to be removed (used for query invalidation and UI links).
   * @param args.entity - The `SgOrder` entity associated with this transaction.
   * @returns A new Transaction instance configured for order removal.
   * @example
   * const tx = await manager.createRemoveOrderTransaction({
   *   txHash: '0x123...',
   *   chainId: 1,
   *   queryKey: '0x456...', // Order hash
   *   entity: sgOrderInstance
   * });
   */
  public async createRemoveOrderTransaction(
    args: InternalTransactionArgs & {
      entity: RaindexOrder;
      raindexClient: RaindexClient;
    },
  ): Promise<Transaction> {
    const name = TransactionName.REMOVAL;
    const errorMessage = "Order removal failed.";
    const successMessage = "Order removed successfully.";
    const {
      chainId,
      entity: { raindex },
      queryKey,
      txHash,
      raindexClient,
    } = args;

    const explorerLink = await getExplorerLink(txHash, chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    const awaitIndexingFn = createSdkIndexingFn({
      call: () =>
        raindexClient.getRemoveOrdersForTransaction(chainId, raindex, txHash),
      isSuccess: (orders) => Array.isArray(orders) && orders.length > 0,
    });

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      queryKey,
      toastLinks,
      awaitIndexingFn,
    });
  }

  /**
   * Creates and initializes a new transaction for withdrawing funds from a vault.
   * @param args - Configuration for the withdrawal transaction.
   * @param args.txHash - Hash of the transaction to track.
   * @param args.chainId - Chain ID where the transaction is being executed.
   * @param args.queryKey - The ID of the vault from which funds are withdrawn (used for query invalidation and UI links).
   * @param args.entity - The `SgVault` entity associated with this transaction.
   * @returns A new Transaction instance configured for withdrawal.
   * @example
   * const tx = await manager.createWithdrawTransaction({
   *   txHash: '0x123...',
   *   chainId: 1,
   *   queryKey: '0x789...', // Vault ID
   *   entity: sgVaultInstance
   * });
   */
  public async createWithdrawTransaction(
    args: InternalTransactionArgs & {
      entity: RaindexVault;
      raindexClient: RaindexClient;
    },
  ): Promise<Transaction> {
    const name = TransactionName.WITHDRAWAL;
    const errorMessage = "Withdrawal failed.";
    const successMessage = "Withdrawal successful.";
    const {
      chainId,
      entity: { raindex },
      queryKey,
      txHash,
      raindexClient,
    } = args;

    const explorerLink = await getExplorerLink(txHash, chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: `/vaults/${chainId}-${raindex}-${queryKey}`,
        label: "View vault",
      },
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    const awaitIndexingFn = createSdkIndexingFn({
      call: () => raindexClient.getTransaction(chainId, raindex, txHash),
      isSuccess: (tx) => !!tx,
    });

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      queryKey,
      toastLinks,
      awaitIndexingFn,
    });
  }

  /**
   * Creates a multicall withdrawal transaction.
   *
   * Precondition: all provided vaults must share the same raindex.
   * This is enforced upstream in handleVaultsWithdrawAll.ts:
   *   if (vaults.some(v => v.raindex !== vaults[0].raindex)) { … }
   *
   * @param args.chainId the target chain ID
   * @param args.vaults list of RaindexVault instances (must share a raindex)
   * @param args.txHash the transaction hash to wrap
   * @param args.queryKey cache key for invalidation
   * @param args.raindexClient Raindex API client	 * @example
   * const tx = await manager.createVaultsWithdrawAllTransaction({
   *   txHash: '0x123...',
   *   chainId: 1,
   *   queryKey: 'QKEY_VAULTS',
   *   vaults: [vault1, vault2, vault3],
   *   raindexClient: clientInstance
   * });
   */
  public async createVaultsWithdrawAllTransaction(
    args: InternalTransactionArgs & {
      vaults: RaindexVault[];
      raindexClient: RaindexClient;
    },
  ): Promise<Transaction> {
    const name = TransactionName.WITHDRAWAL_MULTIPLE;
    const errorMessage = "Withdrawal failed.";
    const successMessage = "Withdrawal successful.";
    const { chainId, vaults, txHash, queryKey, raindexClient } = args;

    if (vaults.length === 0) {
      throw new Error("At least one vault is required for withdrawal");
    }
    // All vaults must share the same raindex for multicall transactions
    // It should be validated before calling this method
    const raindex = vaults[0].raindex;
    const explorerLink = await getExplorerLink(txHash, chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: "/vaults/",
        label: "View all vaults",
      },
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    const awaitIndexingFn = createSdkIndexingFn({
      call: () => raindexClient.getTransaction(chainId, raindex, txHash),
      isSuccess: (tx) => !!tx,
    });

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      toastLinks,
      queryKey,
      awaitIndexingFn,
    });
  }

  /**
   * Creates and initializes a new transaction for approving token spend.
   * @param args - Configuration for the approval transaction.
   * @param args.txHash - Hash of the transaction to track.
   * @param args.chainId - Chain ID where the transaction is being executed.
   * @param args.queryKey - The ID of the vault or context for which approval is made (used for query invalidation and UI links).
   * @param args.tokenSymbol - The symbol of the token being approved.
   * @param args.entity - The `SgVault` entity associated with this transaction. (Optional, used for approvals to pre-existing vaults).
   * @returns A new Transaction instance configured for token approval.
   * @example
   * const tx = await manager.createApprovalTransaction({
   *   txHash: '0xabc...',
   *   chainId: 1,
   *   queryKey: '0x789...', // Vault ID
   *   entity: sgVaultInstance
   * });
   */
  public async createApprovalTransaction(
    args: InternalTransactionArgs & { entity?: RaindexVault },
  ): Promise<Transaction> {
    const { entity, queryKey, chainId } = args;
    const tokenSymbol = entity?.token.symbol || "token";
    const name = `Approving ${tokenSymbol} spend`;
    const errorMessage = "Approval failed.";
    const successMessage = "Approval successful.";

    const explorerLink = await getExplorerLink(args.txHash, args.chainId, "tx");
    let toastLinks: ToastLink[] = [
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    if (entity) {
      toastLinks = [
        {
          link: `/vaults/${chainId}-${entity.raindex}-${queryKey}`,
          label: "View vault",
        },
        ...toastLinks,
      ];
    }

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      toastLinks,
    });
  }

  /**
   * Creates and initializes a new transaction for publishing metadata to the metaboard.
   * @param args - Configuration for the metadata transaction.
   * @param args.txHash - Hash of the metadata transaction.
   * @param args.chainId - Chain ID where the transaction is executed.
   * @param args.queryKey - Identifier used for query invalidation, typically the order hash.
   */
  public async createMetaTransaction(
    args: InternalTransactionArgs,
  ): Promise<Transaction> {
    const name = "Publishing metadata";
    const errorMessage = "Metadata publication failed.";
    const successMessage = "Metadata published.";

    const explorerLink = await getExplorerLink(args.txHash, args.chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      toastLinks,
    });
  }

  /**
   * Creates and initializes a new transaction for depositing funds into a vault.
   * @param args - Configuration for the deposit transaction.
   * @param args.txHash - Hash of the transaction to track.
   * @param args.chainId - Chain ID where the transaction is being executed.
   * @param args.queryKey - The ID of the vault into which funds are deposited (used for query invalidation and UI links).
   * @param args.entity - The `SgVault` entity associated with this transaction.
   * @param args.amount - The amount of tokens being deposited.
   * @returns A new Transaction instance configured for deposit.
   * @example
   * const tx = await manager.createDepositTransaction({
   *   txHash: '0xdef...',
   *   chainId: 1,
   *   queryKey: '0x789...', // Vault ID
   *   entity: sgVaultInstance,
   *   amount: 1000n
   * });
   */
  public async createDepositTransaction(
    args: InternalTransactionArgs & {
      amount: Float;
      entity: RaindexVault;
      raindexClient: RaindexClient;
    },
  ): Promise<Transaction> {
    const tokenSymbol = args.entity.token.symbol;
    const name = `Depositing ${args.amount.format().value} ${tokenSymbol}`;
    const errorMessage = "Deposit failed.";
    const successMessage = "Deposit successful.";
    const {
      chainId,
      entity: { raindex },
      txHash,
      queryKey,
      raindexClient,
    } = args;

    const explorerLink = await getExplorerLink(txHash, chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: `/vaults/${chainId}-${raindex}-${queryKey}`,
        label: "View vault",
      },
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    const awaitIndexingFn = createSdkIndexingFn({
      call: () => raindexClient.getTransaction(chainId, raindex, txHash),
      isSuccess: (tx) => !!tx,
    });

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      queryKey,
      toastLinks,
      awaitIndexingFn,
    });
  }

  /**
   * Creates and initializes a new transaction for deploying an order.
   * @param args - Configuration for the deployment transaction.
   * @param args.txHash - Hash of the transaction to track.
   * @param args.chainId - Chain ID where the transaction is being executed.
   * @param args.queryKey - The ID of the vault into which funds are deposited (used for query invalidation and UI links).
   * @returns A new Transaction instance configured for deposit.
   * @example
   * const tx = await manager.createAddOrderTransaction({
   *   txHash: '0xdeploytxhash',
   *   chainId: 1,
   *   queryKey: '0x789...', // Vault ID
   * });
   */

  public async createAddOrderTransaction(
    args: InternalTransactionArgs & {
      raindex: Address;
      raindexClient: RaindexClient;
    },
  ): Promise<Transaction> {
    const { queryKey, txHash, chainId, raindex, raindexClient } = args;
    const name = "Deploying order";
    const errorMessage = "Deployment failed.";
    const successMessage = "Order deployed successfully.";

    const explorerLink = await getExplorerLink(txHash, chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];

    // SDK-based indexing - getAddOrdersForTransaction handles the retry loop.
    const awaitIndexingFn = createSdkIndexingFn({
      call: () =>
        raindexClient.getAddOrdersForTransaction(
          chainId,
          raindex,
          txHash,
          ADD_ORDER_INDEXING_MAX_ATTEMPTS,
          ADD_ORDER_INDEXING_INTERVAL_MS,
        ),
      isSuccess: (orders) => Array.isArray(orders) && orders.length > 0,
      buildLinks: (orders) => {
        if (!Array.isArray(orders) || orders.length === 0) return [];
        const firstOrder = orders[0];
        if (!firstOrder?.orderHash) return [];
        return [
          {
            link: `/orders/${chainId}-${raindex}-${firstOrder.orderHash}`,
            label: "View order",
          },
        ];
      },
    });

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      queryKey,
      toastLinks,
      awaitIndexingFn,
    });
  }

  /**
   * Creates and initializes a new transaction for taking orders.
   * @param args - Configuration for the take order transaction.
   * @param args.txHash - Hash of the transaction to track.
   * @param args.chainId - Chain ID where the transaction is being executed.
   * @param args.queryKey - The hash of the order being taken (used for query invalidation).
   * @param args.entity - The `RaindexOrder` entity associated with this transaction.
   * @returns A new Transaction instance configured for taking orders.
   * @example
   * const tx = await manager.createTakeOrderTransaction({
   *   txHash: '0x123...',
   *   chainId: 1,
   *   queryKey: '0x456...', // Order hash
   *   entity: raindexOrderInstance,
   *   raindexClient: clientInstance
   * });
   */
  public async createTakeOrderTransaction(
    args: InternalTransactionArgs & {
      entity: RaindexOrder;
      raindexClient: RaindexClient;
    },
  ): Promise<Transaction> {
    const name = TransactionName.TAKE_ORDER;
    const errorMessage = "Take order failed.";
    const successMessage = "Order taken successfully.";
    const {
      chainId,
      entity: { raindex },
      queryKey,
      txHash,
      raindexClient,
    } = args;

    const explorerLink = await getExplorerLink(txHash, chainId, "tx");
    const toastLinks: ToastLink[] = [
      {
        link: explorerLink,
        label: "View on explorer",
      },
    ];
    const awaitIndexingFn = createSdkIndexingFn({
      call: () => raindexClient.getTransaction(chainId, raindex, txHash),
      isSuccess: (tx) => !!tx,
    });

    return this.createTransaction({
      ...args,
      name,
      errorMessage,
      successMessage,
      queryKey,
      toastLinks,
      awaitIndexingFn,
    });
  }

  /**
   * Creates, initializes, and executes a new transaction instance.
   * @param args - Configuration for the transaction.
   * @param args.name - Name or title of the transaction.
   * @param args.errorMessage - Message to display on transaction failure.
   * @param args.successMessage - Message to display on transaction success.
   * @param args.queryKey - Key used for query invalidation.
   * @param args.toastLinks - Array of links to display in toast notifications.
   * @param args.awaitIndexingFn - Optional function to await transaction indexing.
   * @returns A new Transaction instance that has been initialized and started.
   * @private
   */
  private async createTransaction(args: TransactionArgs): Promise<Transaction> {
    const createTransactionArgs: TransactionArgs & { config: Config } = {
      ...args,
      config: this.wagmiConfig,
    };

    const onSuccess = () => {
      if (args.queryKey) {
        this.queryClient.invalidateQueries({ queryKey: [args.queryKey] });
      }
    };

    const onError = () => {
      this.addToast({
        message: args.errorMessage,
        type: "error",
        color: "red",
        links: args.toastLinks,
      });
    };

    const transactionInstance = new TransactionStore(
      createTransactionArgs,
      onSuccess,
      onError,
    );

    this.transactions.update((currentTransactions) => [
      ...currentTransactions,
      transactionInstance,
    ]);
    await transactionInstance.execute();
    return transactionInstance;
  }

  /**
   * Retrieves the store containing all active transactions.
   * @returns A readable store containing all active Transaction instances.
   * @example
   * const transactionsStore = manager.getTransactions();
   * transactionsStore.subscribe(transactions => console.log(transactions));
   */
  public getTransactions(): Readable<Transaction[]> {
    return this.transactions;
  }

  /**
   * Removes all transactions from the store, resetting the transaction tracking state.
   * @example
   * manager.clearTransactions(); // Clear all tracked transactions
   */
  public clearTransactions(): void {
    this.transactions.set([]);
  }
}
