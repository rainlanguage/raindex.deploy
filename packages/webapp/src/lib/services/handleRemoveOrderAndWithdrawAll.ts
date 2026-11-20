import type {
  RaindexClient,
  RaindexOrder,
  RaindexVaultsList,
} from "@rainlanguage/raindex";
import { encodeFunctionData, type Hex } from "viem";
import type {
  TransactionManager,
  TransactionConfirmationProps,
} from "@rainlanguage/ui-components";
import type { TransactionConfirmationModalResult } from "./modal";

/**
 * ABI fragment for the Raindex `multicall(bytes[])` function (inherited from
 * OpenZeppelin `Multicall`). Each entry is delegatecalled into the contract, so
 * a removeOrder call and the per-vault withdraw calls can be batched into a
 * single transaction.
 */
const MULTICALL_ABI = [
  {
    type: "function",
    name: "multicall",
    inputs: [{ name: "data", type: "bytes[]" }],
    outputs: [{ name: "results", type: "bytes[]" }],
    stateMutability: "nonpayable",
  },
] as const;

export interface HandleRemoveOrderAndWithdrawAllDependencies {
  raindexClient: RaindexClient;
  order: RaindexOrder;
  vaultsList: RaindexVaultsList;
  handleTransactionConfirmationModal: (
    props: TransactionConfirmationProps,
  ) => Promise<TransactionConfirmationModalResult>;
  errToast: (message: string) => void;
  manager: TransactionManager;
}

/**
 * Removes an order and withdraws all of its withdrawable vault balances in a
 * single transaction.
 *
 * Mirrors `handleVaultsWithdrawAll`, but the generated calldata is a
 * `multicall([removeOrder, ...withdraws])` so that the order removal and the
 * vault withdrawals execute atomically. The withdraw calldata is sourced from
 * `vaultsList.getWithdrawCalldata()` (which itself only includes vaults with a
 * positive balance), so an order whose vaults are all empty is still removed.
 */
export async function handleRemoveOrderAndWithdrawAll(
  deps: HandleRemoveOrderAndWithdrawAllDependencies,
): Promise<void> {
  const {
    raindexClient,
    order,
    vaultsList,
    handleTransactionConfirmationModal,
    errToast,
    manager,
  } = deps;

  try {
    const removeCalldataResult = order.getRemoveCalldata();
    if (removeCalldataResult.error) {
      return errToast(removeCalldataResult.error.readableMsg);
    }
    const removeCalldata = removeCalldataResult.value as Hex;

    const calls: Hex[] = [removeCalldata];

    // Only attempt to build withdraw calldata when there is something to
    // withdraw. Empty vaults must not block the order removal.
    const withdrawableResult = vaultsList.getWithdrawableVaults();
    if (withdrawableResult.error) {
      return errToast(
        `Failed to get withdrawable vaults: ${withdrawableResult.error.readableMsg}`,
      );
    }
    if (withdrawableResult.value.length > 0) {
      const withdrawCalldataResult = await vaultsList.getWithdrawCalldata();
      if (withdrawCalldataResult.error) {
        return errToast(
          `Failed to generate withdraw calldata: ${withdrawCalldataResult.error.readableMsg}`,
        );
      }
      calls.push(withdrawCalldataResult.value as Hex);
    }

    const calldata = encodeFunctionData({
      abi: MULTICALL_ABI,
      functionName: "multicall",
      args: [calls],
    });

    await handleTransactionConfirmationModal({
      open: true,
      modalTitle: "Removing order and withdrawing all vaults",
      args: {
        entity: order,
        toAddress: order.raindex,
        chainId: order.chainId,
        calldata,
        onConfirm: (txHash: Hex) => {
          manager.createRemoveOrderTransaction({
            raindexClient,
            txHash,
            queryKey: order.orderHash,
            chainId: order.chainId,
            entity: order,
          });
        },
      },
    });
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : "Unknown error";
    errToast(`Failed to remove order and withdraw all vaults: ${errorMsg}`);
  }
}
