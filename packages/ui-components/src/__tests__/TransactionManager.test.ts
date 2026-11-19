import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  TransactionManager,
  createSdkIndexingFn,
} from "../lib/providers/transactions/TransactionManager";
import { TransactionStore } from "../lib/models/Transaction";
import type { QueryClient } from "@tanstack/svelte-query";
import type { Config } from "@wagmi/core";
import type { ToastProps } from "../lib/types/toast";
import {
  TransactionName,
  TransactionStatusMessage,
  TransactionStoreErrorMessage,
  type InternalTransactionArgs,
  type IndexingContext,
} from "../lib/types/transaction";
import { getExplorerLink } from "../lib/services/getExplorerLink";
import {
  Float,
  RaindexClient,
  RaindexOrder,
  RaindexVault,
  type Address,
  type WasmEncodedResult,
} from "@rainlanguage/raindex";

vi.mock("../lib/models/Transaction", () => ({
  TransactionStore: vi.fn(),
}));

vi.mock("../lib/services/getExplorerLink", () => ({
  getExplorerLink: vi.fn(),
}));

vi.mock("@rainlanguage/raindex", async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return {
    ...actual,
    // Local stub builds may omit Float; keep deposit fixture setup working.
    Float: actual.Float ?? {
      parse: (value: string) => ({
        value: {
          format: () => ({ value }),
          toString: () => value,
        },
        error: undefined,
      }),
    },
    getTransactionRemoveOrders: vi.fn(),
    getTransaction: vi.fn(),
    getTransactionAddOrders: vi.fn(),
  };
});

