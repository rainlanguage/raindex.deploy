import { render, screen, waitFor } from "@testing-library/svelte";
import {
  RaindexClient,
  RaindexOrder,
  RaindexTransaction,
  RaindexVault,
} from "@rainlanguage/raindex";
import { describe, it, expect, vi, beforeEach, type Mock } from "vitest";
import { QueryClient } from "@tanstack/svelte-query";
import OrderDetail from "../lib/components/detail/OrderDetail.svelte";
import { readable, writable } from "svelte/store";
import { darkChartTheme } from "../lib/utils/lightweightChartsThemes";
import userEvent from "@testing-library/user-event";
import { useAccount } from "$lib/providers/wallet/useAccount";
import type { ComponentProps } from "svelte";
import { invalidateTanstackQueries } from "$lib/queries/queryClient";
import { useToasts } from "$lib/providers/toasts/useToasts";
import { useRaindexClient } from "$lib/hooks/useRaindexClient";
import { getExplorerLink } from "$lib/services/getExplorerLink";

vi.mock("$lib/hooks/useRaindexClient", () => ({
  useRaindexClient: vi.fn(),
}));

// Mock the account hook
vi.mock("$lib/providers/wallet/useAccount", () => ({
  useAccount: vi.fn(),
}));

// Mock the js_api functions
vi.mock("@rainlanguage/raindex", () => ({
  RaindexClient: vi.fn(),
}));

// Mock the query client functions
vi.mock("$lib/queries/queryClient", () => ({
  invalidateTanstackQueries: vi.fn(),
}));

const mockErrToast = vi.fn();

vi.mock("$lib/providers/toasts/useToasts", () => ({
  useToasts: vi.fn(),
}));

vi.mock("$lib/components/charts/OrderTradesChart.svelte", async () => {
  const mockLightweightCharts = (
    await import("../lib/__mocks__/MockComponent.svelte")
  ).default;
  return { default: mockLightweightCharts };
});
vi.mock("$lib/components/CodeMirrorRainlang.svelte", async () => {
  const mockCodeMirror = (
    await import("../lib/__mocks__/CodeMirrorRainlang.svelte")
  ).default;
  return { default: mockCodeMirror };
});

vi.mock("$lib/services/getExplorerLink", () => ({
  getExplorerLink: vi.fn(),
}));
const raindexAddress = "0x123456789012345678901234567890123456abcd";
const orderHash = "0x0234";

const defaultProps: ComponentProps<OrderDetail> = {
  chainId: 1,
  raindexAddress,
  orderHash,
  codeMirrorTheme: readable("dark"),
  lightweightChartsTheme: readable(darkChartTheme),
  onRemove: vi.fn(),
  onDeposit: vi.fn(),
  onWithdraw: vi.fn(),
  onWithdrawAll: vi.fn(),
};

const mockVaultsList = () => ({
  items: [],
  getWithdrawableVaults: () => ({ value: [], error: null }),
});

const mockOrder: RaindexOrder = {
  chainId: 1,
  raindex: raindexAddress,
  id: "mockId",
  orderBytes: "0x0000000000000000000000000000000000000000...",
  orderHash: orderHash,
  owner: "0x1234567890123456789012345678901234567890",
  active: true,
  meta: undefined,
  rainlang: undefined,
  timestampAdded: BigInt(1234567890),
  inputsList: mockVaultsList(),
  outputsList: mockVaultsList(),
  inputsOutputsList: mockVaultsList(),
  vaultsList: {
    ...mockVaultsList(),
    items: [
      {
        chainId: 1,
        vaultType: "input",
        id: "0x0000000000000000000000000000000000000002",
        token: {
          id: "0x0000000000000000000000000000000000000000",
          address: "0x0000000000000000000000000000000000000000",
          name: "MockToken",
          symbol: "MCK",
          decimals: "18",
        },
        balance: BigInt(0),
        vaultId: BigInt(2),
        owner: "0x1234567890123456789012345678901234567890",
        ordersAsOutput: [],
        ordersAsInput: [],
        raindex: raindexAddress,
      } as unknown as RaindexVault,
      {
        chainId: 1,
        vaultType: "output",
        id: "0x0000000000000000000000000000000000000001",
        token: {
          id: "0x0000000000000000000000000000000000000000",
          address: "0x0000000000000000000000000000000000000000",
          name: "MockToken2",
          symbol: "MCK2",
          decimals: "18",
        },
        balance: BigInt(0),
        vaultId: BigInt(1),
        owner: "0x1234567890123456789012345678901234567890",
        ordersAsOutput: [],
        ordersAsInput: [],
        raindex: raindexAddress,
      } as unknown as RaindexVault,
    ],
  },
  transaction: {
    blockNumber: BigInt(12345678),
    timestamp: BigInt(1234567890),
    id: "0x0000000000000000000000000000000000000000",
    from: "0x1234567890123456789012345678901234567890",
  } as unknown as RaindexTransaction,
  tradesCount: 0,
} as unknown as RaindexOrder;

