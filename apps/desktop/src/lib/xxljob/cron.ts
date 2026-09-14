export const cronFields = ["second", "minute", "hour", "day", "month", "week", "year"] as const;
export type CronField = (typeof cronFields)[number];
export type CronMode = "every" | "values" | "range" | "step" | "unspecified" | "omit" | "last" | "lastWorkday" | "workday" | "lastWeekday" | "nthWeekday";
export interface CronRule {
  mode: CronMode;
  values: number[];
  start: number;
  end: number;
  step: number;
  day: number;
  nth: number;
}
export type CronRules = Record<CronField, CronRule>;
export const cronBounds: Record<CronField, readonly [number, number]> = {
  second: [0, 59], minute: [0, 59], hour: [0, 23], day: [1, 31], month: [1, 12], week: [1, 7], year: [1970, 2099],
};
export function cronModes(field: CronField): CronMode[] {
  const modes: CronMode[] = ["every", "values", "range", "step"];
  if (field === "day") modes.push("unspecified", "last", "lastWorkday", "workday");
  if (field === "week") modes.push("unspecified", "lastWeekday", "nthWeekday");
  if (field === "year") modes.unshift("omit");
  return modes;
}
export function defaultCronRule(field: CronField): CronRule {
  const [min, max] = cronBounds[field];
  return { mode: field === "year" ? "omit" : field === "week" ? "unspecified" : "every", values: [min], start: min, end: max, step: 1, day: min, nth: 1 };
}
export function defaultCronRules(): CronRules {
  const rules = Object.fromEntries(cronFields.map((field) => [field, defaultCronRule(field)])) as CronRules;
  rules.second.mode = "values";
  return rules;
}
export function formatCronRule(field: CronField, rule: CronRule): string | null {
  const [min, max] = cronBounds[field];
  const valid = (n: number) => Number.isInteger(n) && n >= min && n <= max;
  if (!cronModes(field).includes(rule.mode)) return null;
  switch (rule.mode) {
    case "every": return "*";
    case "unspecified": return "?";
    case "omit": return "";
    case "last": return "L";
    case "lastWorkday": return "LW";
    case "values": return rule.values.length && rule.values.every(valid) ? rule.values.join(",") : null;
    case "range": return valid(rule.start) && valid(rule.end) && rule.start <= rule.end ? `${rule.start}-${rule.end}` : null;
    case "step": return valid(rule.start) && valid(rule.end) && rule.start <= rule.end && Number.isInteger(rule.step) && rule.step > 0 && rule.step <= max - min + 1 ? `${rule.start}-${rule.end}/${rule.step}` : null;
    case "workday": return valid(rule.day) ? `${rule.day}W` : null;
    case "lastWeekday": return valid(rule.day) ? `${rule.day}L` : null;
    case "nthWeekday": return valid(rule.day) && Number.isInteger(rule.nth) && rule.nth >= 1 && rule.nth <= 5 ? `${rule.day}#${rule.nth}` : null;
  }
}
export function formatCron(rules: CronRules): string | null {
  if ((rules.day.mode === "unspecified") === (rules.week.mode === "unspecified")) return null;
  const parts = cronFields.map((field) => formatCronRule(field, rules[field]));
  return parts.some((part) => part === null) ? null : parts.filter((part) => part !== "").join(" ");
}
function parseRule(field: CronField, raw: string): CronRule | null {
  const rule = defaultCronRule(field);
  // Named months/weekdays are unambiguous in Quartz. Other combinations stay manual.
  const names = field === "month" ? ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"] : field === "week" ? ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"] : [];
  const text = raw.toUpperCase().replace(/[A-Z]{3}/g, (name) => names.includes(name) ? String(names.indexOf(name) + 1) : name);
  let match: RegExpMatchArray | null;
  if (text === "*") rule.mode = "every";
  else if (text === "?") rule.mode = "unspecified";
  else if (text === "" && field === "year") rule.mode = "omit";
  else if (text === "L" && field === "day") rule.mode = "last";
  else if (text === "L" && field === "week") { rule.mode = "values"; rule.values = [7]; }
  else if (text === "LW") rule.mode = "lastWorkday";
  else if ((match = text.match(/^(\d+)(W|L)$/))) { rule.mode = match[2] === "W" ? "workday" : "lastWeekday"; rule.day = Number(match[1]); }
  else if ((match = text.match(/^(\d+)#([1-5])$/))) { rule.mode = "nthWeekday"; rule.day = Number(match[1]); rule.nth = Number(match[2]); }
  else if (/^\d+(,\d+)*$/.test(text)) { rule.mode = "values"; rule.values = text.split(",").map(Number); }
  else if ((match = text.match(/^(\d+)-(\d+)$/))) { rule.mode = "range"; rule.start = Number(match[1]); rule.end = Number(match[2]); }
  else if ((match = text.match(/^(\*|\d+)(?:-(\d+))?\/(\d+)$/))) {
    if (match[1] === "*" && match[2]) return null;
    rule.mode = "step";
    rule.start = match[1] === "*" ? cronBounds[field][0] : Number(match[1]);
    rule.end = match[2] ? Number(match[2]) : cronBounds[field][1];
    rule.step = Number(match[3]);
  } else return null;
  return formatCronRule(field, rule) === null ? null : rule;
}
export function parseCron(expression: string): CronRules | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 6 && parts.length !== 7) return null;
  const rules = defaultCronRules();
  for (const [index, field] of cronFields.entries()) {
    const rule = parseRule(field, parts[index] ?? "");
    if (!rule) return null;
    rules[field] = rule;
  }
  return formatCron(rules) === null ? null : rules;
}
