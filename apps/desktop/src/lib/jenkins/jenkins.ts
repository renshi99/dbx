import type { JenkinsJob, JenkinsParameter, JenkinsParameterValue } from "@/types/jenkins";

export function isCheckboxParameter(parameter: JenkinsParameter): boolean {
  return parameter._class === "com.cwctravel.hudson.plugins.extended_choice_parameter.ExtendedChoiceParameterDefinition" && parameter.type === "PT_CHECKBOX";
}
export function parameterDefault(parameter: JenkinsParameter): JenkinsParameterValue {
  const value = parameter.defaultParameterValue?.value;
  if (isCheckboxParameter(parameter)) return Array.isArray(value) ? [...value] : [];
  return value ?? (parameterKind(parameter) === "BooleanParameterDefinition" ? false : parameter.choices?.[0] || "");
}

export function parameterKind(parameter: JenkinsParameter): string {
  return (parameter._class || parameter.type || "").split(".").pop() || "";
}
export function parametersFor(job?: JenkinsJob): JenkinsParameter[] {
  return job?.property?.flatMap((p) => p.parameterDefinitions || []) || [];
}
export function supportedParameters(job?: JenkinsJob): boolean {
  return !job?.parameterError && parametersFor(job).every((p) => isCheckboxParameter(p)
    ? !!job?.parameterRevision && !!p.choices?.length
    : ["StringParameterDefinition", "TextParameterDefinition", "BooleanParameterDefinition", "ChoiceParameterDefinition"].includes(parameterKind(p)));
}
export function isFolder(job: JenkinsJob): boolean {
  return /(?:Folder|MultiBranchProject|OrganizationFolder)$/.test(job._class);
}
export function jobUrl(base: string, path: string[], number?: number): string {
  const url = new URL(base);
  url.pathname = `${url.pathname.replace(/\/$/, "")}/${path.map((part) => `job/${encodeURIComponent(part)}`).join("/")}${number ? `/${number}` : ""}/`;
  return url.toString();
}
export function trimLog(text: string, limit = 5 * 1024 * 1024): { text: string; truncated: boolean } {
  const bytes = new TextEncoder().encode(text);
  if (bytes.length <= limit) return { text, truncated: false };
  let start = bytes.length - limit;
  while (start < bytes.length && (bytes[start] & 0xc0) === 0x80) start++;
  return { text: new TextDecoder().decode(bytes.subarray(start)), truncated: true };
}
