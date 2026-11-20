import { describe, it, expect, vi, beforeEach } from "vitest";
import { loadRegistryUrl } from "../lib/services/loadRegistryUrl";
import { RegistryManager } from "../lib/providers/registry/RegistryManager";
import { initialRegistry } from "../__fixtures__/RegistryManager";
import { DotrainRegistry } from "@rainlanguage/raindex";

// Mock dependencies. validate/new are spied so the tests can assert that
// loadRegistryUrl performs no registry fetch of its own. Fetching and
// validation are owned by the post-reload +layout.ts (via DotrainRegistry.new);
// loadRegistryUrl only persists the URL and reloads.
vi.mock("@rainlanguage/raindex", () => ({
  DotrainRegistry: {
    validate: vi.fn(),
    new: vi.fn(),
  },
}));

describe("loadRegistryUrl", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    const originalLocation = window.location;
    const mockLocation = { ...originalLocation, reload: vi.fn() };
    Object.defineProperty(window, "location", {
      writable: true,
      value: mockLocation,
    });
  });

  it("should throw an error if no URL is provided", async () => {
    const mockRegistryManager = initialRegistry as RegistryManager;
    await expect(loadRegistryUrl("", mockRegistryManager)).rejects.toThrow(
      "No URL provided",
    );
  });

  it("should throw an error if no registry manager is provided", async () => {
    await expect(
      loadRegistryUrl(
        "https://example.com/registry",
        null as unknown as RegistryManager,
      ),
    ).rejects.toThrow("Registry manager is required");
  });

  it("should set the registry and reload the page", async () => {
    const testUrl = "https://example.com/registry";
    const mockRegistryManager = initialRegistry as RegistryManager;

    await loadRegistryUrl(testUrl, mockRegistryManager);

    expect(mockRegistryManager.setRegistry).toHaveBeenCalledWith(testUrl);
    expect(window.location.reload).toHaveBeenCalled();
  });

  it("should NOT fetch/validate the registry itself (avoid double-fetch from GitHub)", async () => {
    const testUrl = "https://example.com/registry";
    const mockRegistryManager = initialRegistry as RegistryManager;

    await loadRegistryUrl(testUrl, mockRegistryManager);

    // loadRegistryUrl does not fetch or validate the registry/settings: that
    // is owned by the post-reload +layout.ts via DotrainRegistry.new. So
    // neither validate nor new is called from here.
    expect(DotrainRegistry.validate).not.toHaveBeenCalled();
    expect(DotrainRegistry.new).not.toHaveBeenCalled();
  });

  it("should reload only after persisting the registry (ordering)", async () => {
    const testUrl = "https://example.com/registry";
    const calls: string[] = [];
    const mockRegistryManager = {
      setRegistry: vi.fn(() => {
        calls.push("setRegistry");
      }),
    } as unknown as RegistryManager;
    (window.location.reload as ReturnType<typeof vi.fn>).mockImplementation(
      () => {
        calls.push("reload");
      },
    );

    await loadRegistryUrl(testUrl, mockRegistryManager);

    expect(calls).toEqual(["setRegistry", "reload"]);
  });

  it("should rethrow as an Error when setRegistry throws an Error", async () => {
    const testUrl = "https://example.com/registry";
    const mockRegistryManager = {
      setRegistry: vi.fn(() => {
        throw new Error("Failed to save to localStorage");
      }),
    } as unknown as RegistryManager;

    await expect(loadRegistryUrl(testUrl, mockRegistryManager)).rejects.toThrow(
      "Failed to save to localStorage",
    );

    expect(window.location.reload).not.toHaveBeenCalled();
  });

  it("should map a non-Error throw to a default message", async () => {
    const testUrl = "https://example.com/registry";
    const mockRegistryManager = {
      setRegistry: vi.fn(() => {
        throw "String error";
      }),
    } as unknown as RegistryManager;

    await expect(loadRegistryUrl(testUrl, mockRegistryManager)).rejects.toThrow(
      "Failed to update registry URL",
    );

    expect(window.location.reload).not.toHaveBeenCalled();
  });
});
