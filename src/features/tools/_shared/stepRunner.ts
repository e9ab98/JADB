/**
 * Shared "step-runner" primitives used by every multi-step tool card in
 * `ToolsPanel.tsx` (MIUI USB install, developer-option matrix, bug report,
 * etc.). Centralises the state machine + log buffer + step row UI so each
 * new tool only has to describe its `Step[]` instead of re-implementing
 * the run/retry/details-toggle loop.
 *
 * Kept dependency-light on purpose: only React + the same adb IPC the rest
 * of the panel already uses, no Zustand, no router. This is deliberate --
 * a tool card is a leaf component that should be disposable if the panel
 * is ever rewritten.
 */
import { useEffect, useRef, useState } from 'react';

/** Per-step status displayed in the tool card. The set is intentional:
 *  "skipped" is rendered when a step's input check determines it has
 *  no work to do (e.g. property already at target), "idle" never gets
 *  shown because every step transitions out of it as soon as a run
 *  starts. */
export type StepStatus = 'pending' | 'running' | 'success' | 'error' | 'skipped';

export type Step = {
  /** Stable id used as React key + log prefix. */
  id: string;
  /** i18n key path under the tool's namespace, e.g.
   *  `tools.miuiUsbInstall.steps.<id>`. The card UI computes the full
   *  path from `labelNamespace` passed to the row view. */
  labelKey: string;
  /** Run the step. Throw or return `ok:false` to mark it as error;
   *  return `{ ok: true, skipped: true }` to render it as a no-op. */
  run: (
    serial: string,
    log: (line: string) => void,
  ) => Promise<{ ok: boolean; skipped?: boolean; detail?: string }>;
  /** When false (default), a failure here aborts the whole script and
   *  marks subsequent steps as 'skipped'. Set true for non-fatal
   *  cleanup steps (e.g. process restart). */
  optional?: boolean;
};

export type StepRow = {
  id: string;
  labelKey: string;
  status: StepStatus;
  detail?: string;
};

export type RunnerResult = null | 'success' | 'failed';

/**
 * `useStepRunner` is the state machine shared by every step-based tool.
 *
 * Why a hook instead of a render-prop component: each tool card has a
 * very different header (icon, badges, subtitle, custom controls like a
 * duration input), and forcing that through a single component leads to
 * 6+ boolean props. A hook + the small `<StepRunnerCard>` wrapper below
 * keeps the header local to the tool while sharing everything else.
 */
export function useStepRunner(steps: Step[], serial: string) {
  const [rows, setRows] = useState<StepRow[]>(() =>
    steps.map((s) => ({ id: s.id, labelKey: s.labelKey, status: 'pending' as const })),
  );
  const [running, setRunning] = useState(false);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [lastResult, setLastResult] = useState<RunnerResult>(null);
  // `detailsOpen` is independent of `running`/result: users almost
  // never want steps + log before the first run, so it starts closed
  // and stays closed until they click the toggle.
  const [detailsOpen, setDetailsOpen] = useState(false);
  // Mirror the steps in a ref so the closures captured by `run()` see
  // the latest list (the i18n-tied `build*Steps(t)` factory rebuilds on
  // locale change; this ref keeps them in sync without re-creating
  // `run` itself).
  const stepsRef = useRef<Step[]>(steps);
  useEffect(() => {
    stepsRef.current = steps;
  }, [steps]);

  function appendLog(line: string) {
    setLogLines((prev) => [...prev, line]);
  }

  /** Patch a single step's status in state. Centralised so step
   *  loops don't drift on partial updates. */
  function patchStep(id: string, status: StepStatus, detail?: string) {
    setRows((prev) =>
      prev.map((r) => {
        if (r.id !== id) return r;
        // exactOptionalPropertyTypes: spread `detail` only when defined so
        // the optional field isn't serialised as `detail: undefined`.
        if (detail !== undefined) return { ...r, status, detail };
        return { ...r, status };
      }),
    );
  }

  async function run(introLine?: string) {
    if (running) return false;
    const current = stepsRef.current;
    setRunning(true);
    setLastResult(null);
    setLogLines([]);
    setRows(current.map((s) => ({ id: s.id, labelKey: s.labelKey, status: 'pending' })));
    if (introLine) appendLog(introLine);

    let allOk = true;
    for (const step of current) {
      patchStep(step.id, 'running');
      try {
        const res = await step.run(serial, appendLog);
        if (res.skipped) {
          patchStep(step.id, 'skipped', res.detail);
        } else if (res.ok) {
          patchStep(step.id, 'success', res.detail);
        } else {
          patchStep(step.id, 'error', res.detail);
          allOk = false;
          break;
        }
      } catch (e) {
        patchStep(step.id, 'error', String(e));
        allOk = false;
        // Skip remaining non-optional steps; keep optionals flagged
        // as skipped so the UI makes clear what didn't run.
        const idx = current.findIndex((s) => s.id === step.id);
        for (let i = idx + 1; i < current.length; i++) {
          const next = current[i];
          if (!next || next.optional) continue;
          patchStep(next.id, 'skipped');
        }
        break;
      }
    }

    setRunning(false);
    setLastResult(allOk ? 'success' : 'failed');
    return allOk;
  }

  // After a run, the step list and/or log are populated; that's the only
  // time we want to show the "Show details" toggle. Initial state has
  // neither so the toggle is hidden -- keeps the card from looking like
  // there's something to discover when there isn't.
  const hasDetails =
    logLines.length > 0 || rows.some((r) => r.status !== 'pending');

  return {
    rows,
    running,
    logLines,
    lastResult,
    detailsOpen,
    setDetailsOpen,
    hasDetails,
    appendLog,
    run,
    patchStep,
    setRunning,
    setLogLines,
    setLastResult,
  };
}

