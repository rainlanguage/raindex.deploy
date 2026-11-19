import * as chains from "viem/chains";

// viem 2.24.3 does not ship Robinhood Chain (4663).
const extraExplorers: Record<number, string> = {
  4663: "https://robinhoodchain.blockscout.com",
};

export const getExplorerLink = (
  hash: string,
  chainId: number,
  type: "tx" | "address",
): string => {
  const chain = Object.values(chains).find((chain) => chain.id === chainId);
  const explorerUrl =
    chain?.blockExplorers?.default.url ?? extraExplorers[chainId];
  if (explorerUrl) {
    return explorerUrl + `/${type}/${hash}`;
  }
  return "";
};
