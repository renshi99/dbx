export interface JenkinsConfig {
  serverAddr: string;
  tlsSkipVerify: boolean;
}
export type JenkinsParameterValue = string | boolean | string[];
export interface JenkinsRequest {
  connectionId: string;
  path?: string[];
  number?: number;
  queueId?: number;
  page?: number;
  start?: number;
  parameters?: Record<string, JenkinsParameterValue>;
  parameterRevision?: string;
}
export interface JenkinsConnectionInfo {
  version?: string;
  anonymous: boolean;
  url: string;
}
export interface JenkinsParameter {
  name: string;
  type?: string;
  _class?: string;
  description?: string;
  choices?: string[];
  defaultParameterValue?: { value?: JenkinsParameterValue } | null;
}
export interface JenkinsBuild {
  number: number;
  result?: string | null;
  building: boolean;
  timestamp?: number;
  duration?: number;
  displayName?: string;
  queueId?: number;
}
export interface JenkinsJob {
  parameterRevision?: string;
  parameterError?: string;
  name: string;
  displayName?: string;
  _class: string;
  color?: string;
  buildable?: boolean;
  inQueue?: boolean;
  queueItem?: JenkinsQueueItem | null;
  lastBuild?: JenkinsBuild;
  property?: { parameterDefinitions?: JenkinsParameter[] }[];
}
export interface JenkinsQueueItem {
  id: number;
  cancelled?: boolean;
  why?: string;
  executable?: { number: number };
}
export interface JenkinsLog {
  text: string;
  nextStart: number;
  more: boolean;
}
export interface JenkinsTriggerResult {
  accepted: boolean;
  queueId?: number | null;
}
