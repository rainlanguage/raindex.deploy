import { render, cleanup } from "@testing-library/svelte";
import Hash from "../lib/components/Hash.svelte";
import { describe, it, expect, afterEach } from "vitest";
import userEvent from "@testing-library/user-event";
import { scrub } from "../lib/storesGeneric/scrub";

describe("Hash Component", () => {
  afterEach(() => {
    cleanup();
    scrub.set(false);
  });

  it("renders with shortened hash display", () => {
    const { getByText } = render(Hash, {
      props: {
        value: "abcdef1234567890",
        type: 1, // HashType.Wallet
        shorten: true,
        sliceLen: 5,
      },
    });
    expect(getByText("abcde...67890")).toBeInTheDocument();
  });

  it("renders short vault ids in full instead of duplicating them with ellipsis", () => {
    const { getByText, queryByText } = render(Hash, {
      props: {
        value: "0xfab",
        type: 0, // HashType.Identifier
        shorten: true,
        sliceLen: 5,
      },
    });
    expect(getByText("0xfab")).toBeInTheDocument();
    expect(queryByText("0xfab...0xfab")).not.toBeInTheDocument();
  });

  it("does not truncate values that are exactly two slice lengths", () => {
    const { getByText } = render(Hash, {
      props: {
        value: "abcdefghij",
        shorten: true,
        sliceLen: 5,
      },
    });
    expect(getByText("abcdefghij")).toBeInTheDocument();
  });

  it("renders full hash when shorten is false", () => {
    const { getByText } = render(Hash, {
      props: {
        value: "abcdef1234567890",
        type: 1,
        shorten: false,
      },
    });

    expect(getByText("abcdef1234567890")).toBeInTheDocument();
  });

  it("copies hash to clipboard and shows copied message", async () => {
    const user = userEvent.setup();

    const { getByRole, findByText } = render(Hash, {
      props: {
        value: "abcdef1234567890",
        type: 1,
        copyOnClick: true,
      },
    });

    const button = getByRole("button");
    await user.click(button);
    expect(await findByText("Copied to clipboard")).toBeInTheDocument();
    const clipboardText = await navigator.clipboard.readText();
    expect(clipboardText).toBe("abcdef1234567890");
  });

  it("does not copy to clipboard if copyOnClick is false", async () => {
    const user = userEvent.setup();

    const { getByRole } = render(Hash, {
      props: {
        value: "abcdef1234567890",
        type: 1,
        copyOnClick: false,
      },
    });

    const button = getByRole("button");
    await user.click(button);
    const clipboardText = await navigator.clipboard.readText();
    expect(clipboardText).toBe("");
  });

  it("does not mask the displayed value when scrub is off", () => {
    scrub.set(false);
    const { container } = render(Hash, {
      props: { value: "abcdef1234567890", type: 1, shorten: true, sliceLen: 5 },
    });
    expect(
      container.querySelector('[data-testid="sensitive-mask"]'),
    ).toBeNull();
    const content = container.querySelector(
      '[data-testid="sensitive-content"]',
    );
    expect(content).not.toHaveClass("invisible");
  });

  it("masks the displayed value when scrub is on", () => {
    scrub.set(true);
    const { getByText, container } = render(Hash, {
      props: { value: "abcdef1234567890", type: 1, shorten: true, sliceLen: 5 },
    });
    // The shortened value is still rendered (presentational only) ...
    expect(getByText("abcde...67890")).toBeInTheDocument();
    // ... but wrapped in an invisible Sensitive region with a grey mask over it.
    expect(
      container.querySelector('[data-testid="sensitive-mask"]'),
    ).toBeInTheDocument();
    const content = container.querySelector(
      '[data-testid="sensitive-content"]',
    );
    expect(content).toHaveClass("invisible");
  });
});
