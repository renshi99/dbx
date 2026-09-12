import { describe, expect, it, vi } from "vitest";
import { isFolder, jobUrl, parametersFor, supportedParameters, trimLog } from "./jenkins";
import { createJenkinsApi } from "@/lib/backend/jenkins-api";
import { quickConnectionOpenTarget } from "@/lib/connection/connectionOpenTarget";
import type { JenkinsJob } from "@/types/jenkins";

describe("Jenkins job navigation and parameters", () => {
  it("preserves proxy prefixes and encodes each path segment", () => {
    expect(jobUrl("https://ci.example.com/jenkins", ["team space", "feature/a"], 12)).toBe("https://ci.example.com/jenkins/job/team%20space/job/feature%2Fa/12/");
    expect(jobUrl("https://ci.example.com/jenkins/", ["feature%2Fa"])).toContain("feature%252Fa");
  });
  it("recognizes containers without treating Pipeline jobs as folders", () => {
    expect(isFolder({ name: "branches", _class: "org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject" })).toBe(true);
    expect(isFolder({ name: "pipeline", _class: "org.jenkinsci.plugins.workflow.job.WorkflowJob" })).toBe(false);
  });
  it("rejects unsupported plugin parameters while retaining definitions", () => {
    const job: JenkinsJob = { name: "build", _class: "hudson.model.FreeStyleProject", property: [{ parameterDefinitions: [{ name: "TARGET", _class: "hudson.model.ChoiceParameterDefinition", choices: ["dev", "prod"] }] }] };
    expect(supportedParameters(job)).toBe(true);
    job.property!.push({ parameterDefinitions: [{ name: "DYNAMIC", _class: "org.biouno.unochoice.ChoiceParameter" }] });
    expect(parametersFor(job)).toHaveLength(2);
    expect(supportedParameters(job)).toBe(false);
  });
  it("opens Jenkins and keeps the Nacos quick-open behavior", () => {
    expect(quickConnectionOpenTarget({ db_type: "jenkins" })).toEqual({ kind: "jenkins" });
    expect(quickConnectionOpenTarget({ db_type: "nacos" })).toEqual({ kind: "nacos-admin" });
  });
});

describe("Jenkins incremental logs", () => {
  it("bounds retained UTF-8 bytes without splitting a character", () => {
    const result = trimLog("before\n构建完成\n", 11);
    expect(result.truncated).toBe(true);
    expect(new TextEncoder().encode(result.text).length).toBeLessThanOrEqual(11);
    expect(result.text).not.toContain("�");
    expect(result.text.endsWith("完成\n")).toBe(true);
  });
  it("retains log text literally, including HTML", () => {
    expect(trimLog("<script>alert(1)</script>")).toEqual({ text: "<script>alert(1)</script>", truncated: false });
  });
});

it("uses identical operation payloads for either backend transport", async () => {
  const send = vi.fn().mockResolvedValue({ accepted: true, queueId: 42 });
  const api = createJenkinsApi(send);
  const request = { connectionId: "ci", path: ["folder", "job"], parameters: { FLAG: false } };
  expect(await api.jenkinsTriggerBuild(request)).toEqual({ accepted: true, queueId: 42 });
  expect(send).toHaveBeenCalledExactlyOnceWith("triggerBuild", request);
});
