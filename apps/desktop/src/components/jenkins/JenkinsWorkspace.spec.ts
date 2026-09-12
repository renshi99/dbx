/** @vitest-environment happy-dom */
import { createApp, h, KeepAlive, nextTick, ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import JenkinsWorkspace from "./JenkinsWorkspace.vue";

const mocks = vi.hoisted(() => ({
  test: vi.fn(),
  list: vi.fn(),
  job: vi.fn(),
  history: vi.fn(),
  build: vi.fn(),
  log: vi.fn(),
  trigger: vi.fn(),
  queue: vi.fn(),
  cancel: vi.fn(),
  stop: vi.fn(),
  guard: vi.fn(),
}));
vi.mock("vue-i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/stores/connectionStore", () => ({ useConnectionStore: () => ({ getConfig: () => ({ id: "ci", name: "CI" }), ensureConnected: async () => {} }) }));
vi.mock("@/lib/database/productionExecutionGuard", () => ({ executeWithProductionContextGuard: mocks.guard }));
vi.mock("@/lib/backend/api", () => ({
  jenkinsTestConnection: mocks.test,
  jenkinsListJobs: mocks.list,
  jenkinsGetJob: mocks.job,
  jenkinsListBuilds: mocks.history,
  jenkinsGetBuild: mocks.build,
  jenkinsGetBuildLog: mocks.log,
  jenkinsTriggerBuild: mocks.trigger,
  jenkinsGetQueueItem: mocks.queue,
  jenkinsCancelQueueItem: mocks.cancel,
  jenkinsStopBuild: mocks.stop,
}));

let app: ReturnType<typeof createApp> | undefined;
let host: HTMLDivElement;
async function flush() {
  for (let i = 0; i < 12; i++) await Promise.resolve();
  await nextTick();
}
function click(text: string) {
  const button = [...host.querySelectorAll("button")].find((el) => el.textContent?.includes(text));
  expect(button).toBeTruthy();
  button!.click();
}
async function mount() {
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(JenkinsWorkspace, { connectionId: "ci" });
  app.mount(host);
  await flush();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  window.confirm = vi.fn(() => true);
  Object.defineProperty(document, "hidden", { configurable: true, value: false });
  mocks.test.mockResolvedValue({ anonymous: false, url: "https://ci.example.com/jenkins/", version: "2.x" });
  mocks.list.mockResolvedValue({ jobs: [{ name: "compile", _class: "hudson.model.FreeStyleProject", buildable: true }] });
  mocks.job.mockResolvedValue({ name: "compile", _class: "hudson.model.FreeStyleProject", buildable: true, property: [] });
  mocks.history.mockResolvedValue({ builds: [{ number: 1, result: "SUCCESS", building: false }] });
  mocks.build.mockResolvedValue({ number: 1, building: false, result: "SUCCESS" });
  mocks.log.mockResolvedValue({ text: "<script>unsafe()</script>\n", nextStart: 26, more: false });
  mocks.guard.mockImplementation(({ execute }) => execute());
});
afterEach(() => {
  app?.unmount();
  host?.remove();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Jenkins workspace", () => {
  it("allows cancelling an existing queued job without triggering it again", async () => {
    mocks.job.mockResolvedValue({ name: "compile", _class: "hudson.model.FreeStyleProject", buildable: true, queueItem: { id: 77, why: "Waiting for executor" } });
    mocks.cancel.mockResolvedValue(undefined);
    await mount();
    click("compile");
    await flush();
    click("jenkins.cancel");
    await flush();
    expect(mocks.cancel).toHaveBeenCalledWith(expect.objectContaining({ connectionId: "ci", queueId: 77 }));
    expect(mocks.trigger).not.toHaveBeenCalled();
  });
  it("pauses while its cached tab is inactive and resumes after activation", async () => {
    mocks.history.mockResolvedValue({ builds: [{ number: 1, building: true }] });
    mocks.build.mockResolvedValue({ number: 1, building: true });
    const shown = ref(true);
    host = document.createElement("div");
    document.body.append(host);
    app = createApp({ setup: () => () => h(KeepAlive, null, { default: () => (shown.value ? h(JenkinsWorkspace, { connectionId: "ci" }) : null) }) });
    app.mount(host);
    await flush();
    click("compile");
    await flush();
    shown.value = false;
    await flush();
    await vi.advanceTimersByTimeAsync(6000);
    expect(mocks.build).not.toHaveBeenCalled();
    shown.value = true;
    await flush();
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.build).toHaveBeenCalledOnce();
  });
  it("renders logs as text and blocks anonymous writes", async () => {
    mocks.test.mockResolvedValue({ anonymous: true, url: "https://ci.example.com/" });
    await mount();
    click("compile");
    await flush();
    expect(host.querySelector("pre")?.textContent).toContain("<script>unsafe()");
    expect(host.querySelector("script")).toBeNull();
    const trigger = [...host.querySelectorAll("button")].find((b) => b.textContent?.includes("jenkins.trigger"));
    expect(trigger?.disabled).toBe(true);
    trigger?.click();
    expect(mocks.trigger).not.toHaveBeenCalled();
  });
  it("tracks an accepted queue item into its build and stops polling on unmount", async () => {
    mocks.trigger.mockResolvedValue({ accepted: true, queueId: 9 });
    mocks.queue.mockResolvedValue({ id: 9, executable: { number: 2 } });
    mocks.build.mockResolvedValue({ number: 2, building: true });
    await mount();
    click("compile");
    await flush();
    click("jenkins.trigger");
    await flush();
    expect(mocks.guard).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(2000);
    await flush();
    expect(mocks.build).toHaveBeenCalledWith(expect.objectContaining({ number: 2 }));
    expect(host.querySelector("pre")?.parentElement?.textContent).toContain("#2");
    app!.unmount();
    app = undefined;
    const calls = mocks.build.mock.calls.length;
    await vi.advanceTimersByTimeAsync(6000);
    expect(mocks.build).toHaveBeenCalledTimes(calls);
  });
  it("does not retry a trigger after an uncertain network result", async () => {
    mocks.trigger.mockRejectedValue(new Error("JENKINS_UNCONFIRMED"));
    await mount();
    click("compile");
    await flush();
    click("jenkins.trigger");
    await flush();
    await vi.advanceTimersByTimeAsync(6000);
    expect(mocks.trigger).toHaveBeenCalledOnce();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("JENKINS_UNCONFIRMED");
  });
  it("does not append an old build's delayed log after changing selection", async () => {
    let finish: (value: unknown) => void = () => {};
    mocks.history.mockResolvedValue({
      builds: [
        { number: 1, building: false },
        { number: 2, building: false },
      ],
    });
    mocks.log
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      )
      .mockResolvedValue({ text: "build two", nextStart: 9, more: false });
    await mount();
    click("compile");
    await flush();
    click("#2");
    await flush();
    finish({ text: "build one", nextStart: 9, more: false });
    await flush();
    expect(host.querySelector("pre")?.textContent).toBe("build two");
  });
  it("pauses polling while the document is hidden", async () => {
    mocks.history.mockResolvedValue({ builds: [{ number: 1, building: true }] });
    await mount();
    click("compile");
    await flush();
    Object.defineProperty(document, "hidden", { configurable: true, value: true });
    await vi.advanceTimersByTimeAsync(6000);
    expect(mocks.build).not.toHaveBeenCalled();
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.build).toHaveBeenCalledOnce();
  });
});
