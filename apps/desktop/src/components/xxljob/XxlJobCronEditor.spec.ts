/** @vitest-environment happy-dom */
import { createApp, nextTick } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import XxlJobCronEditor from "./XxlJobCronEditor.vue";

const mocks = vi.hoisted(() => ({ preview: vi.fn() }));
vi.mock("vue-i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/lib/backend/api", () => ({ xxljobNextTriggerTime: mocks.preview }));
vi.mock("@/components/ui/tabs", async () => {
  const { h } = await import("vue");
  const component = { setup: (_: unknown, ctx: any) => () => h("div", ctx.slots.default?.()) };
  return { Tabs: component, TabsList: component, TabsTrigger: component, TabsContent: component };
});
let app: ReturnType<typeof createApp> | undefined;
let host: HTMLDivElement;
afterEach(() => { app?.unmount(); host?.remove(); vi.resetAllMocks(); });
async function mount(expression: string) {
  const apply = vi.fn();
  const cancel = vi.fn();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(XxlJobCronEditor, { expression, connectionId: "xxl", busy: false, onApply: apply, onCancel: cancel });
  app.mount(host);
  await nextTick();
  return { apply, cancel };
}
function button(key: string) {
  return [...host.querySelectorAll("button")].find((element) => element.textContent === key)!;
}
function expressionInput() { return host.querySelector('input:not([type="checkbox"]):not([type="number"])') as HTMLInputElement; }

describe("CRON visual editor", () => {
  it.each([" 0 */5 9 ? JAN MON-FRI ", "0 0 9 L-3 * ?"])("preserves untouched expression %s", async (expression) => {
    const { apply } = await mount(expression);
    expect(expressionInput().value).toBe(expression);
    button("xxljob.cron.apply").click();
    expect(apply).toHaveBeenCalledWith(expression);
  });
  it("keeps edits local and makes day/week mutually exclusive", async () => {
    const { apply, cancel } = await mount("0 0 9 * * ?");
    const weekMode = host.querySelectorAll("select")[5];
    weekMode.value = "lastWeekday";
    weekMode.dispatchEvent(new Event("change", { bubbles: true }));
    await nextTick();
    expect(expressionInput().value).toBe("0 0 9 ? * 1L");
    expect(apply).not.toHaveBeenCalled();
    button("xxljob.cancel").click();
    expect(cancel).toHaveBeenCalledOnce();
    expect(apply).not.toHaveBeenCalled();
  });
  it("ignores an old preview after expression changes", async () => {
    let resolve!: (value: string[]) => void;
    mocks.preview.mockReturnValue(new Promise<string[]>((done) => { resolve = done; }));
    await mount("0 0 9 * * ?");
    button("xxljob.preview").click();
    expect(mocks.preview).toHaveBeenCalledWith({ connectionId: "xxl", scheduleType: "CRON", scheduleConf: "0 0 9 * * ?" });
    expressionInput().value = "0 0 10 * * ?";
    expressionInput().dispatchEvent(new Event("input", { bubbles: true }));
    await nextTick();
    resolve(["outdated-preview"]);
    await Promise.resolve();
    await nextTick();
    expect(host.textContent).not.toContain("outdated-preview");
  });
  it("shows scheduler errors", async () => {
    mocks.preview.mockRejectedValue(new Error("Invalid cron"));
    await mount("0 0 9 * * ?");
    button("xxljob.preview").click();
    await Promise.resolve();
    await nextTick();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("Invalid cron");
  });
});
