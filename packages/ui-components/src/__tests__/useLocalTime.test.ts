import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { get } from "svelte/store";
import { useLocalTime } from "../lib/storesGeneric/useLocalTime";

describe("useLocalTime store", () => {
  beforeEach(() => {
    localStorage.clear();
    useLocalTime.set(false);
  });

  afterEach(() => {
    localStorage.clear();
    useLocalTime.set(false);
  });

  it("defaults to false (UTC)", () => {
    expect(get(useLocalTime)).toBe(false);
  });

  it("persists true to localStorage", () => {
    useLocalTime.set(true);
    expect(localStorage.getItem("settings.useLocalTime")).toBe("true");
    expect(get(useLocalTime)).toBe(true);
  });
});
