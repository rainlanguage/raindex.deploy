import { describe, it, expect, vi } from "vitest";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  dateTimestamp,
  formatTimestampSecondsAsLocal,
  promiseTimeout,
  timestampSecondsToUTCTimestamp,
} from "../lib/services/time";

describe("Date and timestamp utilities", () => {
  describe("formatTimestampSecondsAsLocal", () => {
    it("converts timestamp to UTC format by default", () => {
      const result = formatTimestampSecondsAsLocal(BigInt("1672531200")); // Jan 1, 2023 12:00 AM UTC
      expect(result).toBe("01/01/2023 12:00 AM");
    });

    it("converts timestamp to local format when useLocalTime is true", () => {
      // Spawn a fresh process with TZ=America/New_York so local getters are
      // non-UTC, and exercise the exported formatter from time.ts.
      const scriptPath = path.join(
        path.dirname(fileURLToPath(import.meta.url)),
        "formatTimestampLocal.ny.ts",
      );
      const viteNode = path.resolve(
        path.dirname(fileURLToPath(import.meta.url)),
        "../../../../node_modules/.bin/vite-node",
      );
      const result = spawnSync(
        viteNode,
        ["--config", "vite.config.ts", scriptPath],
        {
          encoding: "utf8",
          cwd: path.resolve(
            path.dirname(fileURLToPath(import.meta.url)),
            "../..",
          ),
          env: { ...process.env, TZ: "America/New_York" },
        },
      );
      expect(result.status, result.stderr).toBe(0);
      expect(result.stdout.trim()).toBe("12/31/2022 7:00 PM");
    });
  });

  describe("timestampSecondsToUTCTimestamp", () => {
    it("converts bigint timestamp to UTCTimestamp", () => {
      const result = timestampSecondsToUTCTimestamp(BigInt("1672531200"));
      expect(result).toBe(1672531200);
    });
  });

  describe("promiseTimeout", () => {
    it("resolves when promise resolves before timeout", async () => {
      const testValue = "test";
      const promise = Promise.resolve(testValue);
      const result = await promiseTimeout(promise, 100, new Error("Timeout"));
      expect(result).toBe(testValue);
    });

    it("rejects when promise times out", async () => {
      const promise = new Promise((resolve) => setTimeout(resolve, 200));
      const exception = new Error("Timeout");

      await expect(promiseTimeout(promise, 100, exception)).rejects.toThrow(
        exception,
      );
    });

    it("rejects when original promise rejects", async () => {
      const error = new Error("Original rejection");
      const promise = Promise.reject(error);

      await expect(
        promiseTimeout(promise, 100, new Error("Timeout")),
      ).rejects.toThrow(error);
    });

    it("clears timeout after promise resolution", async () => {
      vi.spyOn(global, "clearTimeout");
      const promise = Promise.resolve("test");
      await promiseTimeout(promise, 100, new Error("Timeout"));

      expect(clearTimeout).toHaveBeenCalled();
    });

    it("clears timeout after promise rejection", async () => {
      vi.spyOn(global, "clearTimeout");
      const promise = Promise.reject(new Error("Original rejection"));

      try {
        await promiseTimeout(promise, 100, new Error("Timeout"));
      } catch {
        // Ignore the error
      }

      expect(clearTimeout).toHaveBeenCalled();
    });
  });

  describe("dateTimestamp", () => {
    it("should get date timestamp in seconds", () => {
      const date = new Date(2022, 1, 16, 17, 32, 11, 168);
      const result = dateTimestamp(date);
      const expected = Math.floor(date.getTime() / 1000);

      expect(result).toEqual(expected);
    });
  });
});
