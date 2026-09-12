/** @vitest-environment happy-dom */
import { createApp, nextTick, reactive } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import XxlJobWorkspace from "./XxlJobWorkspace.vue";
import { newJobDraft } from "@/lib/xxljob/xxljob";
const mocks = vi.hoisted(() => ({ test: vi.fn(), executors: vi.fn(), jobs: vi.fn(), logs: vi.fn(), read: vi.fn(), cancel: vi.fn(), start: vi.fn(), stop: vi.fn(), trigger: vi.fn(), readonly: false, query: null as any }));
vi.mock("vue-i18n", () => ({ useI18n: () => ({ t: (k: string) => k }) }));
vi.mock("@/stores/connectionStore", () => ({ useConnectionStore: () => ({ getConfig: () => ({ id: "xxl", read_only: mocks.readonly }) }) }));
vi.mock("@/stores/queryStore", () => ({ useQueryStore: () => mocks.query }));
vi.mock("@/lib/database/readOnlyWriteAccess", () => ({ connectionIsEffectivelyReadOnly: (c: any) => c.read_only }));
vi.mock("@/lib/database/productionExecutionGuard", () => ({ executeWithProductionContextGuard: ({ execute }: any) => execute() }));
vi.mock("@/components/editor/DangerConfirmDialog.vue", () => ({ default: { render: () => null } }));
vi.mock("./XxlJobEditor.vue", () => ({ default: { render: () => null } }));
vi.mock("@/lib/backend/api", () => ({
  xxljobTestConnection: mocks.test,
  xxljobListExecutors: mocks.executors,
  xxljobListJobs: mocks.jobs,
  xxljobListLogs: mocks.logs,
  xxljobReadLog: mocks.read,
  xxljobCancelLog: mocks.cancel,
  xxljobStartJob: mocks.start,
  xxljobStopJob: mocks.stop,
  xxljobTriggerJob: mocks.trigger,
}));
let app: ReturnType<typeof createApp> | undefined;
let host: HTMLDivElement;
async function flush() {
  for (let i = 0; i < 25; i++) await Promise.resolve();
  await nextTick();
}
async function mount() {
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(XxlJobWorkspace, { connectionId: "xxl", tabId: "tab" });
  app.mount(host);
  await flush();
}
function button(key: string, root: ParentNode = host) {
  const b = [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === "xxljob." + key);
  expect(b).toBeTruthy();
  return b!;
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  mocks.readonly = false;
  mocks.query = reactive({ activeTabId: "tab", tabs: [{ id: "tab" }] });
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  mocks.test.mockResolvedValue({ version: "2.5", executorMode: "automatic" });
  mocks.executors.mockResolvedValue({
    items: [
      { id: 1, title: "Example" },
      { id: 2, title: "Other" },
    ],
    total: 2,
  });
  mocks.jobs.mockResolvedValue({ items: [{ ...newJobDraft(1), id: 7, jobDesc: "Example job", executorHandler: "handler", executorParam: "saved parameters", triggerStatus: 0 }], total: 1 });
  mocks.logs.mockResolvedValue({ items: [{ id: 10, jobGroup: 1, jobId: 7, triggerTime: 1, triggerCode: 200, handleCode: 0 }], total: 1 });
  mocks.read.mockResolvedValue({ text: "&lt;img src=x onerror=alert(1)&gt;\n", htmlEscaped: true, nextLine: 2, end: false });
  mocks.cancel.mockResolvedValue(undefined);
  mocks.start.mockResolvedValue(null);
  mocks.trigger.mockResolvedValue({ accepted: true });
});
afterEach(() => {
  app?.unmount();
  app = undefined;
  host?.remove();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
describe("XXL-JOB workspace", () => {
  it("shows native tasks, starts schedules and blocks writes on read-only connections", async () => {
    mocks.readonly = true;
    await mount();
    expect(host.textContent).toContain("Example job");
    expect(button("add").disabled).toBe(true);
    expect(button("start").disabled).toBe(true);
    button("start").click();
    await flush();
    expect(mocks.start).not.toHaveBeenCalled();
    expect(button("logs").disabled).toBe(false);
  });
  it("starts a task once and refreshes after success", async () => {
    await mount();
    button("start").click();
    await flush();
    expect(mocks.start).toHaveBeenCalledOnce();
    expect(mocks.start).toHaveBeenCalledWith({ connectionId: "xxl", jobGroup: 1, id: 7 });
  });
  it("renders logs as text, advances line cursors, pauses with a hidden tab and resumes", async () => {
    await mount();
    button("logs").click();
    await flush();
    button("readLog").click();
    await flush();
    expect(host.querySelector("[data-xxljob-log]")?.textContent).toContain("<img src=x onerror=alert(1)>");
    expect(host.querySelector("[data-xxljob-log] img")).toBeNull();
    await vi.advanceTimersByTimeAsync(2000);
    await flush();
    expect(mocks.read).toHaveBeenLastCalledWith(expect.objectContaining({ fromLineNum: 2 }));
    const count = mocks.read.mock.calls.length;
    mocks.query.activeTabId = "other";
    await flush();
    await vi.advanceTimersByTimeAsync(6000);
    expect(mocks.read).toHaveBeenCalledTimes(count);
    mocks.query.activeTabId = "tab";
    await flush();
    expect(mocks.read).toHaveBeenCalledTimes(count + 1);
  });
  it("cancels in-flight log reads when closed and ignores late responses", async () => {
    let resolve: (v: unknown) => void = () => {};
    mocks.read.mockImplementation(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    await mount();
    button("logs").click();
    await flush();
    button("readLog").click();
    await flush();
    const operationId = mocks.read.mock.calls[0][0].operationId;
    button("closeLog").click();
    await flush();
    expect(mocks.cancel).toHaveBeenCalledWith({ connectionId: "xxl", operationId });
    resolve({ text: "late", htmlEscaped: false, nextLine: 2, end: false });
    await flush();
    expect(host.textContent).not.toContain("late");
    await vi.advanceTimersByTimeAsync(6000);
    expect(mocks.read).toHaveBeenCalledOnce();
  });
  it("preserves filters in the tab and resets pages on executor changes", async () => {
    mocks.query.tabs[0].xxljobViewState = { view: "jobs", group: 1, executorPage: 0, jobPage: 3, logPage: 2, jobDesc: "scheduled", author: "", executorHandler: "", triggerStatus: -1, logJobId: 0, logStatus: 0, filterTime: "" };
    await mount();
    expect(mocks.jobs).toHaveBeenCalledWith(expect.objectContaining({ page: 3, jobDesc: "scheduled" }));
    const select = host.querySelector("select")!;
    select.value = "2";
    select.dispatchEvent(new Event("change"));
    await flush();
    expect(mocks.jobs).toHaveBeenLastCalledWith(expect.objectContaining({ jobGroup: 2, page: 0 }));
    expect(mocks.query.tabs[0].xxljobViewState.jobDesc).toBe("scheduled");
  });
  it("keeps permission failures visible and retries initialization only on explicit refresh", async () => {
    mocks.test.mockRejectedValue(new Error("XXLJOB_PERMISSION"));
    await mount();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("XXLJOB_PERMISSION");
    expect(mocks.jobs).not.toHaveBeenCalled();
    button("refresh").click();
    await flush();
    expect(mocks.test).toHaveBeenCalledTimes(2);
  });
});
