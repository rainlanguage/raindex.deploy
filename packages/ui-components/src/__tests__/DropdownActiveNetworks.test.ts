import { render, fireEvent, screen, waitFor } from "@testing-library/svelte";
import { get, writable, type Writable } from "svelte/store";
import { beforeEach, expect, test, describe, type Mock } from "vitest";
import DropdownActiveNetworks from "../lib/components/dropdown/DropdownActiveNetworks.svelte";
import { useRaindexClient } from "$lib/hooks/useRaindexClient";
import type { NetworkCfg, NetworkSyncStatus } from "@rainlanguage/raindex";

vi.mock("$lib/hooks/useRaindexClient", () => ({
  useRaindexClient: vi.fn(),
}));

describe("DropdownActiveNetworks", () => {
  let selectedChainIdsStore: Writable<number[]>;

  beforeEach(() => {
    (useRaindexClient as Mock).mockReturnValue({
      getUniqueChainIds: () => ({
        error: undefined,
        value: [1, 2, 14],
      }),
      getAllNetworks: () => ({
        error: undefined,
        value: new Map([
          ["mainnet", { key: "Ethereum", chainId: 1 } as NetworkCfg],
          ["testnet", { key: "Expanse Network", chainId: 2 } as NetworkCfg],
          ["local", { key: "local", chainId: 14 } as NetworkCfg],
        ]),
      }),
    });
    selectedChainIdsStore = writable([]);
  });

  test("renders correctly", () => {
    render(DropdownActiveNetworks, {
      props: {
        selectedChainIds: selectedChainIdsStore,
      },
    });
    expect(screen.getByText("Networks")).toBeInTheDocument();
  });

  test("displays the correct number of options", async () => {
    render(DropdownActiveNetworks, {
      props: {
        selectedChainIds: selectedChainIdsStore,
      },
    });

    await fireEvent.click(screen.getByTestId("dropdown-checkbox-button"));

    await waitFor(() => {
      const options = screen.getAllByTestId("dropdown-checkbox-option");
      expect(options).toHaveLength(3);
    });
  });

  test("updates selected chain ids when an option is selected", async () => {
    render(DropdownActiveNetworks, {
      props: {
        selectedChainIds: selectedChainIdsStore,
      },
    });

    await fireEvent.click(screen.getByTestId("dropdown-checkbox-button"));
    await fireEvent.click(screen.getByText("Ethereum"));
    await waitFor(() => {
      expect(get(selectedChainIdsStore)).toEqual([1]);
    });

    await fireEvent.click(screen.getByText("Expanse Network"));
    await waitFor(() => {
      expect(get(selectedChainIdsStore)).toEqual([1, 2]);
    });

    await fireEvent.click(screen.getByText("Flare Mainnet"));
    await waitFor(() => {
      expect(get(selectedChainIdsStore)).toEqual([1, 2, 14]);
    });
  });

  test("uses registry names and identifies subgraph-only networks", async () => {
    (useRaindexClient as Mock).mockReturnValue({
      getUniqueChainIds: () => ({ error: undefined, value: [4663] }),
      getAllNetworks: () => ({
        error: undefined,
        value: new Map([
          ["robinhood", { key: "robinhood", chainId: 4663 } as NetworkCfg],
        ]),
      }),
    });

    render(DropdownActiveNetworks, {
      props: {
        selectedChainIds: selectedChainIdsStore,
        localDbStatuses: new Map<number, NetworkSyncStatus>(),
      },
    });

    await fireEvent.click(screen.getByTestId("dropdown-checkbox-button"));

    expect(screen.getByText("Robinhood · Subgraph")).toBeInTheDocument();
  });
});
