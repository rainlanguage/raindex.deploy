import { RegistryManager } from "../providers/registry/RegistryManager";

export async function loadRegistryUrl(
  url: string,
  registryManager: RegistryManager,
): Promise<void> {
  if (!url) {
    throw new Error("No URL provided");
  }

  if (!registryManager) {
    throw new Error("Registry manager is required");
  }

  try {
    // Persist the new registry URL and reload. Page-load (+layout.ts) calls
    // DotrainRegistry.new(url), which owns fetching and validating the registry
    // and settings files. This function persists and reloads only; it does not
    // fetch or validate.
    registryManager.setRegistry(url);
    window.location.reload();
  } catch (e) {
    const errorMessage =
      e instanceof Error ? e.message : "Failed to update registry URL";
    throw new Error(errorMessage);
  }
}