describe("TransactionManager", () => {
  let mockQueryClient: QueryClient;
  let mockAddToast: (toast: Omit<ToastProps, "id">) => void;
  let mockWagmiConfig: Config;
  let manager: TransactionManager;

  const mockRaindexClient = {
    getRemoveOrdersForTransaction: vi.fn(),
    getTransaction: vi.fn(),
    getAddOrdersForTransaction: vi.fn(),
  } as unknown as RaindexClient;

  const mockSgOrderEntity = {
    id: "mockOrderEntityId",
    raindex: "mockRaindex",
    getRemoveOrdersForTransaction: vi.fn(),
  } as unknown as RaindexOrder;

  const mockSgVaultEntity = {
    token: { symbol: "MOCKVAULT", decimals: "18" },
    vaultId: "mockVaultEntityId",
    id: "mockVaultEntityId",
  } as unknown as RaindexVault;

  const mockBaseArgs: InternalTransactionArgs = {
    txHash: "0xcallbacktxhash" as `0x${string}`,
    chainId: 1,
    queryKey: "0xcallbackkey",
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockQueryClient = {
      invalidateQueries: vi.fn(),
    } as unknown as QueryClient;

    mockAddToast = vi.fn();

    mockWagmiConfig = {} as Config;
    manager = new TransactionManager(
      mockQueryClient,
      mockAddToast,
      mockWagmiConfig,
    );
  });

  describe("initialization", () => {
    it("should initialize with empty transactions store", () => {
      const transactions = manager.getTransactions();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let storeValue: any[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toEqual([]);
    });
  });

  describe("createRemoveOrderTransaction", () => {
    const removeOrderMockArgs: InternalTransactionArgs & {
      raindexClient: RaindexClient;
      entity: RaindexOrder;
    } = {
      txHash:
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef" as `0x${string}`,
      chainId: 1,
      queryKey:
        "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
      entity: mockSgOrderEntity,
      raindexClient: mockRaindexClient,
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
      );
    });

    it("should create a transaction with correct parameters and awaitIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      expect(TransactionStore).toHaveBeenCalledWith(
        expect.objectContaining({
          name: TransactionName.REMOVAL,
          errorMessage: "Order removal failed.",
          successMessage: "Order removed successfully.",
          queryKey: removeOrderMockArgs.queryKey,
          toastLinks: [
            {
              link: "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
          awaitIndexingFn: expect.any(Function),
        }),
        expect.any(Function),
        expect.any(Function),
      );

      // Verify awaitIndexingFn is a function
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(typeof callArgs.awaitIndexingFn).toBe("function");
    });

    it("should execute the transaction after creation", async () => {
      const mockExecute = vi.fn();
      const mockTransaction = { execute: mockExecute };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      expect(mockExecute).toHaveBeenCalled();
    });

    it("should add transaction to store", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      const transactions = manager.getTransactions();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let storeValue: any[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toContain(mockTransaction);
    });

    it("should handle successful transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onSuccess: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success) => {
        onSuccess = success;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      onSuccess!();

      expect(mockQueryClient.invalidateQueries).toHaveBeenCalledWith({
        queryKey: [removeOrderMockArgs.queryKey],
      });
    });

    it("should handle failed transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onError: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success, error) => {
        onError = error;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      onError!();

      expect(mockAddToast).toHaveBeenCalledWith({
        message: "Order removal failed.",
        type: "error",
        color: "red",
        links: [
          {
            link: "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            label: "View on explorer",
          },
        ],
      });
    });

    it("should use SDK-based indexing via createSdkIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      // Verify awaitIndexingFn was passed and is a function
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(callArgs.awaitIndexingFn).toBeDefined();
      expect(typeof callArgs.awaitIndexingFn).toBe("function");

      // Simulate calling the awaitIndexingFn to verify it calls the SDK
      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      // Mock a successful SDK response
      vi.mocked(
        mockRaindexClient.getRemoveOrdersForTransaction,
      ).mockResolvedValueOnce({
        value: [{ orderHash: "0xremovedhash" }],
      } as unknown as WasmEncodedResult<RaindexOrder[]>);

      await callArgs.awaitIndexingFn!(mockContext);

      // Verify the SDK method was called with correct arguments
      expect(
        mockRaindexClient.getRemoveOrdersForTransaction,
      ).toHaveBeenCalledWith(
        removeOrderMockArgs.chainId,
        mockSgOrderEntity.raindex,
        removeOrderMockArgs.txHash,
      );

      // Verify success was called
      expect(mockContext.onSuccess).toHaveBeenCalled();
    });

    it("should treat SDK indexing timeout as soft success in awaitIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createRemoveOrderTransaction(removeOrderMockArgs);

      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];

      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      // Mock a timeout error from the SDK
      vi.mocked(
        mockRaindexClient.getRemoveOrdersForTransaction,
      ).mockResolvedValueOnce({
        error: {
          readableMsg:
            "Timeout waiting for the subgraph to index transaction 0x123 after 10 attempts.",
        },
      } as unknown as WasmEncodedResult<RaindexOrder[]>);

      await callArgs.awaitIndexingFn!(mockContext);

      // Receipt already confirmed — timeout means indexing is slow, not that the tx failed.
      expect(mockContext.updateState).toHaveBeenCalledWith({
        status: TransactionStatusMessage.SUCCESS,
        errorDetails: TransactionStoreErrorMessage.INDEXING_CATCHING_UP,
      });
      expect(mockContext.onSuccess).toHaveBeenCalled();
      expect(mockContext.onError).not.toHaveBeenCalled();
    });
  });

  describe("createWithdrawTransaction", () => {
    const withdrawMockArgs: InternalTransactionArgs & {
      raindexClient: RaindexClient;
      entity: RaindexVault;
    } = {
      txHash:
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef" as `0x${string}`,
      chainId: 1,
      queryKey:
        "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
      entity: mockSgVaultEntity,
      raindexClient: mockRaindexClient,
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
      );
    });

    it("should create a transaction with correct parameters and awaitIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createWithdrawTransaction(withdrawMockArgs);

      expect(TransactionStore).toHaveBeenCalledWith(
        expect.objectContaining({
          name: TransactionName.WITHDRAWAL,
          errorMessage: "Withdrawal failed.",
          successMessage: "Withdrawal successful.",
          queryKey: withdrawMockArgs.queryKey,
          toastLinks: [
            {
              link: `/vaults/${withdrawMockArgs.chainId}-${withdrawMockArgs.entity.raindex}-${withdrawMockArgs.queryKey}`,
              label: "View vault",
            },
            {
              link: "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
          awaitIndexingFn: expect.any(Function),
        }),
        expect.any(Function),
        expect.any(Function),
      );

      // Verify awaitIndexingFn is a function
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(typeof callArgs.awaitIndexingFn).toBe("function");
    });

    it("should execute the transaction after creation", async () => {
      const mockExecute = vi.fn();
      const mockTransaction = { execute: mockExecute };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createWithdrawTransaction(withdrawMockArgs);

      expect(mockExecute).toHaveBeenCalled();
    });

    it("should add transaction to store", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createWithdrawTransaction(withdrawMockArgs);

      const transactions = manager.getTransactions();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let storeValue: any[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toContain(mockTransaction);
    });

    it("should handle successful transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onSuccess: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success) => {
        onSuccess = success;
        return mockTransaction as unknown as TransactionStore;
      });

      const testSpecificArgs: InternalTransactionArgs & {
        raindexClient: RaindexClient;
        entity: RaindexVault;
      } = {
        ...withdrawMockArgs, // Use base withdraw args
        queryKey: "0xvaultid", // Override queryKey for this specific test
      };

      await manager.createWithdrawTransaction(testSpecificArgs);

      onSuccess!();

      expect(mockQueryClient.invalidateQueries).toHaveBeenCalledWith({
        queryKey: ["0xvaultid"],
      });
    });

    it("should handle failed transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onError: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success, error) => {
        onError = error;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createWithdrawTransaction(withdrawMockArgs);

      onError!();

      expect(mockAddToast).toHaveBeenCalledWith({
        message: "Withdrawal failed.",
        type: "error",
        color: "red",
        links: expect.any(Array),
      });
    });

    it("should use SDK-based indexing via createSdkIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createWithdrawTransaction(withdrawMockArgs);

      // Verify awaitIndexingFn was passed and is a function
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(callArgs.awaitIndexingFn).toBeDefined();
      expect(typeof callArgs.awaitIndexingFn).toBe("function");

      // Simulate calling the awaitIndexingFn to verify it calls the SDK
      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      // Mock a successful SDK response
      vi.mocked(mockRaindexClient.getTransaction).mockResolvedValueOnce({
        value: {
          id: "0xwithdrawhash",
          from: "0xowner",
          blockNumber: 123n,
          timestamp: 1700000000n,
        },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
      } as any);

      await callArgs.awaitIndexingFn!(mockContext);

      // Verify the SDK method was called with correct arguments (chainId, raindex, txHash)
      expect(mockRaindexClient.getTransaction).toHaveBeenCalledWith(
        withdrawMockArgs.chainId,
        withdrawMockArgs.entity.raindex,
        withdrawMockArgs.txHash,
      );

      // Verify success was called
      expect(mockContext.onSuccess).toHaveBeenCalled();
    });
  });

  describe("transaction callbacks", () => {
    it("should handle successful transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onSuccess: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success) => {
        onSuccess = success;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createRemoveOrderTransaction({
        ...mockBaseArgs,
        queryKey: "0xsuccesskey",
        entity: mockSgOrderEntity,
        raindexClient: mockRaindexClient,
      });

      onSuccess!();

      expect(mockAddToast).not.toHaveBeenCalled();
      expect(mockQueryClient.invalidateQueries).toHaveBeenCalledWith({
        queryKey: ["0xsuccesskey"],
      });
    });

    it("should handle failed transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onError: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success, error) => {
        onError = error;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createRemoveOrderTransaction({
        ...mockBaseArgs,
        queryKey: "0xfailkey",
        entity: mockSgOrderEntity,
        raindexClient: mockRaindexClient,
      });

      onError!();

      expect(mockAddToast).toHaveBeenCalledWith({
        message: "Order removal failed.",
        type: "error",
        color: "red",
        links: expect.any(Array),
      });
    });
  });

  describe("clearTransactions", () => {
    it("should clear all transactions from store", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createRemoveOrderTransaction({
        ...mockBaseArgs,
        queryKey: "0xclearkey",
        entity: mockSgOrderEntity,
        raindexClient: mockRaindexClient,
      });

      manager.clearTransactions();

      const transactions = manager.getTransactions();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let storeValue: any[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toEqual([]);
    });
  });

  describe("createApprovalTransaction", () => {
    const mockEntity = {
      token: {
        symbol: "TEST",
        decimals: "18",
      },
    } as unknown as RaindexVault;
    const mockArgs: InternalTransactionArgs = {
      txHash: "0xapprovehash" as `0x${string}`,
      chainId: 1,
      queryKey: "0xvaultid",
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0xapprovehash",
      );
    });

    it("should create a transaction with correct parameters when entity is provided", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createApprovalTransaction({
        ...mockArgs,
        entity: mockEntity,
      });

      expect(TransactionStore).toHaveBeenCalledWith(
        {
          ...mockArgs,
          entity: mockEntity,
          name: "Approving TEST spend",
          errorMessage: "Approval failed.",
          successMessage: "Approval successful.",
          queryKey: mockArgs.queryKey,
          toastLinks: [
            {
              link: `/vaults/${mockArgs.chainId}-${mockEntity.raindex}-${mockArgs.queryKey}`,
              label: "View vault",
            },
            {
              link: "https://explorer.example.com/tx/0xapprovehash",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
        },
        expect.any(Function),
        expect.any(Function),
      );
    });

    it("should create a transaction with correct parameters when entity is not provided", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      // entity is deliberately omitted here
      await manager.createApprovalTransaction({ ...mockArgs });

      expect(TransactionStore).toHaveBeenCalledWith(
        {
          ...mockArgs,
          name: "Approving token spend",
          errorMessage: "Approval failed.",
          successMessage: "Approval successful.",
          queryKey: mockArgs.queryKey,
          toastLinks: [
            {
              link: "https://explorer.example.com/tx/0xapprovehash",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
        },
        expect.any(Function),
        expect.any(Function),
      );
    });
  });

  describe("createMetaTransaction", () => {
    const mockArgs: InternalTransactionArgs = {
      txHash: "0xmetahash" as `0x${string}`,
      chainId: 10,
      queryKey: "0xmetakey",
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0xmetahash",
      );
    });

    it("should create a transaction with correct parameters", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createMetaTransaction(mockArgs);

      expect(TransactionStore).toHaveBeenCalledWith(
        {
          ...mockArgs,
          name: "Publishing metadata",
          errorMessage: "Metadata publication failed.",
          successMessage: "Metadata published.",
          queryKey: mockArgs.queryKey,
          toastLinks: [
            {
              link: "https://explorer.example.com/tx/0xmetahash",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
        },
        expect.any(Function),
        expect.any(Function),
      );
    });

    it("should execute the transaction and store it", async () => {
      const mockExecute = vi.fn();
      const mockTransaction = { execute: mockExecute };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createMetaTransaction(mockArgs);

      expect(mockExecute).toHaveBeenCalled();

      const transactions = manager.getTransactions();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let storeValue: any[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toContain(mockTransaction);
    });
  });

  describe("createDepositTransaction", () => {
    const mockEntity = {
      token: {
        symbol: "TEST",
        decimals: "18",
      },
    } as unknown as RaindexVault;
    const mockArgs: InternalTransactionArgs & {
      amount: Float;
      entity: RaindexVault;
      raindexClient: RaindexClient;
    } = {
      txHash: "0xdeposithash" as `0x${string}`,
      chainId: 1,
      queryKey: "0xvaultid",
      entity: mockEntity,
      amount: Float.parse("1").value as Float,
      raindexClient: mockRaindexClient,
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0xdeposithash",
      );
    });

    it("should create a transaction with correct parameters and awaitIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createDepositTransaction(mockArgs);

      expect(TransactionStore).toHaveBeenCalledWith(
        expect.objectContaining({
          name: `Depositing ${mockArgs.amount.format().value} ${mockEntity.token.symbol}`,
          errorMessage: "Deposit failed.",
          successMessage: "Deposit successful.",
          queryKey: mockArgs.queryKey,
          toastLinks: [
            {
              link: `/vaults/${mockArgs.chainId}-${mockEntity.raindex}-${mockArgs.queryKey}`,
              label: "View vault",
            },
            {
              link: "https://explorer.example.com/tx/0xdeposithash",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
          awaitIndexingFn: expect.any(Function),
        }),
        expect.any(Function),
        expect.any(Function),
      );

      // Verify awaitIndexingFn is a function
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(typeof callArgs.awaitIndexingFn).toBe("function");
    });

    it("should execute the transaction after creation", async () => {
      const mockExecute = vi.fn();
      const mockTransaction = { execute: mockExecute };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createDepositTransaction(mockArgs);

      expect(mockExecute).toHaveBeenCalled();
    });

    it("should add transaction to store", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createDepositTransaction(mockArgs);

      const transactions = manager.getTransactions();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let storeValue: any[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toContain(mockTransaction);
    });

    it("should handle successful transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onSuccess: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success) => {
        onSuccess = success;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createDepositTransaction(mockArgs);

      onSuccess!();

      expect(mockQueryClient.invalidateQueries).toHaveBeenCalledWith({
        queryKey: ["0xvaultid"],
      });
    });

    it("should handle failed transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onError: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success, error) => {
        onError = error;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createDepositTransaction(mockArgs);

      onError!();

      expect(mockAddToast).toHaveBeenCalledWith({
        message: "Deposit failed.",
        type: "error",
        color: "red",
        links: expect.any(Array),
      });
    });

    it("should use SDK-based indexing via createSdkIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createDepositTransaction(mockArgs);

      // Verify awaitIndexingFn was passed and is a function
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(callArgs.awaitIndexingFn).toBeDefined();
      expect(typeof callArgs.awaitIndexingFn).toBe("function");

      // Simulate calling the awaitIndexingFn to verify it calls the SDK
      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      // Mock a successful SDK response
      vi.mocked(mockRaindexClient.getTransaction).mockResolvedValueOnce({
        value: {
          id: "0xdeposithash",
          from: "0xowner",
          blockNumber: 123n,
          timestamp: 1700000000n,
        },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
      } as any);

      await callArgs.awaitIndexingFn!(mockContext);

      // Verify the SDK method was called with correct arguments (chainId, raindex, txHash)
      expect(mockRaindexClient.getTransaction).toHaveBeenCalledWith(
        mockArgs.chainId,
        mockArgs.entity.raindex,
        mockArgs.txHash,
      );

      // Verify success was called
      expect(mockContext.onSuccess).toHaveBeenCalled();
    });

    it("should treat SDK indexing timeout as soft success in awaitIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createDepositTransaction(mockArgs);

      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];

      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      // Mock a timeout error from the SDK
      vi.mocked(mockRaindexClient.getTransaction).mockResolvedValueOnce({
        error: {
          readableMsg:
            "Timeout waiting for transaction 0x123 to be indexed after 10 attempts.",
        },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
      } as any);

      await callArgs.awaitIndexingFn!(mockContext);

      // Receipt already confirmed — timeout means indexing is slow, not that the tx failed.
      expect(mockContext.updateState).toHaveBeenCalledWith({
        status: TransactionStatusMessage.SUCCESS,
        errorDetails: TransactionStoreErrorMessage.INDEXING_CATCHING_UP,
      });
      expect(mockContext.onSuccess).toHaveBeenCalled();
      expect(mockContext.onError).not.toHaveBeenCalled();
    });
  });

  describe("createAddOrderTransaction", () => {
    const addOrderMockArgs: InternalTransactionArgs & {
      raindexClient: RaindexClient;
      raindex: Address;
    } = {
      txHash: "0xaddordertxhash" as `0x${string}`,
      chainId: 1,
      queryKey: "myNewStrategyDeployment",
      raindexClient: mockRaindexClient,
      raindex: "0xraindex" as Address,
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0xaddordertxhash",
      );
    });

    it("should create a transaction with correct parameters and awaitIndexingFn", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createAddOrderTransaction(addOrderMockArgs);

      expect(TransactionStore).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Deploying order",
          errorMessage: "Deployment failed.",
          successMessage: "Order deployed successfully.",
          queryKey: addOrderMockArgs.queryKey,
          awaitIndexingFn: expect.any(Function),
          toastLinks: [
            {
              link: "https://explorer.example.com/tx/0xaddordertxhash",
              label: "View on explorer",
            },
          ],
          config: mockWagmiConfig,
        }),
        expect.any(Function), // onSuccess
        expect.any(Function), // onError
      );

      // Verify awaitIndexingFn is a function (SDK-based indexing)
      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(typeof callArgs.awaitIndexingFn).toBe("function");
    });

    it("should execute the transaction after creation", async () => {
      const mockExecute = vi.fn();
      const mockTransaction = { execute: mockExecute };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createAddOrderTransaction(addOrderMockArgs);

      expect(mockExecute).toHaveBeenCalled();
    });

    it("should add transaction to store", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createAddOrderTransaction(addOrderMockArgs);

      const transactions = manager.getTransactions();
      let storeValue: unknown[] = [];
      transactions.subscribe((value) => {
        storeValue = value;
      });
      expect(storeValue).toContain(mockTransaction);
    });

    it("should handle successful transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onSuccess: (newOrderHash?: string) => void;
      vi.mocked(TransactionStore).mockImplementation((args, success) => {
        onSuccess = success;
        return mockTransaction as unknown as TransactionStore;
      });
      const newOrderHash = "0xneworderhashfromcallback";

      await manager.createAddOrderTransaction(addOrderMockArgs);

      onSuccess!(newOrderHash); // Simulate successful execution and subgraph indexing providing a new order hash

      expect(mockQueryClient.invalidateQueries).toHaveBeenCalledWith({
        queryKey: [addOrderMockArgs.queryKey],
      });
    });

    it("should wait up to 60 seconds for add-order indexing", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createAddOrderTransaction(addOrderMockArgs);

      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      vi.mocked(
        mockRaindexClient.getAddOrdersForTransaction,
      ).mockResolvedValueOnce({
        value: [{ orderHash: "0xneworderhash" }],
      } as unknown as WasmEncodedResult<RaindexOrder[]>);

      await callArgs.awaitIndexingFn!(mockContext);

      expect(mockRaindexClient.getAddOrdersForTransaction).toHaveBeenCalledWith(
        addOrderMockArgs.chainId,
        addOrderMockArgs.raindex,
        addOrderMockArgs.txHash,
        12,
        5000,
      );
    });

    it("should handle failed transaction", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onError: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success, error) => {
        onError = error;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createAddOrderTransaction(addOrderMockArgs);

      onError!();

      expect(mockAddToast).toHaveBeenCalledWith({
        message: "Deployment failed.",
        type: "error",
        color: "red",
        links: [
          {
            link: "https://explorer.example.com/tx/0xaddordertxhash",
            label: "View on explorer",
          },
        ],
      });
    });
  });

  describe("createTakeOrderTransaction", () => {
    const takeOrderMockArgs: InternalTransactionArgs & {
      raindexClient: RaindexClient;
      entity: RaindexOrder;
    } = {
      txHash:
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef" as `0x${string}`,
      chainId: 1,
      queryKey:
        "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
      entity: mockSgOrderEntity,
      raindexClient: mockRaindexClient,
    };

    beforeEach(() => {
      vi.mocked(getExplorerLink).mockResolvedValue(
        "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
      );
    });

    it("should create transaction with correct name and messages", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createTakeOrderTransaction(takeOrderMockArgs);

      expect(TransactionStore).toHaveBeenCalledWith(
        expect.objectContaining({
          name: TransactionName.TAKE_ORDER,
          errorMessage: "Take order failed.",
          successMessage: "Order taken successfully.",
          queryKey: takeOrderMockArgs.queryKey,
        }),
        expect.any(Function),
        expect.any(Function),
      );
    });

    it("should call getExplorerLink with correct params", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createTakeOrderTransaction(takeOrderMockArgs);

      expect(getExplorerLink).toHaveBeenCalledWith(
        takeOrderMockArgs.txHash,
        takeOrderMockArgs.chainId,
        "tx",
      );
      expect(TransactionStore).toHaveBeenCalledWith(
        expect.objectContaining({
          toastLinks: [
            {
              link: "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
              label: "View on explorer",
            },
          ],
        }),
        expect.any(Function),
        expect.any(Function),
      );
    });

    it("should use SDK-based indexing via createSdkIndexingFn for order tracking", async () => {
      const mockTransaction = { execute: vi.fn() };
      vi.mocked(TransactionStore).mockImplementation(
        () => mockTransaction as unknown as TransactionStore,
      );

      await manager.createTakeOrderTransaction(takeOrderMockArgs);

      const callArgs = vi.mocked(TransactionStore).mock.calls[0][0];
      expect(callArgs.awaitIndexingFn).toBeDefined();
      expect(typeof callArgs.awaitIndexingFn).toBe("function");

      const mockContext: IndexingContext = {
        updateState: vi.fn(),
        onSuccess: vi.fn(),
        onError: vi.fn(),
        links: [],
      };

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      vi.mocked(mockRaindexClient.getTransaction).mockResolvedValueOnce({
        value: {
          id: "0x0123",
          from: "0xowner",
          blockNumber: 123n,
          timestamp: 1700000000n,
        },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
      } as any);

      await callArgs.awaitIndexingFn!(mockContext);

      expect(mockRaindexClient.getTransaction).toHaveBeenCalledWith(
        takeOrderMockArgs.chainId,
        takeOrderMockArgs.entity.raindex,
        takeOrderMockArgs.txHash,
      );

      expect(mockContext.onSuccess).toHaveBeenCalled();
    });

    it("should handle successful transaction completion", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onSuccess: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success) => {
        onSuccess = success;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createTakeOrderTransaction(takeOrderMockArgs);

      onSuccess!();

      expect(mockQueryClient.invalidateQueries).toHaveBeenCalledWith({
        queryKey: [takeOrderMockArgs.queryKey],
      });
    });

    it("should handle transaction failure", async () => {
      const mockTransaction = { execute: vi.fn() };
      let onError: () => void;
      vi.mocked(TransactionStore).mockImplementation((args, success, error) => {
        onError = error;
        return mockTransaction as unknown as TransactionStore;
      });

      await manager.createTakeOrderTransaction(takeOrderMockArgs);

      onError!();

      expect(mockAddToast).toHaveBeenCalledWith({
        message: "Take order failed.",
        type: "error",
        color: "red",
        links: [
          {
            link: "https://explorer.example.com/tx/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            label: "View on explorer",
          },
        ],
      });
    });
  });
});

