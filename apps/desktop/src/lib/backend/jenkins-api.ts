import type { JenkinsRequest, JenkinsJob, JenkinsBuild, JenkinsConnectionInfo, JenkinsLog, JenkinsQueueItem, JenkinsTriggerResult } from "@/types/jenkins";

export function createJenkinsApi(send: (operation: string, request: JenkinsRequest) => Promise<unknown>) {
  const call = <T>(operation: string, request: JenkinsRequest) => send(operation, request) as Promise<T>;
  return {
    jenkinsTestConnection: (req: JenkinsRequest) => call<JenkinsConnectionInfo>("testConnection", req),
    jenkinsListJobs: (req: JenkinsRequest) => call<{ jobs: JenkinsJob[] }>("listJobs", req),
    jenkinsGetJob: (req: JenkinsRequest) => call<JenkinsJob>("getJob", req),
    jenkinsListBuilds: (req: JenkinsRequest) => call<{ builds: JenkinsBuild[] }>("listBuilds", req),
    jenkinsGetBuild: (req: JenkinsRequest) => call<JenkinsBuild>("getBuild", req),
    jenkinsGetBuildLog: (req: JenkinsRequest) => call<JenkinsLog>("getBuildLog", req),
    jenkinsTriggerBuild: (req: JenkinsRequest) => call<JenkinsTriggerResult>("triggerBuild", req),
    jenkinsGetQueueItem: (req: JenkinsRequest) => call<JenkinsQueueItem>("getQueueItem", req),
    jenkinsCancelQueueItem: (req: JenkinsRequest) => call<void>("cancelQueueItem", req),
    jenkinsStopBuild: (req: JenkinsRequest) => call<void>("stopBuild", req),
  };
}
