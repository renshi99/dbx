<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { cronBounds, cronFields, cronModes, defaultCronRules, formatCron, parseCron, type CronField, type CronMode } from "@/lib/xxljob/cron";
import * as api from "@/lib/backend/api";

const props = defineProps<{ expression: string; connectionId: string; busy: boolean }>();
const emit = defineEmits<{ apply: [string]; cancel: [] }>();
const { t } = useI18n();
const raw = ref(props.expression);
const parsed = parseCron(props.expression);
const rules = ref(parsed ?? defaultCronRules());
const visual = ref(!!parsed || !props.expression.trim());
const changed = ref(!props.expression.trim());
const expression = computed(() => visual.value && changed.value ? formatCron(rules.value) : raw.value);
const times = ref<string[]>([]);
const error = ref("");
const previewing = ref(false);
let revision = 0;
watch(expression, () => { revision++; times.value = []; error.value = ""; previewing.value = false; }, { flush: "sync" });
onBeforeUnmount(() => { revision++; });

function setMode(field: CronField, event: Event) {
  rules.value[field].mode = (event.target as HTMLSelectElement).value as CronMode;
  if (field === "day" || field === "week") {
    const other = field === "day" ? "week" : "day";
    if (rules.value[field].mode !== "unspecified") rules.value[other].mode = "unspecified";
    else if (rules.value[other].mode === "unspecified") rules.value[other].mode = "every";
  }
  changed.value = true;
}
function editRaw(value: string | number) {
  raw.value = String(value);
  changed.value = false;
  const result = parseCron(raw.value);
  visual.value = !!result;
  if (result) rules.value = result;
}
function values(field: CronField) {
  const [min, max] = cronBounds[field];
  return Array.from({ length: max - min + 1 }, (_, index) => index + min);
}
function valueLabel(field: CronField, value: number) {
  return field === "week" ? t(`xxljob.cron.weekday${value}`) : String(value);
}
async function preview() {
  if (!expression.value?.trim()) return;
  const current = ++revision;
  previewing.value = true;
  times.value = [];
  error.value = "";
  try {
    const result = await api.xxljobNextTriggerTime({ connectionId: props.connectionId, scheduleType: "CRON", scheduleConf: expression.value });
    if (current === revision) times.value = result.length ? result : [t("xxljob.noNextTime")];
  } catch (e) {
    if (current === revision) error.value = String(e);
  } finally {
    if (current === revision) previewing.value = false;
  }
}
</script>

<template>
  <section class="grid min-w-0 gap-3 rounded-md border bg-muted/20 p-3" :aria-label="t('xxljob.cron.editor')">
    <p class="text-sm font-medium">{{ t("xxljob.cron.editor") }}</p>
    <p v-if="!visual" class="text-sm text-muted-foreground" role="status">{{ t("xxljob.cron.manualHint") }}</p>
    <Tabs v-if="visual" default-value="second" class="min-w-0">
      <TabsList class="flex h-auto flex-wrap">
        <TabsTrigger v-for="field in cronFields" :key="field" :value="field" :disabled="busy">{{ t(`xxljob.cron.${field}`) }}</TabsTrigger>
      </TabsList>
      <TabsContent v-for="field in cronFields" :key="field" :value="field" class="space-y-3">
        <label class="grid gap-1 text-sm">
          {{ t("xxljob.cron.rule") }}
          <select :value="rules[field].mode" :disabled="busy" class="h-9 rounded-md border bg-background px-2" @change="setMode(field, $event)">
            <option v-for="mode in cronModes(field)" :key="mode" :value="mode">{{ t(`xxljob.cron.${mode}`) }}</option>
          </select>
        </label>
        <div v-if="rules[field].mode === 'values'" class="grid max-h-40 grid-cols-4 gap-2 overflow-y-auto sm:grid-cols-7">
          <label v-for="value in values(field)" :key="value" class="flex items-center gap-1 text-sm">
            <input v-model="rules[field].values" type="checkbox" :value="value" :disabled="busy" @change="changed = true" />{{ valueLabel(field, value) }}
          </label>
        </div>
        <div v-if="rules[field].mode === 'range' || rules[field].mode === 'step'" class="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <label v-for="edge in (['start', 'end'] as const)" :key="edge" class="grid gap-1 text-sm">
            {{ t(`xxljob.cron.${edge}`) }}
            <select v-if="field === 'week'" v-model.number="rules[field][edge]" :disabled="busy" class="h-8 rounded-md border bg-background px-2" @change="changed = true">
              <option v-for="value in values(field)" :key="value" :value="value">{{ valueLabel(field, value) }}</option>
            </select>
            <Input v-else v-model.number="rules[field][edge]" type="number" :min="cronBounds[field][0]" :max="cronBounds[field][1]" step="1" :disabled="busy" @update:model-value="changed = true" />
          </label>
          <label v-if="rules[field].mode === 'step'" class="grid gap-1 text-sm">
            {{ t("xxljob.cron.stepSize") }}
            <Input v-model.number="rules[field].step" type="number" min="1" :max="cronBounds[field][1] - cronBounds[field][0] + 1" step="1" :disabled="busy" @update:model-value="changed = true" />
          </label>
        </div>
        <div v-if="['workday', 'lastWeekday', 'nthWeekday'].includes(rules[field].mode)" class="grid grid-cols-2 gap-2">
          <label class="grid gap-1 text-sm">
            {{ t(`xxljob.cron.${field}`) }}
            <select v-model.number="rules[field].day" :disabled="busy" class="h-8 rounded-md border bg-background px-2" @change="changed = true">
              <option v-for="value in values(field)" :key="value" :value="value">{{ valueLabel(field, value) }}</option>
            </select>
          </label>
          <label v-if="rules[field].mode === 'nthWeekday'" class="grid gap-1 text-sm">
            {{ t("xxljob.cron.occurrence") }}
            <Input v-model.number="rules[field].nth" type="number" min="1" max="5" step="1" :disabled="busy" @update:model-value="changed = true" />
          </label>
        </div>
        <p v-if="field === 'day' || field === 'week'" class="text-xs text-muted-foreground">{{ t("xxljob.cron.dayWeekHint") }}</p>
      </TabsContent>
    </Tabs>
    <label class="grid gap-1 text-sm">
      {{ t("xxljob.cron.expression") }}
      <Input :model-value="expression ?? ''" :disabled="busy" class="font-mono" @update:model-value="editRaw" />
    </label>
    <p v-if="expression === null" role="alert" class="text-sm text-destructive">{{ t("xxljob.cron.invalidRule") }}</p>
    <p class="text-xs text-muted-foreground">{{ t("xxljob.cron.timezoneHint") }}</p>
    <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
    <div aria-live="polite"><p v-for="time in times" :key="time" class="font-mono text-xs">{{ time }}</p></div>
    <div class="flex flex-wrap gap-2">
      <Button type="button" variant="outline" :disabled="busy || previewing || !expression?.trim()" @click="preview">{{ t("xxljob.preview") }}</Button>
      <Button type="button" :disabled="busy || !expression?.trim()" @click="expression && emit('apply', expression)">{{ t("xxljob.cron.apply") }}</Button>
      <Button type="button" variant="outline" :disabled="busy" @click="emit('cancel')">{{ t("xxljob.cancel") }}</Button>
    </div>
  </section>
</template>
