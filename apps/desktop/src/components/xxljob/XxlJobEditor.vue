<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from "@/components/ui/dialog";
import { jobDraft, newJobDraft, preservedOptions, strategyOptions } from "@/lib/xxljob/xxljob";
import * as api from "@/lib/backend/api";
import type { XxlJobDraft } from "@/types/xxljob";
const props = defineProps<{ open: boolean; job?: XxlJobDraft; jobGroup: number; connectionId: string; busy: boolean; serverError?: string }>();
const emit = defineEmits<{ "update:open": [boolean]; save: [XxlJobDraft] }>();
const { t } = useI18n();
const draft = ref(newJobDraft(props.jobGroup));
const times = ref<string[]>([]);
const error = ref("");
const previewing = ref(false);
let revision = 0;
const strings = ["jobDesc", "author", "alarmEmail", "executorHandler", "scheduleConf", "childJobId"] as const;
const strategies = ["scheduleType", "misfireStrategy", "executorRouteStrategy", "executorBlockStrategy"] as const;
watch(
  () => props.open,
  (open) => {
    revision++;
    if (open) {
      draft.value = props.job ? jobDraft(props.job) : newJobDraft(props.jobGroup);
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
    previewing.value = false;
  },
);
function save() {
  const d = draft.value;
  if (!d.jobDesc.trim() || !d.author.trim() || d.jobGroup <= 0 || (d.glueType === "BEAN" && !d.executorHandler.trim()) || !Number.isInteger(d.executorTimeout) || d.executorTimeout < 0 || !Number.isInteger(d.executorFailRetryCount) || d.executorFailRetryCount < 0) {
    error.value = t("xxljob.invalidForm");
    return;
  }
  error.value = "";
  emit("save", jobDraft(d));
}
async function preview() {
  const current = ++revision;
  previewing.value = true;
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
        <div>
          <Button type="button" variant="outline" :disabled="busy || previewing" @click="preview">{{ t("xxljob.preview") }}</Button>
          <p v-for="time in times" :key="time" class="mt-1 font-mono text-xs">{{ time }}</p>
        </div>
        <DialogFooter
          ><Button type="button" variant="outline" :disabled="busy" @click="emit('update:open', false)">{{ t("xxljob.cancel") }}</Button
          ><Button type="submit" :disabled="busy">{{ t("xxljob.save") }}</Button></DialogFooter
        >
      </form>
    </DialogContent>
  </Dialog>
</template>
