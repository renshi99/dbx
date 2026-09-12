<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import DangerConfirmDialog from "@/components/editor/DangerConfirmDialog.vue";
import XxlJobEditor from "./XxlJobEditor.vue";
import * as api from "@/lib/backend/api";
import { useConnectionStore } from "@/stores/connectionStore";
import { useQueryStore } from "@/stores/queryStore";
import { executeWithProductionContextGuard } from "@/lib/database/productionExecutionGuard";
import { connectionIsEffectivelyReadOnly } from "@/lib/database/readOnlyWriteAccess";
import { appendLog } from "@/lib/xxljob/xxljob";
import type { XxlJob, XxlJobDraft, XxlJobExecutor, XxlJobLog, XxlJobRequest, XxlJobViewState } from "@/types/xxljob";

const props = defineProps<{ connectionId: string; tabId: string }>();
const { t } = useI18n();
const connections = useConnectionStore();
const query = useQueryStore();
const tab = () => query.tabs.find((item) => item.id === props.tabId);
const state = reactive<XxlJobViewState>({ view: "jobs", group: 0, executorPage: 0, jobPage: 0, logPage: 0, jobDesc: "", author: "", executorHandler: "", triggerStatus: -1, logJobId: 0, logStatus: 0, filterTime: "", ...tab()?.xxljobViewState });
watch(
  state,
  () => {
    const item = tab();
    if (item) item.xxljobViewState = { ...state };
  },
  { deep: true },
);
const views = ["jobs", "executors", "logs"] as const;
const executors = ref<XxlJobExecutor[]>([]);
const executorTotal = ref(0);
const jobs = ref<XxlJob[]>([]);
const jobTotal = ref(0);
const logs = ref<XxlJobLog[]>([]);
const logTotal = ref(0);
const loading = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const initialized = ref(false);
const editorOpen = ref(false);
const edited = ref<XxlJobDraft>();
const pendingDelete = ref<XxlJob>();
const triggerTarget = ref<XxlJob>();
const triggerParam = ref("");
const triggerAddresses = ref("");
const selectedLog = ref<XxlJobLog>();
const logText = ref("");
const logTruncated = ref(false);
const logEnd = ref(false);
const logPaused = ref(false);
const line = ref(1);
const visible = ref(document.visibilityState !== "hidden");
const componentActive = ref(true);
const readOnly = computed(() => connectionIsEffectivelyReadOnly(connections.getConfig(props.connectionId)));
const canPoll = computed(() => initialized.value && visible.value && componentActive.value && query.activeTabId === props.tabId && state.view === "logs");
const controlsDisabled = computed(() => loading.value || busy.value);
let alive = true;
let generation = 0;
let logGeneration = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let inFlight: string | undefined;
const req = (): XxlJobRequest => ({ connectionId: props.connectionId, jobGroup: state.group });
const isCurrent = (id: number) => alive && id === generation;
const textError = (e: unknown) => (e instanceof Error ? e.message : String(e));

