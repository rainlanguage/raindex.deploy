import type { Hex } from "viem";
import type { RaindexVault } from "@rainlanguage/raindex";
import type { ToastLink } from "./toast";
import type { TransactionStoreState } from "$lib/models/Transaction";

/**
 * Context provided to the indexing function, allowing it to update
 * transaction state and trigger callbacks without TransactionStore
 * knowing the specifics of what's being indexed.
 */
export type IndexingContext = {
  /** Update the transaction state (status, errorDetails, links) */
  updateState: (partialState: Partial<TransactionStoreState>) => void;
  /** Callback to invoke on successful indexing */
  onSuccess: () => void;
  /** Callback to invoke on indexing failure */
  onError: () => void;
  /** Current toast links (useful for prepending new links) */
  links: ToastLink[];
};

/**
 * Generic indexing function type. TransactionStore calls this function
 * after the transaction receipt is confirmed, without knowing what
 * type of indexing is being performed (SDK-based, subgraph, etc.).
 */
export type AwaitIndexingFn = (ctx: IndexingContext) => Promise<void>;

export type VaultActionArgs = {
  vault: RaindexVault;
  onDeposit?: () => void;
  account: Hex;
};

export enum TransactionName {
  REMOVAL = "Order Removal",
  WITHDRAWAL = "Vault Withdrawal",
  WITHDRAWAL_MULTIPLE = "Vaults Withdrawal",
  APPROVAL = "Token Approval",
  DEPOSIT = "Vault Deposit",
  TAKE_ORDER = "Take Order",
}

export enum TransactionStatusMessage {
  IDLE = "Idle",
  STARTING = "Starting transaction...",
  CHECKING_ALLOWANCE = "Checking your allowance...",
  PENDING_WALLET = "Waiting for wallet confirmation...",
  PENDING_APPROVAL = "Approving token spend...",
  PENDING_RECEIPT = "Waiting for transaction receipt...",
  PENDING_DEPLOYMENT = "Deploying your order...",
  PENDING_WITHDRAWAL = "Withdrawing tokens...",
  PENDING_DEPOSIT = "Depositing tokens...",
  PENDING_SUBGRAPH = "Awaiting transaction indexing...",
  SUCCESS = "Transaction confirmed",
  ERROR = "Something went wrong",
}

export enum TransactionStoreErrorMessage {
  SWITCH_CHAIN_FAILED = "Failed to switch chain.",
  SUBGRAPH_TIMEOUT_ERROR = "The subgraph took too long to respond. Your transaction may still be processing.",
  /** Shown under SUCCESS when indexing is slow after an on-chain receipt. */
  INDEXING_CATCHING_UP = "On-chain confirmation succeeded. Balances and history may take a moment to refresh — use the explorer link to verify now.",
  SUBGRAPH_FAILED = "Failed to index transaction.",
  RECEIPT_FAILED = "Failed to get transaction receipt.",
}

export type InternalTransactionArgs = {
  chainId: number;
  txHash: Hex;
  queryKey: string;
};

export type TransactionArgs = InternalTransactionArgs & {
  name: string;
  // Used for toast notifications upon final completion/failure
  errorMessage: string;
  successMessage: string;
  queryKey: string;
  toastLinks: ToastLink[];
  // Optional indexing function called after receipt confirmation.
  // TransactionStore doesn't know what this function does - it just calls it.
  // The function handles updating state, success/error callbacks, and links.
  awaitIndexingFn?: AwaitIndexingFn;
};
