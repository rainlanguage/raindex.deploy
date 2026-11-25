import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  handleRemoveOrderAndWithdrawAll,
  type HandleRemoveOrderAndWithdrawAllDependencies,
} from "../lib/services/handleRemoveOrderAndWithdrawAll";
import type {
  RaindexClient,
  RaindexOrder,
  RaindexVaultsList,
} from "@rainlanguage/raindex";
import type { Hex } from "viem";
import { decodeFunctionData } from "viem";
import type { TransactionManager } from "@rainlanguage/ui-components";

const MULTICALL_ABI = [
  {
    type: "function",
    name: "multicall",
    inputs: [{ name: "data", type: "bytes[]" }],
    outputs: [{ name: "results", type: "bytes[]" }],
    stateMutability: "nonpayable",
  },
] as const;

const REMOVE_CALLDATA = "0xdeadbeef" as Hex;
const WITHDRAW_CALLDATA = "0xcafe" as Hex;

// Pre-computed via viem: multicall([REMOVE_CALLDATA, WITHDRAW_CALLDATA]).
const EXPECTED_BOTH =
  "0xac9650d800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000004deadbeef000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002cafe000000000000000000000000000000000000000000000000000000000000";
// Pre-computed via viem: multicall([REMOVE_CALLDATA]).
const EXPECTED_REMOVE_ONLY =
  "0xac9650d80000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000004deadbeef00000000000000000000000000000000000000000000000000000000";

const mockHandleTransactionConfirmationModal = vi.fn();
const mockErrToast = vi.fn();
const mockCreateRemoveOrderTransaction = vi.fn();

const mockManager = {
  createRemoveOrderTransaction: mockCreateRemoveOrderTransaction,
} as unknown as TransactionManager;

const mockRaindexClient = {} as unknown as RaindexClient;

const orderHash = "0xorderhash" as Hex;
const raindex = "0xraindexaddress" as Hex;
const chainId = 137;

const makeOrder = (overrides: Partial<RaindexOrder> = {}) =>
  ({
    orderHash,
    raindex,
    chainId,
    getRemoveCalldata: vi
      .fn()
      .mockReturnValue({ value: REMOVE_CALLDATA, error: undefined }),
    ...overrides,
  }) as unknown as RaindexOrder;

const makeVaultsList = (overrides: Partial<RaindexVaultsList> = {}) =>
  ({
    getWithdrawableVaults: vi
      .fn()
      .mockReturnValue({ value: [{ id: "0xvault" }], error: undefined }),
    getWithdrawCalldata: vi
      .fn()
      .mockResolvedValue({ value: WITHDRAW_CALLDATA, error: undefined }),
    ...overrides,
  }) as unknown as RaindexVaultsList;

const makeDeps = (
  order: RaindexOrder,
  vaultsList: RaindexVaultsList,
): HandleRemoveOrderAndWithdrawAllDependencies => ({
  raindexClient: mockRaindexClient,
  order,
  vaultsList,
  handleTransactionConfirmationModal: mockHandleTransactionConfirmationModal,
  errToast: mockErrToast,
  manager: mockManager,
});