describe("createSdkIndexingFn", () => {
  let mockContext: IndexingContext;
  let mockUpdateState: ReturnType<typeof vi.fn>;
  let mockOnSuccess: ReturnType<typeof vi.fn>;
  let mockOnError: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockUpdateState = vi.fn();
    mockOnSuccess = vi.fn();
    mockOnError = vi.fn();

    mockContext = {
      updateState: mockUpdateState,
      onSuccess: mockOnSuccess,
      onError: mockOnError,
      links: [{ link: "/existing", label: "Existing Link" }],
    };
  });

  it("should set PENDING_SUBGRAPH status when called", async () => {
    const mockCall = vi.fn().mockResolvedValue({ value: "test-value" });
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.PENDING_SUBGRAPH,
    });
  });

  it("should call onSuccess when SDK returns valid data and isSuccess returns true", async () => {
    const mockCall = vi.fn().mockResolvedValue({ value: ["order1", "order2"] });
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: (value: string[]) => value.length > 0,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.SUCCESS,
    });
    expect(mockOnSuccess).toHaveBeenCalled();
    expect(mockOnError).not.toHaveBeenCalled();
  });

  it("should call buildLinks and prepend links to context when provided", async () => {
    const mockCall = vi.fn().mockResolvedValue({ value: { id: "order-123" } });
    const mockBuildLinks = vi
      .fn()
      .mockReturnValue([{ link: "/orders/order-123", label: "View Order" }]);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
      buildLinks: mockBuildLinks,
    });

    await indexingFn(mockContext);

    expect(mockBuildLinks).toHaveBeenCalledWith({ id: "order-123" });
    expect(mockUpdateState).toHaveBeenCalledWith({
      links: [
        { link: "/orders/order-123", label: "View Order" },
        { link: "/existing", label: "Existing Link" },
      ],
    });
  });

  it("should not update links when buildLinks returns empty array", async () => {
    const mockCall = vi.fn().mockResolvedValue({ value: {} });
    const mockBuildLinks = vi.fn().mockReturnValue([]);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
      buildLinks: mockBuildLinks,
    });

    await indexingFn(mockContext);

    // Should not call updateState with links since array is empty
    const linksUpdateCalls = mockUpdateState.mock.calls.filter(
      (call) => call[0] && typeof call[0] === "object" && "links" in call[0],
    );
    expect(linksUpdateCalls).toHaveLength(0);
  });

  it('should treat indexing timeout as soft success when error contains "timeout"', async () => {
    const mockCall = vi.fn().mockResolvedValue({
      error: { readableMsg: "Request timeout exceeded" },
    } as WasmEncodedResult<unknown>);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.SUCCESS,
      errorDetails: TransactionStoreErrorMessage.INDEXING_CATCHING_UP,
    });
    expect(mockOnSuccess).toHaveBeenCalled();
    expect(mockOnError).not.toHaveBeenCalled();
  });

  it("should treat case-insensitive timeout detection as soft success", async () => {
    const mockCall = vi.fn().mockResolvedValue({
      error: { readableMsg: "TIMEOUT occurred while fetching data" },
    } as WasmEncodedResult<unknown>);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.SUCCESS,
      errorDetails: TransactionStoreErrorMessage.INDEXING_CATCHING_UP,
    });
    expect(mockOnSuccess).toHaveBeenCalled();
    expect(mockOnError).not.toHaveBeenCalled();
  });

  it("should treat SDK TransactionIndexingTimeout as soft success", async () => {
    const mockCall = vi.fn().mockResolvedValue({
      error: {
        readableMsg:
          "Timeout waiting for transaction 0x123abc to be indexed after 10 attempts.",
      },
    } as WasmEncodedResult<unknown>);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.SUCCESS,
      errorDetails: TransactionStoreErrorMessage.INDEXING_CATCHING_UP,
    });
    expect(mockOnSuccess).toHaveBeenCalled();
    expect(mockOnError).not.toHaveBeenCalled();
  });

  it("should set SUBGRAPH_FAILED and call onError for non-timeout errors", async () => {
    const mockCall = vi.fn().mockResolvedValue({
      error: { readableMsg: "Network connection failed" },
    } as WasmEncodedResult<unknown>);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.ERROR,
      errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
    });
    expect(mockOnError).toHaveBeenCalled();
  });

  it("should set SUBGRAPH_FAILED when error has no readableMsg", async () => {
    const mockCall = vi.fn().mockResolvedValue({
      error: {},
    } as WasmEncodedResult<unknown>);
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.ERROR,
      errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
    });
  });

  it("should set SUBGRAPH_FAILED and call onError when isSuccess returns false", async () => {
    const mockCall = vi.fn().mockResolvedValue({ value: [] });
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: (value: unknown[]) => value.length > 0, // empty array fails
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.ERROR,
      errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
    });
    expect(mockOnError).toHaveBeenCalled();
    expect(mockOnSuccess).not.toHaveBeenCalled();
  });

  it("should set SUBGRAPH_FAILED and call onError when value is undefined", async () => {
    const mockCall = vi.fn().mockResolvedValue({ value: undefined });
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.ERROR,
      errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
    });
    expect(mockOnError).toHaveBeenCalled();
  });

  it("should set SUBGRAPH_FAILED and call onError when SDK call throws an exception", async () => {
    const mockCall = vi.fn().mockRejectedValue(new Error("Unexpected error"));
    const indexingFn = createSdkIndexingFn({
      call: mockCall,
      isSuccess: () => true,
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      status: TransactionStatusMessage.ERROR,
      errorDetails: TransactionStoreErrorMessage.SUBGRAPH_FAILED,
    });
    expect(mockOnError).toHaveBeenCalled();
    expect(mockOnSuccess).not.toHaveBeenCalled();
  });

  it("should work with complex value types", async () => {
    type Order = { orderHash: string; owner: string };
    const orders: Order[] = [
      { orderHash: "0x123", owner: "0xabc" },
      { orderHash: "0x456", owner: "0xdef" },
    ];
    const mockCall = vi.fn().mockResolvedValue({ value: orders });
    const indexingFn = createSdkIndexingFn<Order[]>({
      call: mockCall,
      isSuccess: (value) => Array.isArray(value) && value.length > 0,
      buildLinks: (value) => [
        { link: `/orders/${value[0].orderHash}`, label: "View Order" },
      ],
    });

    await indexingFn(mockContext);

    expect(mockUpdateState).toHaveBeenCalledWith({
      links: [
        { link: "/orders/0x123", label: "View Order" },
        { link: "/existing", label: "Existing Link" },
      ],
    });
    expect(mockOnSuccess).toHaveBeenCalled();
  });
});
