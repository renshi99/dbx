import type { XxlJobConfig, XxlJobDraft, XxlJobLogChunk } from "@/types/xxljob";
export const newXxlJobConfig = (): XxlJobConfig => ({ serverAddr: "http://127.0.0.1:8080/xxl-job-admin", version: "2.5", tlsSkipVerify: false, executorMode: "automatic", executors: [] });
export function normalizeXxlJobConfig(cfg: XxlJobConfig): XxlJobConfig {
  const url = new URL(cfg.serverAddr.trim());
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw new Error("XXL-JOB: invalid HTTP(S) URL");
  if (!["2.4", "2.5"].includes(cfg.version)) throw new Error("XXL-JOB: select version 2.4.x or 2.5.x");
  if (!["automatic", "manual"].includes(cfg.executorMode)) throw new Error("XXL-JOB: invalid executor mode");
  if (cfg.executorMode === "manual" && (!cfg.executors.length || cfg.executors.length > 100 || cfg.executors.some((e) => !Number.isSafeInteger(e.id) || e.id <= 0 || e.id > 2147483647) || new Set(cfg.executors.map((e) => e.id)).size !== cfg.executors.length))
    throw new Error("XXL-JOB: provide 1–100 unique positive executor IDs");
  return { ...cfg, serverAddr: url.toString().replace(/\/$/, ""), executors: cfg.executorMode === "manual" ? cfg.executors : [] };
}
export function parseExecutorScopes(text: string): XxlJobConfig["executors"] {
  return text
    .split(/\r?\n/)
    .filter((line) => line.trim())
    .map((line) => {
      const match = line.trim().match(/^(\d+)(?:\s+(.+))?$/);
      if (!match) throw new Error("XXL-JOB: one executor per line: ID optional-name");
      return { id: Number(match[1]), title: match[2]?.trim() || "" };
    });
}
export const newJobDraft = (jobGroup: number): XxlJobDraft => ({
  id: 0,
  jobGroup,
  jobDesc: "",
  author: "",
  alarmEmail: "",
  scheduleType: "CRON",
  scheduleConf: "",
  misfireStrategy: "DO_NOTHING",
  executorRouteStrategy: "FIRST",
  executorHandler: "",
  executorParam: "",
  executorBlockStrategy: "SERIAL_EXECUTION",
  executorTimeout: 0,
  executorFailRetryCount: 0,
  glueType: "BEAN",
  childJobId: "",
});
export function jobDraft(job: XxlJobDraft): XxlJobDraft {
  const draft = newJobDraft(job.jobGroup);
  for (const key of Object.keys(draft) as (keyof XxlJobDraft)[]) if (job[key] !== undefined) Object.assign(draft, { [key]: job[key] });
  return draft;
}
export const strategyOptions = {
  scheduleType: ["NONE", "CRON", "FIX_RATE"],
  misfireStrategy: ["DO_NOTHING", "FIRE_ONCE_NOW"],
  executorRouteStrategy: ["FIRST", "LAST", "ROUND", "RANDOM", "CONSISTENT_HASH", "LEAST_FREQUENTLY_USED", "LEAST_RECENTLY_USED", "FAILOVER", "BUSYOVER", "SHARDING_BROADCAST"],
  executorBlockStrategy: ["SERIAL_EXECUTION", "DISCARD_LATER", "COVER_EARLY"],
};
export function preservedOptions(options: readonly string[], value: string): string[] {
  return options.includes(value) ? [...options] : [value, ...options];
}
// Decode exactly once without parsing remote content as a DOM tree.
export function decodeLogText(chunk: Pick<XxlJobLogChunk, "text" | "htmlEscaped">): string {
  if (!chunk.htmlEscaped) return chunk.text;
  const named: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: "\u00a0" };
  return chunk.text.replace(/&(#x[\da-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/gi, (raw, entity: string) => {
    if (!entity.startsWith("#")) return named[entity.toLowerCase()] ?? raw;
    const code = entity[1].toLowerCase() === "x" ? parseInt(entity.slice(2), 16) : parseInt(entity.slice(1), 10);
    return code > 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff) ? String.fromCodePoint(code) : raw;
  });
}
export function appendLog(previous: string, chunk: XxlJobLogChunk, limit = 5 * 1024 * 1024): { text: string; truncated: boolean } {
  const text = previous + decodeLogText(chunk);
  const bytes = new TextEncoder().encode(text);
  if (bytes.length <= limit) return { text, truncated: false };
  let start = bytes.length - limit;
  while (start < bytes.length && (bytes[start] & 0xc0) === 0x80) start++;
  return { text: new TextDecoder().decode(bytes.subarray(start)), truncated: true };
}
