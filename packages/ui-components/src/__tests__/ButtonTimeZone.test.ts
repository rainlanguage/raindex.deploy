import { render, cleanup, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, afterEach } from "vitest";
import { writable, get } from "svelte/store";
import ButtonTimeZone from "../lib/components/ButtonTimeZone.svelte";

describe("ButtonTimeZone Component", () => {
  afterEach(cleanup);

  it("toggles useLocalTime on from false when clicked", async () => {
    const useLocalTime = writable(false);
    const { getByTestId } = render(ButtonTimeZone, { props: { useLocalTime } });
    await fireEvent.click(getByTestId("timezone-toggle"));
    expect(get(useLocalTime)).toBe(true);
  });

  it("toggles useLocalTime off from true when clicked", async () => {
    const useLocalTime = writable(true);
    const { getByTestId } = render(ButtonTimeZone, { props: { useLocalTime } });
    await fireEvent.click(getByTestId("timezone-toggle"));
    expect(get(useLocalTime)).toBe(false);
  });

  it("shows UTC label when useLocalTime is false", () => {
    const useLocalTime = writable(false);
    const { getByTestId } = render(ButtonTimeZone, { props: { useLocalTime } });
    expect(getByTestId("timezone-toggle")).toHaveTextContent("UTC");
  });

  it("shows Local label when useLocalTime is true", () => {
    const useLocalTime = writable(true);
    const { getByTestId } = render(ButtonTimeZone, { props: { useLocalTime } });
    expect(getByTestId("timezone-toggle")).toHaveTextContent("Local");
  });

  it("reflects useLocalTime state via aria-pressed", async () => {
    const useLocalTime = writable(false);
    const { getByTestId } = render(ButtonTimeZone, { props: { useLocalTime } });
    const button = getByTestId("timezone-toggle");
    expect(button).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
  });
});
