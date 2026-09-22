import { describe, expect, it } from "vitest";

describe("integration test layer", () => {
  it("is independently discoverable", () => {
    expect("integration").toBe("integration");
  });
});
