/** Handle returned by long-running Tauri commands for UI to subscribe to progress. */
export type TaskHandle = {
  task_id: string;
  kind: string;
};
