#!/usr/bin/env node
// Finds a finished, successful run of the CI workflow (.github/workflows/ci.yml)
// that tested exactly the tree of commit SHA, so a release can skip running
// the whole CI again (release.yml).
//
//   node scripts/ci-passed.mjs SHA [OWNER/NAME]
//
// The repository is OWNER/NAME, else GITHUB_REPOSITORY. GITHUB_TOKEN needs
// `actions: read`; without it only push runs count, as job logs need a
// token. Prints the run's URL when one is found, nothing otherwise; with
// GITHUB_OUTPUT set, also writes `run=<url>` (empty when none) there.
//
// What counts, and why it is the same proof as a fresh CI run:
// - A `push` run on SHA: GitHub checks out SHA itself.
// - A `pull_request` run whose head is SHA: GitHub tests a merge of the head
//   into the base as the base was then. Its checkout log names both
//   ("Merge <head> into <base>"); when that base is an ancestor of SHA, the
//   merge's tree is SHA's tree, so the run tested exactly what is published.
//   Anything else (another base, a log that is gone or reads otherwise)
//   does not count.
// - Only push and pull_request runs: they always run every CI job (a manual
//   run may run a part of it).
// Any doubt (an API error, an unexpected answer) finds no run, and the
// release runs CI itself: the safe side.

import { appendFileSync } from "node:fs";

const sha = process.argv[2];
const token = process.env.GITHUB_TOKEN;
const repo = process.argv[3] ?? process.env.GITHUB_REPOSITORY;
if (!/^[0-9a-f]{40}$/.test(sha ?? "") || !repo) {
  console.error("usage: node scripts/ci-passed.mjs <full commit sha> [owner/name] (see the file's comment)");
  process.exit(2);
}

async function api(path, { text = false } = {}) {
  const response = await fetch(`https://api.github.com/repos/${repo}/${path}`, {
    headers: {
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      Accept: "application/vnd.github+json",
      "User-Agent": "config-editor release (github actions)",
    },
  });
  if (!response.ok) throw new Error(`GET ${path}: ${response.status}`);
  return text ? response.text() : response.json();
}

// Whether commit `ancestor` is `sha` or one of its ancestors.
async function isAncestor(ancestor) {
  if (ancestor === sha) return true;
  const comparison = await api(`compare/${ancestor}...${sha}?per_page=1`);
  return comparison.status === "ahead" || comparison.status === "identical";
}

// Whether pull_request run `run` tested a merge whose tree is SHA's tree.
async function mergeTestedSha(run) {
  const { jobs } = await api(`actions/runs/${run.id}/jobs?per_page=100`);
  // Every job of a run checks out the same merge commit (the event's
  // GITHUB_SHA); one job's log says which.
  const job = jobs.find((job) => job.conclusion === "success");
  if (!job) return false;
  const log = await api(`actions/jobs/${job.id}/logs`, { text: true });
  const merge = log.match(/HEAD is now at [0-9a-f]+ Merge ([0-9a-f]{40}) into ([0-9a-f]{40})/);
  if (!merge) return false;
  const [, head, base] = merge;
  return head === sha && (await isAncestor(base));
}

async function find() {
  const { workflow_runs: runs } = await api(
    `actions/workflows/ci.yml/runs?head_sha=${sha}&status=success&per_page=100`,
  );
  for (const run of runs) {
    if (run.head_sha !== sha || run.status !== "completed" || run.conclusion !== "success") continue;
    if (run.path !== ".github/workflows/ci.yml") continue;
    if (run.event === "push") return run;
    if (run.event === "pull_request" && (await mergeTestedSha(run))) return run;
  }
  return null;
}

let found = null;
try {
  found = await find();
} catch (error) {
  console.error(`::warning::could not look up the CI runs of ${sha} (${error.message}); running CI`);
}
const url = found ? found.html_url : "";
if (found) {
  console.log(url);
  console.error(`CI passed on ${sha} in ${found.event} run ${url}`);
} else {
  console.error(`no successful CI run tested ${sha}`);
}
if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `run=${url}\n`);
