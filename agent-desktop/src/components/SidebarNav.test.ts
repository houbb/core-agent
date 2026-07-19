import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import SidebarNav from "./SidebarNav.vue";

describe("SidebarNav", () => {
  it("exposes accessible workspace and Studio destinations", async () => {
    const wrapper = mount(SidebarNav, { props: { active: "console" } });
    const buttons = wrapper.findAll("button");
    expect(buttons).toHaveLength(12);
    expect(buttons[0].attributes("aria-current")).toBe("page");
    await buttons[3].trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual(["trace"]);
  });
});
