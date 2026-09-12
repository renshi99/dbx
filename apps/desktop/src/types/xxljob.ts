export interface XxlJobViewState {
  view: "jobs" | "executors" | "logs";
  group: number;
  executorPage: number;
  jobPage: number;
  logPage: number;
  jobDesc: string;
  author: string;
  executorHandler: string;
  triggerStatus: number;
  logJobId: number;
  logStatus: number;
  filterTime: string;
}
export interface XxlJobConfig {
  serverAddr: string;
  version: "2.4" | "2.5";
  tlsSkipVerify: boolean;
  executorMode: "automatic" | "manual";
  executors: { id: number; title: string }[];
}
export interface XxlJobConnectionInfo {
  version: string;
  executorMode: "automatic" | "manual";
  url: string;
}
export interface XxlJobExecutor {
  id: number;
  title: string;
  appname?: string;
  addressType?: number;
  addressList?: string;
  registryList?: string[];
  manual?: boolean;
}
export interface XxlJobDraft {
  id: number;
  jobGroup: number;
  jobDesc: string;
  author: string;
  alarmEmail: string;
  scheduleType: string;
  scheduleConf: string;
  misfireStrategy: string;
  executorRouteStrategy: string;
  executorHandler: string;
  executorParam: string;
  executorBlockStrategy: string;
  executorTimeout: number;
  executorFailRetryCount: number;
  glueType: string;
  childJobId: string;
}
export interface XxlJob extends XxlJobDraft {
  triggerStatus: number;
  triggerLastTime?: number;
  triggerNextTime?: number;
}
export interface XxlJobLog {
  id: number;
  jobGroup: number;
  jobId: number;
  triggerTime: string | number;
  triggerCode: number;
  triggerMsg?: string;
  handleTime?: string | number;
  handleCode: number;
  handleMsg?: string;
  executorAddress?: string;
}
export interface XxlJobLogChunk {
  text: string;
  htmlEscaped: boolean;
  nextLine: number;
  end: boolean;
}
export interface XxlJobPage<T> {
  items: T[];
  total: number;
}
export interface XxlJobRequest {
  connectionId: string;
  jobGroup?: number;
  id?: number;
  page?: number;
  jobDesc?: string;
  executorHandler?: string;
  author?: string;
  triggerStatus?: number;
  logStatus?: number;
  filterTime?: string;
  executorParam?: string;
  addressList?: string;
  scheduleType?: string;
  scheduleConf?: string;
  fromLineNum?: number;
  draft?: XxlJobDraft;
  operationId?: string;
}
