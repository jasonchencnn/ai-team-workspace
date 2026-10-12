import { describe, expect, it } from "vitest";

describe("unit test layer", () => {
  it("is independently discoverable", () => {
    expect("unit").toBe("unit");
  });
});
