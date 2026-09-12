<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, Folder, ChevronRight, ArrowLeft, Play, Square, ExternalLink } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import * as api from "@/lib/backend/api";
import { useConnectionStore } from "@/stores/connectionStore";
import { executeWithProductionContextGuard } from "@/lib/database/productionExecutionGuard";
import { isFolder, jobUrl, parameterKind, parametersFor, supportedParameters, trimLog } from "@/lib/jenkins/jenkins";
import type { JenkinsBuild, JenkinsConnectionInfo, JenkinsJob, JenkinsQueueItem, JenkinsRequest } from "@/types/jenkins";

const props = defineProps<{ connectionId: string }>();
const { t } = useI18n();
const connections = useConnectionStore();
const info = ref<JenkinsConnectionInfo>();
const folderPath = ref<string[]>([]);
const selectedPath = ref<string[]>([]);
const jobs = ref<JenkinsJob[]>([]);
const job = ref<JenkinsJob>();
const search = ref("");
const history = ref<JenkinsBuild[]>([]);
const page = ref(0);
const build = ref<JenkinsBuild>();
const queue = ref<JenkinsQueueItem>();
const parameters = ref<Record<string, string | boolean>>({});
const log = ref("");
const truncated = ref(false);
const offset = ref(0);
const moreLog = ref(false);
const loading = ref(false);
const mutating = ref(false);
const error = ref("");
const notice = ref("");
let alive = true;
let active = true;
let generation = 0;
let buildGeneration = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let polling = false;
const logRequests = new Set<number>();
const visibleJobs = computed(() => jobs.value.filter((item) => (item.displayName || item.name).toLocaleLowerCase().includes(search.value.toLocaleLowerCase())));
const definitions = computed(() => parametersFor(job.value));
const canWrite = computed(() => !!info.value && !info.value.anonymous);
const externalUrl = computed(() => (info.value ? jobUrl(info.value.url, selectedPath.value, build.value?.number) : ""));
const request = (): JenkinsRequest => ({ connectionId: props.connectionId, path: [...selectedPath.value] });
const current = (id: number) => alive && generation === id;
const message = (reason: unknown) => (reason instanceof Error ? reason.message : String(reason));

async function loadFolder(path: string[]) {
  const id = ++generation;
  buildGeneration++;
  loading.value = true;
  error.value = "";
  job.value = undefined;
  build.value = undefined;
  queue.value = undefined;
  notice.value = "";
  folderPath.value = [...path];
  selectedPath.value = [];
  search.value = "";
  try {
    const result = await api.jenkinsListJobs({ connectionId: props.connectionId, path });
    if (current(id)) jobs.value = result.jobs || [];
  } catch (reason) {
    if (current(id)) {
      jobs.value = [];
      error.value = message(reason);
    }
  } finally {
    if (current(id)) loading.value = false;
  }
}

async function selectJob(item: JenkinsJob) {
  if (mutating.value) return;
  const path = [...folderPath.value, item.name];
  if (isFolder(item)) return loadFolder(path);
  const id = ++generation;
  buildGeneration++;
  selectedPath.value = path;
  job.value = undefined;
  build.value = undefined;
  queue.value = undefined;
  history.value = [];
  log.value = "";
  error.value = "";
  notice.value = "";
  page.value = 0;
  loading.value = true;
  try {
    const [detail, builds] = await Promise.all([api.jenkinsGetJob(request()), api.jenkinsListBuilds({ ...request(), page: 0 })]);
    if (!current(id)) return;
    job.value = detail;
    queue.value = detail.queueItem || undefined;
    history.value = builds.builds || [];
    parameters.value = Object.fromEntries(parametersFor(detail).map((p) => [p.name, p.defaultParameterValue?.value ?? (parameterKind(p) === "BooleanParameterDefinition" ? false : p.choices?.[0] || "")]));
    if (history.value[0]) await selectBuild(history.value[0]);
  } catch (reason) {
    if (current(id)) error.value = message(reason);
  } finally {
    if (current(id)) loading.value = false;
  }
}

async function loadHistory(nextPage = page.value) {
  const id = generation;
  const result = await api.jenkinsListBuilds({ ...request(), page: nextPage });
  if (!current(id)) return;
  page.value = nextPage;
  history.value = result.builds || [];
}

