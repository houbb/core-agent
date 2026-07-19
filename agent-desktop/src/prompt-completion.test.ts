import { describe, expect, it } from "vitest";
import {
  applyPromptCompletion,
  commandCompletions,
  contextCompletions,
  mentionQueryAtCursor,
} from "./prompt-completion";

describe("desktop prompt completion", () => {
  it("uses command definitions supplied by the shared backend registry", () => {
    const completions = commandCompletions("/pl", [
      { name: "plan", usage: "/plan <request>", summary: "Create a read-only plan" },
      { name: "help", usage: "/help", summary: "Show commands" },
    ]);
    expect(completions).toHaveLength(1);
    expect(applyPromptCompletion("/pl", completions[0])).toBe("/plan ");
  });

  it("finds the active mention and completes without submitting the prompt", () => {
    const input = "Review @src";
    expect(mentionQueryAtCursor(input)?.query).toBe("src");
    const [completion] = contextCompletions(input, ["src/main.rs"]);
    expect(applyPromptCompletion(input, completion)).toBe("Review @src/main.rs ");
  });

  it("quotes paths with spaces and ignores embedded email-like at signs", () => {
    const [completion] = contextCompletions("Use @des", ["design docs/guide.md"]);
    expect(applyPromptCompletion("Use @des", completion)).toBe('Use @"design docs/guide.md" ');
    expect(mentionQueryAtCursor("mail@example.com")).toBeUndefined();
  });
});
