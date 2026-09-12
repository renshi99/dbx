import type { XxlJob, XxlJobConnectionInfo, XxlJobExecutor, XxlJobLog, XxlJobLogChunk, XxlJobPage, XxlJobRequest } from "@/types/xxljob";
export function createXxlJobApi(send: (operation: string, request: XxlJobRequest) => Promise<unknown>) {
  const call = <T>(operation: string, request: XxlJobRequest) => send(operation, request) as Promise<T>;
  return {
    xxljobTestConnection: (req: XxlJobRequest) => call<XxlJobConnectionInfo>("testConnection", req),
    xxljobListExecutors: (req: XxlJobRequest) => call<XxlJobPage<XxlJobExecutor>>("listExecutors", req),
    xxljobListJobs: (req: XxlJobRequest) => call<XxlJobPage<XxlJob>>("listJobs", req),
    xxljobAddJob: (req: XxlJobRequest) => call<string>("addJob", req),
    xxljobUpdateJob: (req: XxlJobRequest) => call<unknown>("updateJob", req),
    xxljobRemoveJob: (req: XxlJobRequest) => call<unknown>("removeJob", req),
    xxljobStartJob: (req: XxlJobRequest) => call<unknown>("startJob", req),
    xxljobStopJob: (req: XxlJobRequest) => call<unknown>("stopJob", req),
    xxljobTriggerJob: (req: XxlJobRequest) => call<{ accepted: boolean }>("triggerJob", req),
    xxljobNextTriggerTime: (req: XxlJobRequest) => call<string[]>("nextTriggerTime", req),
    xxljobListLogs: (req: XxlJobRequest) => call<XxlJobPage<XxlJobLog>>("listLogs", req),
    xxljobReadLog: (req: XxlJobRequest) => call<XxlJobLogChunk>("readLog", req),
    xxljobCancelLog: (req: XxlJobRequest) => call<void>("cancelLog", req),
  };
}