async function readLog(id: number, buildId: number, req: JenkinsRequest) {
  if (logRequests.has(buildId)) return;
  logRequests.add(buildId);
  try {
    const chunk = await api.jenkinsGetBuildLog({ ...req, start: offset.value });
    if (!current(id) || buildGeneration !== buildId) return;
    const next = trimLog(log.value + chunk.text);
    log.value = next.text;
    truncated.value ||= next.truncated;
    offset.value = chunk.nextStart;
    moreLog.value = chunk.more;
  } finally {
    logRequests.delete(buildId);
  }
}

async function selectBuild(item: JenkinsBuild) {
  const id = generation;
  const buildId = ++buildGeneration;
  build.value = item;
  log.value = "";
  offset.value = 0;
  truncated.value = false;
  moreLog.value = true;
  try {
    await readLog(id, buildId, { ...request(), number: item.number });
  } catch (reason) {
    if (current(id) && buildGeneration === buildId) {
      error.value = message(reason);
      moreLog.value = false;
    }
  }
}

async function poll() {
  if (!alive || !active || polling || loading.value || mutating.value || document.hidden) return;
  const id = generation;
  const buildId = buildGeneration;
  const req = request();
  polling = true;
  try {
    if (queue.value && !queue.value.cancelled && !queue.value.executable) {
      try {
        const result = await api.jenkinsGetQueueItem({ ...req, queueId: queue.value.id });
        if (!current(id)) return;
        queue.value = result;
        if (result.executable) {
          const next = await api.jenkinsGetBuild({ ...req, number: result.executable.number });
          if (!current(id)) return;
          notice.value = "";
          await loadHistory(0);
          if (current(id)) await selectBuild(next);
          return;
        }
      } catch (reason) {
        if (!current(id)) return;
        if (message(reason).includes("JENKINS_NOT_FOUND")) {
          const queueId = queue.value?.id;
          queue.value = undefined;
          await loadHistory(0);
          // A queue item can expire before the next poll. Reconcile by queueId,
          // never assume that the newest build was initiated by this request.
          for (const candidate of history.value.slice(0, 10)) {
            if (!current(id)) return;
            const detail = await api.jenkinsGetBuild({ ...req, number: candidate.number });
            if (!current(id)) return;
            if (detail.queueId === queueId) {
              await selectBuild(detail);
              notice.value = "";
              return;
            }
          }
          if (current(id)) notice.value = t("jenkins.queueUnknown");
        } else throw reason;
      }
    }
    if (build.value && (build.value.building || moreLog.value)) {
      const number = build.value.number;
      const detail = await api.jenkinsGetBuild({ ...req, number });
      if (!current(id) || buildGeneration !== buildId) return;
      const finished = build.value.building && !detail.building;
      build.value = detail;
      await readLog(id, buildId, { ...req, number });
      if (finished && current(id)) await loadHistory();
    }
  } catch (reason) {
    if (current(id)) error.value = message(reason);
  } finally {
    polling = false;
  }
}

function schedule() {
  if (!active || timer !== undefined) return;
  timer = setTimeout(async () => {
    timer = undefined;
    await poll();
    if (alive && active) schedule();
  }, 2000);
}

async function refresh() {
  const id = generation;
  const selected = build.value?.number;
  const req = request();
  error.value = "";
  try {
    await connections.ensureConnected(props.connectionId);
    if (!current(id)) return;
    if (!info.value) {
      const result = await api.jenkinsTestConnection(req);
      if (!current(id)) return;
      info.value = result;
    }
    if (job.value) {
      await loadHistory();
      if (!current(id)) return;
      if (selected) {
        const detail = await api.jenkinsGetBuild({ ...req, number: selected });
        if (current(id)) await selectBuild(detail);
      }
    } else await loadFolder(folderPath.value);
  } catch (reason) {
    if (alive) error.value = message(reason);
  }
}

async function mutate(action: "trigger" | "cancel" | "stop") {
  if (mutating.value || !canWrite.value) return;
  const id = generation;
  const req = { ...request(), number: build.value?.number, queueId: queue.value?.id, parameters: { ...parameters.value } };
  const review = `${t(`jenkins.${action}`)}: ${selectedPath.value.join(" / ")}${action === "stop" ? ` #${req.number}` : ""}`;
  mutating.value = true;
  error.value = "";
  try {
    if (!window.confirm(review)) return;
    await executeWithProductionContextGuard({
      connection: connections.getConfig(props.connectionId),
      database: "",
      source: "Jenkins",
      reviewText: review,
      execute: async () => {
        if (action === "trigger") {
          const result = await api.jenkinsTriggerBuild(req);
          if (!current(id)) return;
          queue.value = result.queueId ? { id: result.queueId } : undefined;
          notice.value = t(result.queueId ? "jenkins.queued" : "jenkins.acceptedUnknown");
          await loadHistory(0);
        } else if (action === "cancel") {
          await api.jenkinsCancelQueueItem(req);
          if (current(id)) notice.value = t("jenkins.cancelRequested");
        } else {
          await api.jenkinsStopBuild(req);
          if (current(id)) notice.value = t("jenkins.stopRequested");
        }
      },
    });
  } catch (reason) {
    if (current(id)) error.value = message(reason);
  } finally {
    mutating.value = false;
  }
}

