import { describe, expect, it, vi } from "vitest";
import { appendLog, decodeLogText, jobDraft, newJobDraft, newXxlJobConfig, normalizeXxlJobConfig, parseExecutorScopes, preservedOptions } from "./xxljob";
import { createXxlJobApi } from "@/lib/backend/xxljob-api";
describe("XXL-JOB contracts", () => {
  it("normalizes proxy paths, rejects credential URLs and validates ordinary account scopes", () => {
    const cfg = newXxlJobConfig();
    cfg.serverAddr = "https://admin.example/proxy/xxl-job-admin/";
    expect(normalizeXxlJobConfig(cfg).serverAddr).toBe("https://admin.example/proxy/xxl-job-admin");
    for (const serverAddr of ["file:///tmp/test", "https://u:p@host/admin", "https://host/admin?token=x", "https://host/#x"]) expect(() => normalizeXxlJobConfig({ ...cfg, serverAddr })).toThrow();
    const executors = parseExecutorScopes("1 测试\n2");
    expect(normalizeXxlJobConfig({ ...cfg, executorMode: "manual", executors }).executors).toEqual([
      { id: 1, title: "测试" },
      { id: 2, title: "" },
    ]);
    for (const text of ["0 bad", "1 a\n1 b", "2147483648 x", "not-an-id"]) expect(() => normalizeXxlJobConfig({ ...cfg, executorMode: "manual", executors: parseExecutorScopes(text) })).toThrow();
    expect(() => normalizeXxlJobConfig({ ...cfg, executorMode: "manual" })).toThrow();
  });
  it("does not round-trip GLUE source or scheduler state and preserves unknown strategies", () => {
    const original = { ...newJobDraft(1), id: 5, glueType: "GLUE_SHELL", executorRouteStrategy: "CUSTOM", glueSource: "echo hello", triggerStatus: 1 };
    const draft = jobDraft(original);
    expect(draft.glueType).toBe("GLUE_SHELL");
    expect(draft.executorRouteStrategy).toBe("CUSTOM");
    expect(draft).not.toHaveProperty("glueSource");
    expect(draft).not.toHaveProperty("triggerStatus");
    expect(preservedOptions(["FIRST"], "CUSTOM")).toEqual(["CUSTOM", "FIRST"]);
  });
  it("decodes 2.5 escaping exactly once and leaves 2.4 text untouched", () => {
    const text = "&lt;script&gt;&amp;lt;&quot;&#39;&#x1f600;";
    expect(decodeLogText({ text, htmlEscaped: false })).toBe(text);
    expect(decodeLogText({ text, htmlEscaped: true })).toBe("<script>&lt;\"'😀");
  });
  it("caps UTF-8 bytes without splitting a character", () => {
    const result = appendLog("aaaa", { text: "中文😀", htmlEscaped: false, nextLine: 2, end: true }, 8);
    expect(result).toEqual({ text: "文😀", truncated: true });
    expect(new TextEncoder().encode(result.text).length).toBeLessThanOrEqual(8);
  });
  it("uses identical typed payloads for either transport", async () => {
    const send = vi.fn().mockResolvedValue({ accepted: true });
    const api = createXxlJobApi(send);
    await api.xxljobTriggerJob({ connectionId: "a", jobGroup: 1, id: 7, executorParam: "", addressList: "" });
    expect(send).toHaveBeenCalledWith("triggerJob", { connectionId: "a", jobGroup: 1, id: 7, executorParam: "", addressList: "" });
  });
});
