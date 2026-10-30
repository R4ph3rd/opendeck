import { Button, IconButton } from "@opendeck/design-system";
import { Pause, Play, X } from "lucide-react";
import type { JobKind, JobStage } from "../ipc/bindings";
import { baseName } from "../lib/library";
import { cancelJob, pauseJob, resumeJob } from "../state/libraryActions";
import { isTerminal, useLibraryStore, type JobEntry } from "../state/library";

const KIND_LABEL: Record<JobKind, string> = {
  import_folder: "Import folder",
  import_files: "Import files",
  import_playlist: "Import playlist",
  import_rekordbox_xml: "Import rekordbox XML",
  reanalyze: "Analyse tracks",
};

const STAGE_LABEL: Record<JobStage, string> = {
  discover: "Finding files",
  dedupe: "Checking duplicates",
  tags: "Reading tags",
  decode: "Decoding",
  analyze: "Analysing",
  persist: "Saving",
};

const PERCENT = 100;

const STATE_LABEL: Record<JobEntry["state"], string> = {
  queued: "Queued",
  running: "Running",
  paused: "Paused",
  cancelled: "Cancelled",
  completed: "Done",
  failed: "Failed",
};

/** Share of items finished, `0..1`. */
export function jobFraction(job: JobEntry): number {
  if (job.items_total === 0) {
    return job.state === "completed" ? 1 : 0;
  }
  const finished = job.items_done + job.items_duplicate + job.items_failed;
  return Math.min(1, finished / job.items_total);
}

/** One line of counts, for example `12 of 40 added, 3 duplicates, 1 failed`. */
export function jobCounts(job: JobEntry): string {
  const parts = [`${job.items_done} of ${job.items_total} added`];
  if (job.items_duplicate > 0) {
    parts.push(`${job.items_duplicate} duplicate${job.items_duplicate === 1 ? "" : "s"}`);
  }
  if (job.items_failed > 0) {
    parts.push(`${job.items_failed} failed`);
  }
  return parts.join(", ");
}

function JobRow({ job }: { job: JobEntry }) {
  const dismiss = useLibraryStore((s) => s.dismissJob);
  const done = isTerminal(job.state);
  const label = KIND_LABEL[job.kind];
  const stage = job.stage ? STAGE_LABEL[job.stage] : STATE_LABEL[job.state];
  const percent = Math.round(jobFraction(job) * PERCENT);
  return (
    <li className="flex flex-col gap-1 border-b border-border px-3 py-2 last:border-b-0">
      <div className="flex items-center gap-2">
        <span className="truncate text-sm font-medium text-fg">{label}</span>
        <span className="truncate text-xs text-fg-muted">
          {done || job.state === "paused" ? STATE_LABEL[job.state] : stage}
        </span>
        <div className="flex-1" />
        {!done && job.state !== "paused" ? (
          <IconButton size="sm" aria-label={`Pause ${label}`} onClick={() => pauseJob(job.job)}>
            <Pause className="size-3.5" aria-hidden />
          </IconButton>
        ) : null}
        {job.state === "paused" ? (
          <IconButton size="sm" aria-label={`Resume ${label}`} onClick={() => resumeJob(job.job)}>
            <Play className="size-3.5" aria-hidden />
          </IconButton>
        ) : null}
        {done ? (
          <IconButton size="sm" aria-label={`Dismiss ${label}`} onClick={() => dismiss(job.job)}>
            <X className="size-3.5" aria-hidden />
          </IconButton>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Cancel ${label}`}
            onClick={() => cancelJob(job.job)}
          >
            Cancel
          </Button>
        )}
      </div>
      <div
        role="progressbar"
        aria-label={`${label} progress`}
        aria-valuemin={0}
        aria-valuemax={PERCENT}
        aria-valuenow={percent}
        className="h-1 overflow-hidden rounded-sm bg-surface-hover"
      >
        <div
          className="h-full origin-left bg-accent"
          style={{ transform: `scaleX(${jobFraction(job)})` }}
        />
      </div>
      <div className="flex items-center gap-2 text-xs text-fg-muted">
        <span className="shrink-0">{jobCounts(job)}</span>
        {!done && job.current_item ? (
          <span className="truncate" title={job.current_item}>
            {baseName(job.current_item)}
          </span>
        ) : null}
        {job.error ? <span className="truncate text-danger">{job.error}</span> : null}
      </div>
    </li>
  );
}

/** Jobs of the import queue with stage, counts, the current file and pause or cancel. */
export function ImportPanel() {
  const jobs = useLibraryStore((s) => s.jobs);
  const dismissFinished = useLibraryStore((s) => s.dismissFinished);
  if (jobs.length === 0) {
    return null;
  }
  const finished = jobs.some((j) => isTerminal(j.state));
  return (
    <section
      aria-label="Imports"
      className="flex max-h-48 shrink-0 flex-col border-t border-border bg-bg-subtle"
    >
      <header className="flex h-7 shrink-0 items-center justify-between px-3">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-fg-muted">Imports</h3>
        {finished ? (
          <Button size="sm" variant="ghost" onClick={dismissFinished}>
            Clear finished
          </Button>
        ) : null}
      </header>
      <ul className="min-h-0 flex-1 overflow-auto">
        {jobs.map((job) => (
          <JobRow key={job.job} job={job} />
        ))}
      </ul>
    </section>
  );
}