async function changePage(next: number) {
  loading.value = true;
  try {
    await loadHistory(next);
  } catch (reason) {
    error.value = message(reason);
  } finally {
    loading.value = false;
  }
}
onMounted(async () => {
  schedule();
  try {
    await connections.ensureConnected(props.connectionId);
    if (!alive) return;
    const result = await api.jenkinsTestConnection(request());
    if (alive) {
      info.value = result;
      await loadFolder([]);
    }
  } catch (reason) {
    if (alive) error.value = message(reason);
  }
});
onActivated(() => {
  active = true;
  schedule();
});
onDeactivated(() => {
  active = false;
  clearTimeout(timer);
  timer = undefined;
});
onBeforeUnmount(() => {
  alive = false;
  generation++;
  buildGeneration++;
  clearTimeout(timer);
});
</script>

<template>
  <section class="flex h-full min-h-0 w-full flex-col bg-background" data-testid="jenkins-workspace">
    <header class="flex items-center justify-between border-b px-4 py-3">
      <div>
        <h2 class="font-semibold">
          Jenkins <span class="text-xs font-normal text-muted-foreground">{{ info?.version }}</span>
        </h2>
        <p class="text-xs text-muted-foreground">{{ info?.url }}</p>
      </div>
      <Button variant="outline" size="sm" :disabled="loading || mutating" @click="refresh"><RefreshCw class="mr-2 size-4" />{{ t("jenkins.refresh") }}</Button>
    </header>
    <p v-if="error" role="alert" class="border-b bg-destructive/10 p-3 text-sm text-destructive">{{ error }}</p>
    <p v-if="notice" role="status" class="border-b bg-muted p-3 text-sm">{{ notice }}</p>
    <div class="flex min-h-0 flex-1">
      <aside class="flex w-64 shrink-0 flex-col border-r">
        <div class="space-y-2 border-b p-3">
          <Button variant="ghost" size="sm" :disabled="!folderPath.length || loading || mutating" @click="loadFolder(folderPath.slice(0, -1))"><ArrowLeft class="mr-1 size-4" />{{ t("jenkins.back") }}</Button>
          <p class="break-all text-xs text-muted-foreground">{{ folderPath.join(" / ") || t("jenkins.jobs") }}</p>
          <Input v-model="search" :placeholder="t('jenkins.search')" :aria-label="t('jenkins.search')" />
        </div>
        <div class="flex-1 overflow-auto p-2">
          <button
            v-for="item in visibleJobs"
            :key="item.name"
            class="flex w-full items-center gap-2 rounded px-2 py-2 text-left text-sm hover:bg-muted disabled:opacity-50"
            :class="{ 'bg-muted': selectedPath[selectedPath.length - 1] === item.name }"
            :disabled="loading || mutating"
            @click="selectJob(item)"
          >
            <Folder v-if="isFolder(item)" class="size-4 shrink-0" /><span v-else class="size-2 shrink-0 rounded-full" :class="item.color?.startsWith('red') ? 'bg-red-500' : item.color?.startsWith('blue') ? 'bg-green-500' : 'bg-muted-foreground'" />
            <span class="min-w-0 flex-1 truncate">{{ item.displayName || item.name }}</span
            ><ChevronRight v-if="isFolder(item)" class="size-3" />
          </button>
          <p v-if="!visibleJobs.length" class="p-3 text-xs text-muted-foreground">{{ t(loading ? "jenkins.loading" : "jenkins.empty") }}</p>
        </div>
      </aside>
      <main v-if="job" class="flex min-w-0 flex-1 flex-col overflow-auto p-4">
        <div class="mb-3 flex items-center justify-between gap-3">
          <div>
            <h3 class="font-semibold">{{ job.displayName || job.name }}</h3>
            <p class="text-xs text-muted-foreground">{{ selectedPath.join(" / ") }}</p>
          </div>
          <a :href="externalUrl" target="_blank" rel="noopener noreferrer" class="flex items-center gap-1 text-xs text-primary"><ExternalLink class="size-3" />{{ t("jenkins.open") }}</a>
        </div>
        <div v-if="job.buildable" class="mb-4 space-y-3 rounded border p-3">
          <p v-if="!supportedParameters(job)" class="text-sm text-amber-600">{{ t("jenkins.unsupportedParameters") }}</p>
          <template v-else>
            <label v-for="parameter in definitions" :key="parameter.name" class="grid gap-1 text-sm">
              <span>{{ parameter.name }}</span>
              <input v-if="parameterKind(parameter) === 'BooleanParameterDefinition'" v-model="parameters[parameter.name]" type="checkbox" class="size-4" />
              <select v-else-if="parameterKind(parameter) === 'ChoiceParameterDefinition'" v-model="parameters[parameter.name]" class="rounded border bg-background p-2">
                <option v-for="choice in parameter.choices" :key="choice" :value="choice">{{ choice }}</option>
              </select>
              <textarea v-else-if="parameterKind(parameter) === 'TextParameterDefinition'" :value="String(parameters[parameter.name] ?? '')" @input="parameters[parameter.name] = ($event.target as HTMLTextAreaElement).value" class="rounded border bg-background p-2" rows="3" />
              <input v-else v-model="parameters[parameter.name]" class="rounded border bg-background px-3 py-2" />
            </label>
          </template>
          <Button size="sm" :disabled="!canWrite || mutating || loading || !supportedParameters(job)" @click="mutate('trigger')"><Play class="mr-1 size-4" />{{ t("jenkins.trigger") }}</Button>
          <span v-if="info?.anonymous" class="ml-3 text-xs text-muted-foreground">{{ t("jenkins.anonymous") }}</span>
        </div>
        <div v-if="queue" class="mb-3 flex items-center gap-3 rounded border p-3 text-sm">
          <span>#{{ queue.id }} · {{ queue.cancelled ? t("jenkins.cancelled") : queue.why || t("jenkins.queued") }}</span
          ><Button v-if="!queue.cancelled && !queue.executable" variant="outline" size="sm" :disabled="!canWrite || mutating" @click="mutate('cancel')">{{ t("jenkins.cancel") }}</Button>
        </div>
        <div class="grid min-h-80 flex-1 grid-cols-[180px_minmax(0,1fr)] gap-4">
          <section class="flex min-h-0 flex-col rounded border">
            <h4 class="border-b p-2 text-sm font-medium">{{ t("jenkins.history") }}</h4>
            <div class="min-h-0 flex-1 overflow-auto">
              <button v-for="item in history" :key="item.number" class="flex w-full justify-between gap-2 px-3 py-2 text-xs hover:bg-muted" :class="{ 'bg-muted': build?.number === item.number }" @click="selectBuild(item)">
                <span>#{{ item.number }}</span
                ><span>{{ item.building ? t("jenkins.running") : item.result || "—" }}</span>
              </button>
              <p v-if="!history.length" class="p-3 text-xs text-muted-foreground">{{ t("jenkins.empty") }}</p>
            </div>
            <div class="flex justify-between border-t p-2">
              <Button size="sm" variant="ghost" :disabled="page === 0 || loading" @click="changePage(page - 1)">‹</Button><span class="p-2 text-xs">{{ page + 1 }}</span
              ><Button size="sm" variant="ghost" :disabled="history.length < 50 || loading" @click="changePage(page + 1)">›</Button>
            </div>
          </section>
          <section class="flex min-h-0 min-w-0 flex-col rounded border">
            <div class="flex items-center justify-between border-b p-2 text-sm">
              <span
                >{{ t("jenkins.log") }} <span v-if="build">#{{ build.number }} · {{ build.building ? t("jenkins.running") : build.result }}</span></span
              ><Button v-if="build?.building" variant="outline" size="sm" :disabled="!canWrite || mutating" @click="mutate('stop')"><Square class="mr-1 size-3" />{{ t("jenkins.stop") }}</Button>
            </div>
            <p v-if="truncated" class="p-2 text-xs text-amber-600">{{ t("jenkins.truncated") }}</p>
            <pre class="min-h-64 flex-1 overflow-auto whitespace-pre-wrap break-all bg-muted/30 p-3 font-mono text-xs leading-5">{{ log || t("jenkins.noLog") }}</pre>
          </section>
        </div>
      </main>
      <div v-else class="flex flex-1 items-center justify-center text-sm text-muted-foreground">{{ t(loading ? "jenkins.loading" : "jenkins.selectJob") }}</div>
    </div>
  </section>
</template>
