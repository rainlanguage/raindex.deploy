import * as chains from "viem/chains";
import type { NetworkCfg } from "@rainlanguage/raindex";

type NetworkNameConfig = Pick<NetworkCfg, "chainId" | "key" | "label">;

function humanizeNetworkKey(key: string): string {
  return key
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function getNetworkName(
  chainId: number,
  configuredNetworks?: Iterable<NetworkNameConfig>,
): string | undefined {
  const configuredNetwork = configuredNetworks
    ? Array.from(configuredNetworks).find(
        (network) => network.chainId === chainId,
      )
    : undefined;
  const configuredLabel = configuredNetwork?.label?.trim();
  if (configuredLabel) return configuredLabel;

  const chain = Object.values(chains).find((chain) => chain.id === chainId);
  if (chain?.name) return chain.name;

  return configuredNetwork?.key
    ? humanizeNetworkKey(configuredNetwork.key)
    : undefined;
}

if (import.meta.vitest) {
  describe("getNetworkName", () => {
    it("should return the network name for a given chain id", () => {
      expect(getNetworkName(1)).toBe("Ethereum");
      expect(getNetworkName(137)).toBe("Polygon");
    });

    it("uses configured metadata for chains unknown to viem", () => {
      expect(
        getNetworkName(4663, [
          { chainId: 4663, key: "robinhood", label: undefined },
        ]),
      ).toBe("Robinhood");
    });

    it("prefers a configured label", () => {
      expect(
        getNetworkName(1, [
          { chainId: 1, key: "mainnet", label: "Ethereum Mainnet" },
        ]),
      ).toBe("Ethereum Mainnet");
    });
  });
}
