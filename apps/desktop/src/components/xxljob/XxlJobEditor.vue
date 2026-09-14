<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { jobDraft, newJobDraft, preservedOptions, strategyOptions } from "@/lib/xxljob/xxljob";
import * as api from "@/lib/backend/api";
import type { XxlJobDraft } from "@/types/xxljob";
import XxlJobCronEditor from "./XxlJobCronEditor.vue";
const props = defineProps<{ open: boolean; job?: XxlJobDraft; jobGroup: number; connectionId: string; busy: boolean; serverError?: string }>();
const emit = defineEmits<{ "update:open": [boolean]; save: [XxlJobDraft] }>();
const { t } = useI18n();
const draft = ref(newJobDraft(props.jobGroup));
const times = ref<string[]>([]);
const error = ref("");
const previewing = ref(false);
const cronEditing = ref(false);
let scheduleConfigs = new Map<string, string>();
let revision = 0;
const strings = ["jobDesc", "author", "alarmEmail", "executorHandler", "childJobId"] as const;
const strategies = ["misfireStrategy", "executorRouteStrategy", "executorBlockStrategy"] as const;
watch(
  () => props.open,
  (open) => {
    revision++;
    cronEditing.value = false;
    if (open) {
      draft.value = props.job ? jobDraft(props.job) : newJobDraft(props.jobGroup);
      scheduleConfigs = new Map();
      times.value = [];
      error.value = "";
      previewing.value = false;
    }
  },
  { immediate: true },
);
watch(
  () => [draft.value.scheduleType, draft.value.scheduleConf],
  () => {
    revision++;
    times.value = [];
    error.value = "";
    previewing.value = false;
  },
  { flush: "sync" },
);
function changeScheduleType(event: Event) {
  scheduleConfigs.set(draft.value.scheduleType, draft.value.scheduleConf);
  draft.value.scheduleType = (event.target as HTMLSelectElement).value;
  draft.value.scheduleConf = scheduleConfigs.get(draft.value.scheduleType) ?? "";
  cronEditing.value = false;
}
function applyCron(expression: string) {
  draft.value.scheduleConf = expression;
  cronEditing.value = false;
}
function save() {
  if (props.busy || cronEditing.value) return;
  const d = draft.value;
  if (d.scheduleType === "FIX_RATE" && (!/^\d+$/.test(d.scheduleConf) || !Number.isSafeInteger(Number(d.scheduleConf)) || Number(d.scheduleConf) <= 0 || Number(d.scheduleConf) > 2147483647)) {
    error.value = t("xxljob.cron.invalidInterval");
    return;
  }
  if (!d.jobDesc.trim() || !d.author.trim() || d.jobGroup <= 0 || (d.glueType === "BEAN" && !d.executorHandler.trim()) || !Number.isInteger(d.executorTimeout) || d.executorTimeout < 0 || !Number.isInteger(d.executorFailRetryCount) || d.executorFailRetryCount < 0) {
    error.value = t("xxljob.invalidForm");
    return;
  }
  error.value = "";
  emit("save", jobDraft(d));
}
async function preview() {
  if (props.busy || draft.value.scheduleType === "NONE") return;
  const current = ++revision;
  previewing.value = true;
  times.value = [];
  error.value = "";
  try {
    const result = await api.xxljobNextTriggerTime({ connectionId: props.connectionId, scheduleType: draft.value.scheduleType, scheduleConf: draft.value.scheduleConf });
    if (current === revision) times.value = result.length ? result : [t("xxljob.noNextTime")];
  } catch (e) {
    if (current === revision) error.value = String(e);
  } finally {
    if (current === revision) previewing.value = false;
  }
}
</script>
<template>
  <Dialog
    :open="open"
    @update:open="
      (value) => {
        if (!busy) emit('update:open', value);
      }
    "
  >
    <DialogContent class="max-h-[85vh] max-w-3xl overflow-y-auto">
      <DialogHeader
        ><DialogTitle>{{ job ? t("xxljob.edit") : t("xxljob.add") }}</DialogTitle
        ><DialogDescription>{{ t("xxljob.glueHint") }}</DialogDescription></DialogHeader
      >
      <form class="grid gap-4" @submit.prevent="save">
        <p v-if="error || serverError" role="alert" class="text-sm text-destructive">{{ error || serverError }}</p>
        <fieldset :disabled="busy" class="grid gap-4 sm:grid-cols-2">
          <label class="grid gap-1 text-sm">{{ t("xxljob.executor") }}<Input :model-value="String(draft.jobGroup)" disabled /></label>
          <label class="grid gap-1 text-sm">{{ t("xxljob.glueType") }}<Input :model-value="draft.glueType" disabled /></label>
          <label v-for="key in strings" :key="key" class="grid gap-1 text-sm">{{ t("xxljob." + key) }}<Input v-model="draft[key]" :required="key === 'jobDesc' || key === 'author' || (key === 'executorHandler' && draft.glueType === 'BEAN')" /></label>
          <div class="grid min-w-0 gap-3 rounded-md border p-3 sm:col-span-2">
            <label class="grid gap-1 text-sm">
              {{ t("xxljob.scheduleType") }}
              <select :value="draft.scheduleType" class="h-9 rounded-md border bg-background px-2" @change="changeScheduleType">
                <option v-for="option in preservedOptions(strategyOptions.scheduleType, draft.scheduleType)" :key="option" :value="option">{{ strategyOptions.scheduleType.includes(option) ? t('xxljob.cron.type' + option) : option }}</option>
              </select>
            </label>
            <template v-if="draft.scheduleType !== 'NONE'">
              <label class="grid gap-1 text-sm">
                {{ t("xxljob.scheduleConf") }}
                <Input v-if="draft.scheduleType === 'FIX_RATE'" :model-value="draft.scheduleConf" type="number" min="1" max="2147483647" step="1" required @update:model-value="draft.scheduleConf = String($event)" />
                <Input v-else v-model="draft.scheduleConf" :disabled="cronEditing" class="font-mono" />
              </label>
              <p v-if="draft.scheduleType === 'FIX_RATE'" class="text-xs text-muted-foreground">{{ t("xxljob.cron.secondsHint") }}</p>
              <Button v-if="draft.scheduleType === 'CRON' && !cronEditing" type="button" variant="outline" class="justify-self-start" @click="cronEditing = true">{{ t("xxljob.cron.editor") }}</Button>
              <XxlJobCronEditor v-if="cronEditing && draft.scheduleType === 'CRON'" :expression="draft.scheduleConf" :connection-id="connectionId" :busy="busy" @apply="applyCron" @cancel="cronEditing = false" />
            </template>
          </div>
          <label v-for="key in strategies" :key="key" class="grid gap-1 text-sm"
            >{{ t("xxljob." + key)
            }}<select v-model="draft[key]" class="h-9 rounded-md border bg-background px-2">
              <option v-for="option in preservedOptions(strategyOptions[key], draft[key])" :key="option" :value="option">{{ option }}</option>
            </select></label
          >
          <label class="grid gap-1 text-sm">{{ t("xxljob.executorTimeout") }}<Input v-model.number="draft.executorTimeout" type="number" min="0" max="2147483647" step="1" /></label>
          <label class="grid gap-1 text-sm">{{ t("xxljob.executorFailRetryCount") }}<Input v-model.number="draft.executorFailRetryCount" type="number" min="0" max="2147483647" step="1" /></label>
          <label class="grid gap-1 text-sm sm:col-span-2">{{ t("xxljob.executorParam") }}<textarea v-model="draft.executorParam" class="min-h-24 rounded-md border bg-background p-2 font-mono text-sm" /></label>
        </fieldset>
        <div v-if="draft.scheduleType !== 'NONE' && !cronEditing">
          <Button type="button" variant="outline" :disabled="busy || previewing" @click="preview">{{ t("xxljob.preview") }}</Button>
          <p class="mt-1 text-xs text-muted-foreground">{{ t("xxljob.cron.timezoneHint") }}</p>
          <p v-for="time in times" :key="time" class="mt-1 font-mono text-xs">{{ time }}</p>
        </div>
        <DialogFooter
          ><Button type="button" variant="outline" :disabled="busy" @click="emit('update:open', false)">{{ t("xxljob.cancel") }}</Button
          ><Button type="submit" :disabled="busy || cronEditing">{{ t("xxljob.save") }}</Button></DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
