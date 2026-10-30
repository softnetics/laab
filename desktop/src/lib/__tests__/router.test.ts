import { describe, expect, it } from "vitest";

// Mirrors router.tsx: LINE note ids must survive the URL untouched.
const parseSearch = (search: string) => Object.fromEntries(new URLSearchParams(search.replace(/^\?/, "")));

describe("router search", () => {
  it("keeps 19-digit note ids exact", () => {
    expect(parseSearch("?tab=notes&post=1179264411713515628")).toEqual({ tab: "notes", post: "1179264411713515628" });
  });
});
