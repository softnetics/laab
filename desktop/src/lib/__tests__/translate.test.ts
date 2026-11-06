import { describe, expect, it } from "vitest";
import { targetOf, translateTarget } from "../translate";

describe("translate target", () => {
  it("maps locales to LINE's language codes", () => {
    expect(targetOf("th-TH")).toBe("th");
    expect(targetOf("en_US")).toBe("en");
    expect(targetOf("ja")).toBe("ja");
    expect(targetOf("zh-Hans-CN")).toBe("zh-CN");
    expect(targetOf("zh-Hant-TW")).toBe("zh-TW");
    expect(targetOf("zh-HK")).toBe("zh-TW");
    expect(targetOf("zh")).toBe("zh-CN");
    expect(targetOf("sv-SE")).toBeNull();
    expect(targetOf("")).toBeNull();
  });

  it("takes the first language LINE can translate into, else Thai", () => {
    expect(translateTarget(["sv-SE", "en-GB", "th-TH"])).toBe("en");
    expect(translateTarget(["sv-SE"])).toBe("th");
    expect(translateTarget([])).toBe("th");
  });
});
