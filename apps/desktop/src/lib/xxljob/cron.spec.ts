import { describe, expect, it } from "vitest";
import { defaultCronRules, formatCron, parseCron } from "./cron";

describe("XXL-JOB CRON rules", () => {
  it.each([
    "0 0 12 * * ?",
    "0 0,15,30,45 8-18 ? * 2-6 2026",
    "0 0 9 L * ?",
    "0 0 9 LW * ?",
    "0 0 9 15W * ?",
    "0 0 9 ? * 6L",
    "0 0 9 ? * 2#3",
    "0 0-59/5 0-23/2 ? 1-12/2 2-6 2026-2030",
  ])("round trips supported expression %s", (expression) => {
    const rules = parseCron(expression);
    expect(rules).not.toBeNull();
    expect(formatCron(rules!)).toBe(expression);
  });
  it("interprets named weekdays and open-ended steps using Quartz bounds", () => {
    expect(formatCron(parseCron("0 */5 9 ? JAN MON-FRI")!)).toBe("0 0-59/5 9 ? 1 2-6");
    expect(formatCron(parseCron("0 5/10 9 ? * L")!)).toBe("0 5-59/10 9 ? * 7");
  });
  it.each([
    "* * * * *", "0 0 9 * * *", "0 0 9 ? * ?", "60 0 9 * * ?",
    "0 0 9 L-3 * ?", "0 0 9 ? * 2#6", "0 0 9 ? * 0", "0 0 9 ? * 8",
    "0 */0 9 * * ?", "0 10-5 9 * * ?", "0 0 9 32W * ?", "0 0 9 ? * LW",
    "0 0 9 * * ? 2100", "0 0,10-20 9 * * ?",
  ])("leaves unsupported or invalid expression manual: %s", (expression) => {
    expect(parseCron(expression)).toBeNull();
  });
  it("rejects empty selections and fractional steps", () => {
    const rules = defaultCronRules();
    expect(formatCron(rules)).toBe("0 * * * * ?");
    rules.second.values = [];
    expect(formatCron(rules)).toBeNull();
    rules.second.mode = "step";
    rules.second.step = 1.5;
    expect(formatCron(rules)).toBeNull();
  });
});