async function loadExecutors() {
  const result = await api.xxljobListExecutors({ connectionId: props.connectionId, page: state.executorPage });
  if (!alive) return;
  executors.value = result.items;
  executorTotal.value = result.total;
  if (!state.group && result.items.length) state.group = result.items[0].id;
}
async function refresh() {
  const current = ++generation;
  loading.value = true;
  error.value = "";
  try {
    if (state.view === "executors") await loadExecutors();
    else if (state.group > 0 && state.view === "jobs") {
      const result = await api.xxljobListJobs({ ...req(), page: state.jobPage, jobDesc: state.jobDesc, author: state.author, executorHandler: state.executorHandler, triggerStatus: state.triggerStatus });
      if (isCurrent(current)) {
        jobs.value = result.items;
        jobTotal.value = result.total;
      }
    } else if (state.group > 0) {
      const result = await api.xxljobListLogs({ ...req(), page: state.logPage, id: state.logJobId, logStatus: state.logStatus, filterTime: state.filterTime });
      if (isCurrent(current)) {
        logs.value = result.items;
        logTotal.value = result.total;
      }
    }
  } catch (e) {
    if (isCurrent(current)) {
      error.value = textError(e);
      if (state.view === "jobs") jobs.value = [];
      else if (state.view === "logs") logs.value = [];
    }
  } finally {
    if (isCurrent(current)) loading.value = false;
  }
}
async function initialize() {
  loading.value = true;
  error.value = "";
  try {
    await api.xxljobTestConnection({ connectionId: props.connectionId });
    await loadExecutors();
    if (!alive) return;
    initialized.value = true;
    await refresh();
  } catch (e) {
    if (alive) error.value = textError(e);
  } finally {
    if (alive) loading.value = false;
  }
}
function refreshCurrent() {
  return initialized.value ? refresh() : initialize();
}
function selectView(view: XxlJobViewState["view"]) {
  if (controlsDisabled.value) return;
  state.view = view;
  void refresh();
}
async function changeExecutorPage(delta: number) {
  if (controlsDisabled.value) return;
  const previous = state.executorPage;
  state.executorPage += delta;
  loading.value = true;
  error.value = "";
  try {
    await loadExecutors();
  } catch (e) {
    state.executorPage = previous;
    error.value = textError(e);
  } finally {
    loading.value = false;
  }
}
watch(
  () => state.group,
  () => {
    stopLog(true);
    jobs.value = [];
    logs.value = [];
    jobTotal.value = 0;
    logTotal.value = 0;
    state.jobPage = 0;
    state.logPage = 0;
    state.logJobId = 0;
    if (initialized.value) void refresh();
  },
);
function search() {
  if (state.view === "jobs") state.jobPage = 0;
  else state.logPage = 0;
  void refresh();
}
function page(delta: number) {
  if (state.view === "jobs") state.jobPage += delta;
  else state.logPage += delta;
  void refresh();
}
function openEditor(job?: XxlJob) {
  edited.value = job;
  editorOpen.value = true;
  error.value = "";
}
async function mutate(label: string, execute: () => Promise<unknown>, done?: () => void) {
  if (busy.value || readOnly.value) return;
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    await executeWithProductionContextGuard({
      connection: connections.getConfig(props.connectionId),
      reviewText: label,
      source: "XXL-JOB",
      execute: async () => {
        await execute();
        if (alive) {
          done?.();
          notice.value = t("xxljob.saved");
          await refresh();
        }
      },
    });
  } catch (e) {
    if (alive) error.value = textError(e);
  } finally {
    if (alive) busy.value = false;
  }
}
function save(draft: XxlJobDraft) {
  void mutate(
    (draft.id ? "Update" : "Add") + " XXL-JOB " + draft.jobDesc,
    () => (draft.id ? api.xxljobUpdateJob : api.xxljobAddJob)({ ...req(), draft }),
    () => {
      editorOpen.value = false;
    },
  );
}
function toggle(job: XxlJob) {
  void mutate((job.triggerStatus ? "Stop schedule " : "Start schedule ") + job.id, () => (job.triggerStatus ? api.xxljobStopJob : api.xxljobStartJob)({ ...req(), id: job.id }));
}
function remove() {
  const job = pendingDelete.value;
  if (!job) return;
  void mutate(
    "Delete XXL-JOB " + job.id + " " + job.jobDesc,
    () => api.xxljobRemoveJob({ ...req(), id: job.id }),
    () => {
      pendingDelete.value = undefined;
    },
  );
}
function prepareTrigger(job: XxlJob) {
  error.value = "";
  triggerTarget.value = job;
  triggerParam.value = job.executorParam;
  triggerAddresses.value = "";
}
async function trigger() {
  const job = triggerTarget.value;
  if (!job) return;
  let submitted = false;
  await mutate(
    "Trigger XXL-JOB " + job.id + "\n" + triggerParam.value + "\n" + triggerAddresses.value,
    () => api.xxljobTriggerJob({ ...req(), id: job.id, executorParam: triggerParam.value, addressList: triggerAddresses.value }),
    () => {
      triggerTarget.value = undefined;
      submitted = true;
    },
  );
  if (submitted) notice.value = t("xxljob.submitted");
}
function showJobLogs(job: XxlJob) {
  state.logJobId = job.id;
  state.logPage = 0;
  selectView("logs");
}

