export function formatPrice(value?: string | null): string {
  if (!value) return "—";
  const number = Number(value);
  return Number.isFinite(number)
    ? new Intl.NumberFormat("en-US", {
        maximumSignificantDigits: 8,
        useGrouping: false,
      }).format(number)
    : value;
}

export function formatVolume(value: string): string {
  const number = Number(value);
  return Number.isFinite(number)
    ? new Intl.NumberFormat("en-US", {
        notation: "compact",
        maximumSignificantDigits: 6,
      }).format(number)
    : value;
}

export function formatSpread(
  bestBid?: string | null,
  bestAsk?: string | null,
): string {
  if (!bestBid || !bestAsk) return "—";
  const bid = Number(bestBid);
  const ask = Number(bestAsk);
  if (!Number.isFinite(bid) || !Number.isFinite(ask) || ask < bid) return "—";
  const midpoint = (bid + ask) / 2;
  return midpoint > 0
    ? `${(((ask - bid) / midpoint) * 100).toFixed(2)}%`
    : "—";
}

export function shortAddress(address: string): string {
  return address.length > 12
    ? `${address.slice(0, 6)}…${address.slice(-4)}`
    : address;
}
