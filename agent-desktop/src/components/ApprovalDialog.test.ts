import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import ApprovalDialog from "./ApprovalDialog.vue";

describe("ApprovalDialog", () => {
  it("shows risk and parameters and emits an explicit one-shot decision", async () => {
    const wrapper = mount(ApprovalDialog, {
      props: {
        request: {
          id: "approval-1",
          sessionId: "session-1",
          tool: "run_command",
          risk: "HIGH",
          reason: "strict mode requires approval",
          parameters: { command: "cargo test" },
        },
      },
    });

    expect(wrapper.text()).toContain("HIGH RISK");
    expect(wrapper.text()).toContain("cargo test");
    await wrapper.get(".button-primary").trigger("click");
    expect(wrapper.emitted("decide")).toEqual([["ALLOW_ONCE"]]);

    await wrapper.setProps({ busy: true });
    expect(wrapper.get(".button-primary").attributes("disabled")).toBeDefined();
    expect(wrapper.get(".button").attributes("disabled")).toBeDefined();
  });
});