describe("handleRemoveOrderAndWithdrawAll", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockHandleTransactionConfirmationModal.mockResolvedValue({ success: true });
  });

  it("batches removeOrder + withdraw into a single multicall in that exact order", async () => {
    const order = makeOrder();
    const vaultsList = makeVaultsList();

    await handleRemoveOrderAndWithdrawAll(makeDeps(order, vaultsList));

    expect(mockHandleTransactionConfirmationModal).toHaveBeenCalledOnce();
    const args = mockHandleTransactionConfirmationModal.mock.calls[0][0].args;

    // The calldata is exactly the multicall of removeOrder then withdraw,
    // in that order, under the multicall selector.
    expect(args.calldata).toBe(EXPECTED_BOTH);

    // Decode to prove it is a flat multicall containing the two calls in order.
    const decoded = decodeFunctionData({
      abi: MULTICALL_ABI,
      data: args.calldata,
    });
    expect(decoded.functionName).toBe("multicall");
    expect(decoded.args[0]).toEqual([REMOVE_CALLDATA, WITHDRAW_CALLDATA]);

    expect(args.toAddress).toBe(raindex);
    expect(args.chainId).toBe(chainId);
    expect(args.entity).toBe(order);
    expect(mockErrToast).not.toHaveBeenCalled();
  });

  it("removes the order alone (no withdraw call) when there are no withdrawable vaults", async () => {
    const order = makeOrder();
    const vaultsList = makeVaultsList({
      getWithdrawableVaults: vi
        .fn()
        .mockReturnValue({ value: [], error: undefined }),
    });

    await handleRemoveOrderAndWithdrawAll(makeDeps(order, vaultsList));

    const args = mockHandleTransactionConfirmationModal.mock.calls[0][0].args;
    expect(args.calldata).toBe(EXPECTED_REMOVE_ONLY);

    const decoded = decodeFunctionData({
      abi: MULTICALL_ABI,
      data: args.calldata,
    });
    expect(decoded.args[0]).toEqual([REMOVE_CALLDATA]);

    // Must not even ask for withdraw calldata when nothing is withdrawable.
    expect(vaultsList.getWithdrawCalldata).not.toHaveBeenCalled();
    expect(mockErrToast).not.toHaveBeenCalled();
  });

  it("uses the combined-action modal title", async () => {
    await handleRemoveOrderAndWithdrawAll(
      makeDeps(makeOrder(), makeVaultsList()),
    );

    expect(mockHandleTransactionConfirmationModal).toHaveBeenCalledWith(
      expect.objectContaining({
        modalTitle: "Removing order and withdrawing all vaults",
      }),
    );
  });

  it("creates a remove-order transaction on confirmation", async () => {
    const order = makeOrder();
    const txHash = "0xtxhash" as Hex;

    await handleRemoveOrderAndWithdrawAll(makeDeps(order, makeVaultsList()));

    const onConfirm =
      mockHandleTransactionConfirmationModal.mock.calls[0][0].args.onConfirm;
    onConfirm(txHash);

    expect(mockCreateRemoveOrderTransaction).toHaveBeenCalledWith({
      raindexClient: mockRaindexClient,
      txHash,
      queryKey: orderHash,
      chainId,
      entity: order,
    });
  });

  it("toasts the remove-calldata error and never opens the modal", async () => {
    const order = makeOrder({
      getRemoveCalldata: vi.fn().mockReturnValue({
        value: undefined,
        error: { msg: "remove failed", readableMsg: "Remove calldata failed" },
      }),
    });

    await handleRemoveOrderAndWithdrawAll(makeDeps(order, makeVaultsList()));

    expect(mockErrToast).toHaveBeenCalledWith("Remove calldata failed");
    expect(mockHandleTransactionConfirmationModal).not.toHaveBeenCalled();
  });

  it("toasts the withdraw-calldata error and never opens the modal", async () => {
    const vaultsList = makeVaultsList({
      getWithdrawCalldata: vi.fn().mockResolvedValue({
        value: undefined,
        error: {
          msg: "withdraw failed",
          readableMsg: "Withdraw calldata failed",
        },
      }),
    });

    await handleRemoveOrderAndWithdrawAll(makeDeps(makeOrder(), vaultsList));

    expect(mockErrToast).toHaveBeenCalledWith(
      "Failed to generate withdraw calldata: Withdraw calldata failed",
    );
    expect(mockHandleTransactionConfirmationModal).not.toHaveBeenCalled();
  });

  it("toasts the withdrawable-vaults lookup error and never opens the modal", async () => {
    const vaultsList = makeVaultsList({
      getWithdrawableVaults: vi.fn().mockReturnValue({
        value: undefined,
        error: { msg: "lookup failed", readableMsg: "Lookup failed" },
      }),
    });

    await handleRemoveOrderAndWithdrawAll(makeDeps(makeOrder(), vaultsList));

    expect(mockErrToast).toHaveBeenCalledWith(
      "Failed to get withdrawable vaults: Lookup failed",
    );
    expect(mockHandleTransactionConfirmationModal).not.toHaveBeenCalled();
  });
});
