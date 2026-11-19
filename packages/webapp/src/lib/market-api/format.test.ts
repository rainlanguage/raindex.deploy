import { describe, expect, it } from "vitest";
import {
  formatPrice,
  formatSpread,
  formatVolume,
  shortAddress,
} from "./format";

describe("market display formatting", () => {
  it("formats API prices for scanning and exposes missing values", () => {
    expect(formatPrice("184.22830357961256795276")).toBe("184.2283");
    expect(formatPrice()).toBe("—");
  });

  it("formats executable spread from bid and ask", () => {
    expect(formatSpread("99", "101")).toBe("2.00%");
    expect(formatSpread(undefined, "101")).toBe("—");
    expect(formatSpread(null, "101")).toBe("—");
    expect(formatSpread("101", "99")).toBe("—");
  });

  it("compacts API volumes and shortens contract addresses", () => {
    expect(formatVolume("112854.021")).toBe("112.854K");
    expect(shortAddress("0x1234567890abcdef")).toBe("0x1234…cdef");
  });
});