function cancelRead() {
  logGeneration++;
  if (timer) clearTimeout(timer);
  timer = undefined;
  if (inFlight) {
    void api.xxljobCancelLog({ connectionId: props.connectionId, operationId: inFlight }).catch(() => {});
    inFlight = undefined;
  }
}
function stopLog(clear = false) {
  cancelRead();
  logPaused.value = true;
  if (clear) {
    selectedLog.value = undefined;
    logText.value = "";
  }
}
function selectLog(log: XxlJobLog) {
  stopLog();
  selectedLog.value = log;
  logText.value = "";
  logTruncated.value = false;
  logEnd.value = false;
  line.value = 1;
  logPaused.value = false;
  error.value = "";
  void pollLog();
}
async function pollLog() {
  if (!alive || !canPoll.value || !selectedLog.value || logEnd.value || logPaused.value || inFlight) return;
  const current = logGeneration;
  const operationId = crypto.randomUUID();
  inFlight = operationId;
  try {
    const chunk = await api.xxljobReadLog({ ...req(), id: selectedLog.value.id, fromLineNum: line.value, operationId });
    if (!alive || current !== logGeneration) return;
    const result = appendLog(logText.value, chunk);
    logText.value = result.text;
    logTruncated.value ||= result.truncated;
    line.value = chunk.nextLine;
    logEnd.value = chunk.end;
  } catch (e) {
    if (alive && current === logGeneration) {
      error.value = textError(e);
      logPaused.value = true;
    }
  } finally {
    if (inFlight === operationId) inFlight = undefined;
    if (alive && current === logGeneration && canPoll.value && !logPaused.value && !logEnd.value)
      timer = setTimeout(() => {
        void pollLog();
      }, 2000);
  }
}
function resumeLog() {
  error.value = "";
  logPaused.value = false;
  void pollLog();
}
watch(canPoll, (active) => {
  if (!active) cancelRead();
  else if (!logPaused.value) void pollLog();
});
function visibilityChanged() {
  visible.value = document.visibilityState !== "hidden";
}
onMounted(() => {
  document.addEventListener("visibilitychange", visibilityChanged);
  void initialize();
});
onActivated(() => {
  componentActive.value = true;
});
onDeactivated(() => {
  componentActive.value = false;
  cancelRead();
});
onBeforeUnmount(() => {
  alive = false;
  generation++;
  cancelRead();
  document.removeEventListener("visibilitychange", visibilityChanged);
});
const currentPage = computed(() => (state.view === "jobs" ? state.jobPage : state.logPage));
const total = computed(() => (state.view === "jobs" ? jobTotal.value : logTotal.value));
const resultName = (code: number) => t(code === 200 ? "xxljob.success" : code > 0 ? "xxljob.failed" : "xxljob.pending");
const timestamp = (value: string | number | undefined) => (value ? (typeof value === "number" ? new Date(value).toLocaleString() : value) : "—");
</script>

