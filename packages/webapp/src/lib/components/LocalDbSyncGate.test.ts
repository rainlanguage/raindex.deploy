import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import LocalDbSyncGate from "./LocalDbSyncGate.svelte";
import {
  resetLocalDbStatus,
  updateNetworkStatus,
} from "$lib/stores/localDbStatus";

vi.mock("@rainlanguage/ui-components", async (importOriginal) => {
  const original =
    await importOriginal<typeof import("@rainlanguage/ui-components")>();
  return {
    ...original,
    getNetworkName: (chainId: number) =>
      chainId === 1 ? "Ethereum" : "Robinhood",
    useRaindexClient: () => ({
      getAllNetworks: () => ({ value: new Map() }),
    }),
  };
});

describe("LocalDbSyncGate", () => {
  beforeEach(() => resetLocalDbStatus());
  afterEach(() => cleanup());

  it("reports a new network sync after the initial sync is ready and dispatches completion", async () => {
    updateNetworkStatus({
      chainId: 1,
      status: "active",
      schedulerState: "leader",
    });

    const { component } = render(LocalDbSyncGate, { nonBlocking: true });
    const syncComplete = vi.fn();
    component.$on("synccomplete", syncComplete);

    updateNetworkStatus({
      chainId: 2020,
      status: "syncing",
      schedulerState: "leader",
    });

    expect(
      await screen.findByText("Local database syncing: Robinhood"),
    ).toBeInTheDocument();

    updateNetworkStatus({
      chainId: 2020,
      status: "active",
      schedulerState: "leader",
    });

    await waitFor(() => expect(syncComplete).toHaveBeenCalledOnce());
  });

  it("reports a new network failure after the initial sync is ready", async () => {
    updateNetworkStatus({
      chainId: 1,
      status: "active",
      schedulerState: "leader",
    });
    render(LocalDbSyncGate, { nonBlocking: true });

    updateNetworkStatus({
      chainId: 2020,
      status: "failure",
      schedulerState: "leader",
      error: "sync failed",
    });

    expect(
      await screen.findByText("Local database sync failed for Robinhood"),
    ).toBeInTheDocument();
  });
});
