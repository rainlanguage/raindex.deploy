import { cachedWritableStore } from "./cachedWritableStore";

/**
 * Global preference for displaying timestamps in the browser's local timezone
 * instead of UTC.
 *
 * When `false` (default), all formatted timestamps stay in UTC — matching the
 * historical Raindex webapp behaviour. When `true`, the same values are shown
 * in the user's local timezone.
 *
 * Persisted under `settings.useLocalTime` so the preference survives reloads.
 */
export const useLocalTime = cachedWritableStore<boolean>(
  "settings.useLocalTime",
  false,
  (value) => JSON.stringify(value),
  (serialized) => {
    try {
      return JSON.parse(serialized) === true;
    } catch {
      return false;
    }
  },
);
