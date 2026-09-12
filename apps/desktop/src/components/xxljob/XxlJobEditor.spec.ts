/** @vitest-environment happy-dom */
import { createApp, nextTick } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import XxlJobEditor from "./XxlJobEditor.vue";
import { newJobDraft } from "@/lib/xxljob/xxljob";
const mocks = vi.hoisted(() => ({ preview: vi.fn() }));
vi.mock("vue-i18n", () => ({ useI18n: () => ({ t: (k: string) => k }) }));
vi.mock("@/lib/backend/api", () => ({ xxljobNextTriggerTime: mocks.preview }));
vi.mock("@/components/ui/dialog", async () => {
  const { h } = await import("vue");
  const component = { setup: (_: unknown, ctx: any) => () => h("div", ctx.slots.default?.()) };
  return { Dialog: component, DialogContent: component, DialogHeader: component, DialogTitle: component, DialogFooter: component, DialogDescription: component };
});
let app: ReturnType<typeof createApp> | undefined;
let host: HTMLDivElement;
afterEach(() => {
  app?.unmount();
  host?.remove();
  vi.clearAllMocks();
});
async function mount(props: Record<string, unknown>) {
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(XxlJobEditor, { open: true, jobGroup: 1, connectionId: "xxl", busy: false, ...props });
  app.mount(host);
  await nextTick();
}
describe("XXL-JOB editor", () => {
  it("hydrates GLUE tasks, preserves unknown strategies and submits only editable fields", async () => {
    const save = vi.fn();
    const job = { ...newJobDraft(1), id: 7, jobDesc: "Script", author: "owner", glueType: "GLUE_SHELL", executorRouteStrategy: "CUSTOM", glueSource: "echo retained" };
    await mount({ job, onSave: save });
    expect([...host.querySelectorAll("option")].some((o) => o.value === "CUSTOM")).toBe(true);
    host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(save).toHaveBeenCalledOnce();
    expect(save.mock.calls[0][0]).toMatchObject({ id: 7, jobGroup: 1, glueType: "GLUE_SHELL", executorRouteStrategy: "CUSTOM" });
    expect(save.mock.calls[0][0]).not.toHaveProperty("glueSource");
    expect(job.glueSource).toBe("echo retained");
  });
  it("shows backend validation errors inside the dialog", async () => {
    await mount({ serverError: "Invalid cron expression" });
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("Invalid cron expression");
  });
  it("validates required fields before submitting", async () => {
    const save = vi.fn();
    await mount({ onSave: save });
    host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await nextTick();
    expect(save).not.toHaveBeenCalled();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("xxljob.invalidForm");
  });
});