const mockMatchesAccount = vi.fn();
describe("OrderDetail", () => {
  let queryClient: QueryClient;
  let mockRaindexClient: RaindexClient;
  const resolveOrder = (override: Partial<RaindexOrder> = {}) =>
    (mockRaindexClient.getOrderByHash as Mock).mockResolvedValue({
      value: { ...mockOrder, ...override },
    });

  beforeEach(async () => {
    vi.clearAllMocks();
    vi.resetAllMocks();
    queryClient = new QueryClient();

    (useAccount as Mock).mockReturnValue({
      matchesAccount: mockMatchesAccount,
    });

    mockRaindexClient = {
      getOrderByHash: vi.fn().mockResolvedValue({
        value: mockOrder,
      }),
    } as unknown as RaindexClient;
    (useRaindexClient as Mock).mockReturnValue(mockRaindexClient);

    (useToasts as Mock).mockReturnValue({
      toasts: writable([]),
      errToast: mockErrToast,
      removeToast: vi.fn(),
    });

    (getExplorerLink as Mock).mockReturnValue(
      "https://etherscan.io/address/0x1234567890123456789012345678901234567890",
    );
  });

  it("calls the order detail query with the correct order hash", async () => {
    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    expect(mockRaindexClient.getOrderByHash).toHaveBeenCalledWith(
      1,
      raindexAddress,
      orderHash,
    );
  });

  it("shows the correct empty message when the query returns no data", async () => {
    (mockRaindexClient.getOrderByHash as Mock).mockResolvedValue({
      value: null,
    });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order not found")).toBeInTheDocument();
    });
  });

  it("shows the correct data when the query returns data", async () => {
    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
      expect(screen.getByText("Raindex")).toBeInTheDocument();
      expect(screen.getByText("Owner")).toBeInTheDocument();
      expect(screen.getByText("Created")).toBeInTheDocument();
      expect(screen.getByText(raindexAddress)).toBeInTheDocument();
      expect(
        screen.getByText("0x1234567890123456789012345678901234567890"),
      ).toBeInTheDocument();
    });
  });

  it("shows remove button if owner wallet matches and order is active", async () => {
    mockMatchesAccount.mockReturnValue(true);
    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      const removeButton = screen.getByTestId("remove-button");
      expect(removeButton).toBeInTheDocument();
      expect(defaultProps.onRemove).not.toHaveBeenCalled();
    });

    await userEvent.click(screen.getByTestId("remove-button"));

    await waitFor(() => {
      expect(defaultProps.onRemove).toHaveBeenCalledWith(
        mockRaindexClient,
        mockOrder,
      );
    });
  });

  it("renders a remove dropdown (not a bare remove button) when onRemoveAndWithdrawAll is provided", async () => {
    mockMatchesAccount.mockReturnValue(true);
    render(OrderDetail, {
      props: { ...defaultProps, onRemoveAndWithdrawAll: vi.fn() },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      // When onRemoveAndWithdrawAll is provided, the dropdown trigger renders.
      expect(screen.getByTestId("remove-order-menu")).toBeInTheDocument();
    });

    // Dropdown items are hidden until the trigger is opened.
    expect(
      screen.queryByTestId("remove-and-withdraw-all-button"),
    ).not.toBeInTheDocument();

    await userEvent.click(screen.getByTestId("remove-order-menu"));

    await waitFor(() => {
      expect(screen.getByTestId("remove-button")).toBeInTheDocument();
      expect(
        screen.getByTestId("remove-and-withdraw-all-button"),
      ).toBeInTheDocument();
    });
  });

  it("calls onRemove (not onRemoveAndWithdrawAll) when the Remove dropdown item is clicked", async () => {
    mockMatchesAccount.mockReturnValue(true);
    const onRemove = vi.fn();
    const onRemoveAndWithdrawAll = vi.fn();
    render(OrderDetail, {
      props: { ...defaultProps, onRemove, onRemoveAndWithdrawAll },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByTestId("remove-order-menu")).toBeInTheDocument();
    });
    await userEvent.click(screen.getByTestId("remove-order-menu"));
    await userEvent.click(await screen.findByTestId("remove-button"));

    expect(onRemove).toHaveBeenCalledWith(mockRaindexClient, mockOrder);
    expect(onRemoveAndWithdrawAll).not.toHaveBeenCalled();
  });

  it("calls onRemoveAndWithdrawAll with the order's vaultsList when that dropdown item is clicked", async () => {
    mockMatchesAccount.mockReturnValue(true);
    const onRemove = vi.fn();
    const onRemoveAndWithdrawAll = vi.fn();
    render(OrderDetail, {
      props: { ...defaultProps, onRemove, onRemoveAndWithdrawAll },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByTestId("remove-order-menu")).toBeInTheDocument();
    });
    await userEvent.click(screen.getByTestId("remove-order-menu"));
    await userEvent.click(
      await screen.findByTestId("remove-and-withdraw-all-button"),
    );

    expect(onRemoveAndWithdrawAll).toHaveBeenCalledWith(
      mockRaindexClient,
      mockOrder,
      mockOrder.vaultsList,
    );
    expect(onRemove).not.toHaveBeenCalled();
  });

  it("does not show the remove dropdown for inactive orders even with onRemoveAndWithdrawAll", async () => {
    mockMatchesAccount.mockReturnValue(true);
    (mockRaindexClient.getOrderByHash as Mock).mockResolvedValue({
      value: { ...mockOrder, active: false },
    });
    render(OrderDetail, {
      props: { ...defaultProps, onRemoveAndWithdrawAll: vi.fn() },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("remove-order-menu")).not.toBeInTheDocument();
    expect(screen.queryByTestId("remove-button")).not.toBeInTheDocument();
  });

  it("does not show remove button if account does not match owner", async () => {
    mockMatchesAccount.mockReturnValue(false);

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.queryByTestId("remove-button")).not.toBeInTheDocument();
    });
  });

  it("does not show remove button if order is not active", async () => {
    // Modify the mock to return an inactive order
    (mockRaindexClient.getOrderByHash as Mock).mockResolvedValue({
      value: {
        ...mockOrder,
        active: false,
      },
    });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.queryByTestId("remove-button")).not.toBeInTheDocument();
    });
  });

  it("refresh button triggers query invalidation when clicked", async () => {
    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(async () => {
      const refreshButton = screen.getByTestId("top-refresh");
      await userEvent.click(refreshButton);

      expect(invalidateTanstackQueries).toHaveBeenCalledWith(queryClient, [
        orderHash,
      ]);
    });
  });

  it("failed query invalidation triggers a toast", async () => {
    (invalidateTanstackQueries as Mock).mockRejectedValue(
      new Error("Failed to refresh"),
    );

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(async () => {
      const refreshButton = screen.getByTestId("top-refresh");
      await userEvent.click(refreshButton);
    });

    await waitFor(() => {
      expect(mockErrToast).toHaveBeenCalledWith("Failed to refresh");
    });
  });

  it("calls onDeposit callback when deposit button is clicked", async () => {
    mockMatchesAccount.mockReturnValue(true);
    const user = userEvent.setup();
    const mockOnDeposit = vi.fn();

    render(OrderDetail, {
      props: {
        ...defaultProps,
        onDeposit: mockOnDeposit,
      },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();

      expect(screen.getByText("Raindex")).toBeInTheDocument();

      expect(screen.getByText("Owner")).toBeInTheDocument();

      expect(screen.getByText("Created")).toBeInTheDocument();
    });

    const depositButton = screen.getAllByTestId("deposit-button");
    await user.click(depositButton[0]);

    expect(mockOnDeposit).toHaveBeenCalledWith(
      mockRaindexClient,
      mockOrder.vaultsList.items[1],
    );
  });

  it("calls onWithdraw callback when withdraw button is clicked", async () => {
    mockMatchesAccount.mockReturnValue(true);
    const user = userEvent.setup();
    const mockOnWithdraw = vi.fn();

    render(OrderDetail, {
      props: {
        ...defaultProps,
        onWithdraw: mockOnWithdraw,
      },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
      expect(screen.getByText("Raindex")).toBeInTheDocument();
      expect(screen.getByText("Owner")).toBeInTheDocument();
      expect(screen.getByText("Created")).toBeInTheDocument();
    });

    const withdrawButton = screen.getAllByTestId("withdraw-button");
    await user.click(withdrawButton[0]);

    expect(mockOnWithdraw).toHaveBeenCalledWith(
      mockRaindexClient,
      mockOrder.vaultsList.items[1],
    );
  });

  it("shows hover title and aria-label on the deposit and withdraw icon buttons", async () => {
    mockMatchesAccount.mockReturnValue(true);

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
    });

    const depositButton = screen.getAllByTestId("deposit-button")[0];
    expect(depositButton).toHaveAttribute("title", "Deposit into vault");
    expect(depositButton).toHaveAttribute("aria-label", "Deposit into vault");

    const withdrawButton = screen.getAllByTestId("withdraw-button")[0];
    expect(withdrawButton).toHaveAttribute("title", "Withdraw from vault");
    expect(withdrawButton).toHaveAttribute("aria-label", "Withdraw from vault");

    const refreshButton = screen.getByTestId("top-refresh");
    expect(refreshButton).toHaveAttribute("title", "Refresh order");
    expect(refreshButton).toHaveAttribute("aria-label", "Refresh order");
  });

  it("does not show Take Order button for inactive orders", async () => {
    (mockRaindexClient.getOrderByHash as Mock).mockResolvedValue({
      value: {
        ...mockOrder,
        active: false,
      },
    });

    render(OrderDetail, {
      props: {
        ...defaultProps,
        onTakeOrder: vi.fn(),
      },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
    });

    expect(screen.queryByTestId("take-order-button")).not.toBeInTheDocument();
  });

  it("shows Take Order button for active orders", async () => {
    render(OrderDetail, {
      props: {
        ...defaultProps,
        onTakeOrder: vi.fn(),
      },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
    });

    expect(screen.getByTestId("take-order-button")).toBeInTheDocument();
  });

  it("calls onTakeOrder callback when Take Order button clicked", async () => {
    const user = userEvent.setup();
    const mockOnTakeOrder = vi.fn();

    render(OrderDetail, {
      props: {
        ...defaultProps,
        onTakeOrder: mockOnTakeOrder,
      },
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.getByText("Order")).toBeInTheDocument();
    });

    const takeOrderButton = screen.getByTestId("take-order-button");
    expect(takeOrderButton).toBeInTheDocument();

    await user.click(takeOrderButton);

    expect(mockOnTakeOrder).toHaveBeenCalledWith(mockRaindexClient, mockOrder);
  });

  it("renders the Dotrain tab and content when dotrain source exists", async () => {
    const user = userEvent.setup();
    resolveOrder({ dotrainSource: "dotrain:source" });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    const dotrainTab = await screen.findByText("Dotrain");
    await user.click(dotrainTab);

    await waitFor(() => {
      expect(screen.getByTestId("codemirror-rainlang")).toHaveTextContent(
        "dotrain:source",
      );
    });
  });

  it("does not render the Dotrain tab when dotrain source is missing", async () => {
    resolveOrder({ dotrainSource: undefined });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(screen.queryByText("Dotrain")).not.toBeInTheDocument();
    });
  });

  it("renders the builder state tab with formatted JSON when present", async () => {
    const user = userEvent.setup();
    const builderState = JSON.stringify({ foo: "bar" });
    resolveOrder({ orderBuilderState: builderState });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    const builderTab = await screen.findByText("Builder State");
    await user.click(builderTab);

    await waitFor(() => {
      expect(screen.getByTestId("builder-state-json")).toHaveTextContent(
        '"foo": "bar"',
      );
    });
  });

  it("handles invalid builder state JSON gracefully", async () => {
    const user = userEvent.setup();
    resolveOrder({ orderBuilderState: "{invalid" });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    const builderTab = await screen.findByText("Builder State");
    await user.click(builderTab);

    await waitFor(() => {
      expect(screen.getByTestId("builder-state-json")).toHaveTextContent(
        "Invalid builder state",
      );
    });
  });

  it("renders on-chain Rainlang even without a Dotrain source", async () => {
    const user = userEvent.setup();
    const rainlangText = "/* rainlang source */";
    resolveOrder({ rainlang: rainlangText, dotrainSource: undefined });

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    const rainlangTab = await screen.findByText("On-chain Rainlang");
    await user.click(rainlangTab);

    await waitFor(() => {
      expect(screen.getByTestId("codemirror-rainlang")).toHaveTextContent(
        rainlangText,
      );
    });
  });

  it("renders owner address as explorer link when explorer is available", async () => {
    const explorerUrl =
      "https://etherscan.io/address/0x1234567890123456789012345678901234567890";
    (getExplorerLink as Mock).mockReturnValue(explorerUrl);

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      const ownerLink = screen.getByRole("link", {
        name: /0x1234567890123456789012345678901234567890/i,
      });
      expect(ownerLink).toBeInTheDocument();
      expect(ownerLink).toHaveAttribute("href", explorerUrl);
      expect(ownerLink).toHaveAttribute("target", "_blank");
      expect(ownerLink).toHaveAttribute("rel", "noopener noreferrer");
    });
  });

  it("falls back to Hash component when no explorer link is available", async () => {
    (getExplorerLink as Mock).mockReturnValue("");

    render(OrderDetail, {
      props: defaultProps,
      context: new Map([["$$_queryClient", queryClient]]),
    });

    await waitFor(() => {
      expect(
        screen.getByText("0x1234567890123456789012345678901234567890"),
      ).toBeInTheDocument();
      expect(
        screen.queryByRole("link", {
          name: /0x1234567890123456789012345678901234567890/i,
        }),
      ).not.toBeInTheDocument();
    });
  });

  describe("Oracle URL functionality", () => {
    it("renders Oracle section when oracleUrl is present", async () => {
      const oracleUrl = "https://oracle.example.com/api";
      resolveOrder({ oracleUrl });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        expect(screen.getByText("Oracle")).toBeInTheDocument();
      });
    });

    it("does not render Oracle section when oracleUrl is not present", async () => {
      resolveOrder({ oracleUrl: undefined });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        expect(screen.queryByText("Oracle")).not.toBeInTheDocument();
      });
    });

    it("renders Oracle URL as clickable link when URL is https", async () => {
      const oracleUrl = "https://oracle.example.com/api";
      resolveOrder({ oracleUrl });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        const oracleLink = screen.getByRole("link", { name: oracleUrl });
        expect(oracleLink).toBeInTheDocument();
        expect(oracleLink).toHaveAttribute("href", oracleUrl);
        expect(oracleLink).toHaveAttribute("target", "_blank");
        expect(oracleLink).toHaveAttribute("rel", "noopener noreferrer");
        expect(oracleLink).toHaveClass("text-blue-500", "hover:underline");
      });
    });

    it("renders Oracle URL as clickable link when URL is http", async () => {
      const oracleUrl = "http://localhost:8080/oracle";
      resolveOrder({ oracleUrl });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        const oracleLink = screen.getByRole("link", { name: oracleUrl });
        expect(oracleLink).toBeInTheDocument();
        expect(oracleLink).toHaveAttribute("href", oracleUrl);
        expect(oracleLink).toHaveAttribute("target", "_blank");
        expect(oracleLink).toHaveAttribute("rel", "noopener noreferrer");
      });
    });

    it("renders Oracle URL as plain text when URL does not start with http/https", async () => {
      const oracleUrl = "ftp://oracle.example.com/api";
      resolveOrder({ oracleUrl });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        expect(screen.getByText(oracleUrl)).toBeInTheDocument();
        expect(
          screen.queryByRole("link", { name: oracleUrl }),
        ).not.toBeInTheDocument();
      });
    });

    it("renders Oracle URL as plain text with gray styling for non-http URLs", async () => {
      const oracleUrl = "javascript:alert(1)";
      resolveOrder({ oracleUrl });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        const textElement = screen.getByText(oracleUrl);
        expect(textElement).toBeInTheDocument();
        expect(textElement.tagName).toBe("SPAN");
        expect(textElement).toHaveClass("text-gray-500");
        // Should not be a link itself (but other links may exist on page)
        expect(
          screen.queryByRole("link", { name: oracleUrl }),
        ).not.toBeInTheDocument();
      });
    });

    it("displays Oracle section with info icon when URL is present", async () => {
      const oracleUrl = "https://oracle.example.com/api";
      resolveOrder({ oracleUrl });

      render(OrderDetail, {
        props: defaultProps,
        context: new Map([["$$_queryClient", queryClient]]),
      });

      await waitFor(() => {
        // Check that Oracle section is present
        expect(screen.getByText("Oracle")).toBeInTheDocument();
        // Check that the Oracle URL is displayed
        expect(screen.getByText(oracleUrl)).toBeInTheDocument();
      });
    });
  });
});