<template>
  <section class="flex h-full min-h-0 flex-col overflow-hidden bg-background" data-xxljob-workspace>
    <header class="flex flex-wrap items-center gap-3 border-b px-4 py-3">
      <strong class="text-sm">XXL-JOB</strong>
      <span v-if="readOnly" class="rounded bg-muted px-2 py-1 text-xs">{{ t("xxljob.readOnly") }}</span>
      <nav class="flex gap-1">
        <Button v-for="view in views" :key="view" size="sm" :variant="state.view === view ? 'secondary' : 'ghost'" :disabled="controlsDisabled" @click="selectView(view)">{{ t("xxljob." + view) }}</Button>
      </nav>
      <Button class="ml-auto" size="sm" variant="outline" :disabled="controlsDisabled" @click="refreshCurrent">{{ loading ? t("xxljob.loading") : t("xxljob.refresh") }}</Button>
    </header>
    <div class="flex flex-wrap items-center gap-2 border-b px-4 py-2">
      <label class="flex items-center gap-2 text-sm"
        >{{ t("xxljob.executor") }}
        <select v-model.number="state.group" class="h-9 min-w-40 rounded-md border bg-background px-2" :disabled="controlsDisabled">
          <option :value="0" disabled>{{ t("xxljob.chooseExecutor") }}</option>
          <option v-if="state.group && !executors.some((e) => e.id === state.group)" :value="state.group">#{{ state.group }}</option>
          <option v-for="e in executors" :key="e.id" :value="e.id">{{ e.title }} (#{{ e.id }})</option>
        </select>
      </label>
      <Button size="sm" variant="ghost" :disabled="controlsDisabled || state.executorPage === 0" @click="changeExecutorPage(-1)">{{ t("xxljob.previous") }}</Button>
      <span class="text-xs text-muted-foreground">{{ t("xxljob.page", { page: state.executorPage + 1, total: executorTotal }) }}</span>
      <Button size="sm" variant="ghost" :disabled="controlsDisabled || (state.executorPage + 1) * 20 >= executorTotal" @click="changeExecutorPage(1)">{{ t("xxljob.next") }}</Button>
    </div>
    <p v-if="error" role="alert" class="whitespace-pre-wrap border-b bg-destructive/5 px-4 py-2 text-sm text-destructive">{{ error }}</p>
    <p v-if="notice" role="status" class="px-4 py-2 text-sm text-muted-foreground">{{ notice }}</p>
    <form v-if="state.view === 'jobs'" class="flex flex-wrap gap-2 border-b p-3" @submit.prevent="search">
      <Input v-model="state.jobDesc" class="w-44" :aria-label="t('xxljob.jobDesc')" :placeholder="t('xxljob.jobDesc')" />
      <Input v-model="state.executorHandler" class="w-36" :aria-label="t('xxljob.executorHandler')" placeholder="Handler" />
      <Input v-model="state.author" class="w-28" :aria-label="t('xxljob.author')" :placeholder="t('xxljob.author')" />
      <select v-model.number="state.triggerStatus" :aria-label="t('xxljob.state')" class="rounded-md border bg-background px-2">
        <option :value="-1">{{ t("xxljob.all") }}</option>
        <option :value="1">{{ t("xxljob.running") }}</option>
        <option :value="0">{{ t("xxljob.stopped") }}</option>
      </select>
      <Button type="submit" variant="outline" :disabled="controlsDisabled || !state.group">{{ t("xxljob.search") }}</Button>
      <Button class="ml-auto" type="button" :disabled="controlsDisabled || readOnly || !state.group" @click="openEditor()">{{ t("xxljob.add") }}</Button>
    </form>
    <form v-if="state.view === 'logs'" class="flex flex-wrap gap-2 border-b p-3" @submit.prevent="search">
      <label class="flex items-center gap-2 text-xs">{{ t("xxljob.jobId") }}<Input v-model.number="state.logJobId" class="w-24" type="number" min="0" max="2147483647" /></label>
      <select v-model.number="state.logStatus" :aria-label="t('xxljob.logStatus')" class="rounded-md border bg-background px-2">
        <option :value="0">{{ t("xxljob.all") }}</option>
        <option :value="1">{{ t("xxljob.success") }}</option>
        <option :value="2">{{ t("xxljob.failed") }}</option>
        <option :value="3">{{ t("xxljob.running") }}</option>
      </select>
      <Input v-model="state.filterTime" class="min-w-64 flex-1" :aria-label="t('xxljob.filterTime')" :placeholder="t('xxljob.timeHint')" />
      <Button type="submit" variant="outline" :disabled="controlsDisabled || !state.group">{{ t("xxljob.search") }}</Button>
    </form>
    <div class="min-h-0 flex-1 overflow-auto">
      <table v-if="state.view === 'jobs'" class="w-full whitespace-nowrap text-left text-sm">
        <thead class="sticky top-0 bg-muted text-xs">
          <tr>
            <th>ID</th>
            <th>{{ t("xxljob.jobDesc") }}</th>
            <th>Handler</th>
            <th>{{ t("xxljob.author") }}</th>
            <th>{{ t("xxljob.scheduleConf") }}</th>
            <th>{{ t("xxljob.state") }}</th>
            <th>{{ t("xxljob.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="job in jobs" :key="job.id">
            <td>{{ job.id }}</td>
            <td class="max-w-64 truncate" :title="job.jobDesc">{{ job.jobDesc }}</td>
            <td>{{ job.executorHandler || job.glueType }}</td>
            <td>{{ job.author }}</td>
            <td>{{ job.scheduleType }} {{ job.scheduleConf }}</td>
            <td>{{ job.triggerStatus ? t("xxljob.running") : t("xxljob.stopped") }}</td>
            <td class="space-x-1">
              <Button size="sm" variant="ghost" :disabled="controlsDisabled || readOnly" @click="openEditor(job)">{{ t("xxljob.edit") }}</Button>
              <Button size="sm" variant="ghost" :disabled="controlsDisabled || readOnly" @click="toggle(job)">{{ job.triggerStatus ? t("xxljob.stop") : t("xxljob.start") }}</Button>
              <Button size="sm" variant="ghost" :disabled="controlsDisabled || readOnly" @click="prepareTrigger(job)">{{ t("xxljob.trigger") }}</Button>
              <Button size="sm" variant="ghost" :disabled="controlsDisabled" @click="showJobLogs(job)">{{ t("xxljob.logs") }}</Button>
              <Button size="sm" variant="ghost" class="text-destructive" :disabled="controlsDisabled || readOnly" @click="pendingDelete = job">{{ t("xxljob.remove") }}</Button>
            </td>
          </tr>
        </tbody>
      </table>
      <table v-else-if="state.view === 'executors'" class="w-full text-left text-sm">
        <thead class="sticky top-0 bg-muted text-xs">
          <tr>
            <th>ID</th>
            <th>{{ t("xxljob.executor") }}</th>
            <th>AppName</th>
            <th>{{ t("xxljob.addresses") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="e in executors" :key="e.id">
            <td>{{ e.id }}</td>
            <td>{{ e.title }}</td>
            <td>{{ e.appname || "—" }}</td>
            <td class="max-w-xl whitespace-pre-wrap break-all">{{ e.manual ? t("xxljob.manualInfo") : e.registryList?.join("\n") || e.addressList || "—" }}</td>
          </tr>
        </tbody>
      </table>
      <table v-else class="w-full whitespace-nowrap text-left text-sm">
        <thead class="sticky top-0 bg-muted text-xs">
          <tr>
            <th>{{ t("xxljob.logId") }}</th>
            <th>{{ t("xxljob.jobId") }}</th>
            <th>{{ t("xxljob.triggerTime") }}</th>
            <th>{{ t("xxljob.triggerResult") }}</th>
            <th>{{ t("xxljob.handleResult") }}</th>
            <th>{{ t("xxljob.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="log in logs" :key="log.id">
            <td>{{ log.id }}</td>
            <td>{{ log.jobId }}</td>
            <td>{{ timestamp(log.triggerTime) }}</td>
            <td>{{ resultName(log.triggerCode) }}</td>
            <td>{{ resultName(log.handleCode) }}</td>
            <td>
              <Button size="sm" variant="ghost" :disabled="controlsDisabled" @click="selectLog(log)">{{ t("xxljob.readLog") }}</Button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-if="!loading && !(state.view === 'jobs' ? jobs.length : state.view === 'logs' ? logs.length : executors.length)" class="p-8 text-center text-sm text-muted-foreground">{{ state.group || state.view === "executors" ? t("xxljob.empty") : t("xxljob.chooseExecutor") }}</p>
    </div>
    <footer v-if="state.view !== 'executors'" class="flex items-center justify-end gap-3 border-t px-4 py-2">
      <Button size="sm" variant="ghost" :disabled="controlsDisabled || currentPage === 0" @click="page(-1)">{{ t("xxljob.previous") }}</Button>
      <span class="text-xs">{{ t("xxljob.page", { page: currentPage + 1, total }) }}</span>
      <Button size="sm" variant="ghost" :disabled="controlsDisabled || (currentPage + 1) * 20 >= total" @click="page(1)">{{ t("xxljob.next") }}</Button>
    </footer>
    <div v-if="selectedLog && state.view === 'logs'" class="flex h-[45%] min-h-40 flex-col border-t">
      <div class="flex items-center gap-3 bg-muted/50 px-3 py-2 text-xs">
        <strong>{{ t("xxljob.log") }} #{{ selectedLog.id }}</strong>
        <span>{{ logEnd ? t("xxljob.logFinished") : logPaused || !canPoll ? t("xxljob.logPaused") : t("xxljob.logLive") }}</span>
        <span v-if="logTruncated">{{ t("xxljob.truncated") }}</span>
        <Button v-if="logPaused && !logEnd" size="sm" variant="ghost" @click="resumeLog">{{ t("xxljob.resume") }}</Button>
        <Button size="sm" variant="ghost" class="ml-auto" @click="stopLog(true)">{{ t("xxljob.closeLog") }}</Button>
      </div>
      <div class="min-h-0 flex-1 overflow-auto p-3 font-mono text-xs">
        <pre class="whitespace-pre-wrap break-all text-muted-foreground">{{ selectedLog.triggerMsg }}{{ selectedLog.handleMsg ? "\n" + selectedLog.handleMsg : "" }}</pre>
        <pre class="mt-2 whitespace-pre-wrap break-all" data-xxljob-log>{{ logText }}</pre>
      </div>
    </div>
    <XxlJobEditor v-model:open="editorOpen" :connection-id="connectionId" :job-group="state.group" :job="edited" :busy="busy" :server-error="error" @save="save" />
    <DangerConfirmDialog
      :open="!!pendingDelete"
      :title="t('xxljob.deleteTitle')"
      :message="t('xxljob.deleteMessage') + (error ? '\n' + error : '')"
      :details-text="pendingDelete ? '#' + pendingDelete.id + ' ' + pendingDelete.jobDesc : ''"
      :confirm-label="t('xxljob.remove')"
      :loading="busy"
      :confirm-disabled="readOnly"
      :close-on-confirm="false"
      @update:open="
        (value) => {
          if (!value && !busy) pendingDelete = undefined;
        }
      "
      @confirm="remove"
    />
    <Dialog
      :open="!!triggerTarget"
      @update:open="
        (value) => {
          if (!value && !busy) triggerTarget = undefined;
        }
      "
      ><DialogContent>
        <DialogHeader
          ><DialogTitle>{{ t("xxljob.trigger") }} #{{ triggerTarget?.id }}</DialogTitle
          ><DialogDescription>{{ t("xxljob.triggerHint") }}</DialogDescription></DialogHeader
        >
        <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
        <label class="grid gap-1 text-sm">{{ t("xxljob.executorParam") }}<textarea v-model="triggerParam" :disabled="busy" class="min-h-28 rounded-md border bg-background p-2 font-mono" /></label>
        <label class="grid gap-1 text-sm">{{ t("xxljob.addressList") }}<Input v-model="triggerAddresses" :disabled="busy" /></label>
        <DialogFooter
          ><Button variant="outline" :disabled="busy" @click="triggerTarget = undefined">{{ t("xxljob.cancel") }}</Button
          ><Button :disabled="busy || readOnly" @click="trigger">{{ t("xxljob.trigger") }}</Button></DialogFooter
        >
      </DialogContent></Dialog
    >
  </section>
</template>
<style scoped>
th,
td {
  padding: 0.6rem 0.8rem;
  border-bottom: 1px solid var(--border);
}
tbody tr:hover {
  background: color-mix(in srgb, var(--muted) 35%, transparent);
}
</style>
