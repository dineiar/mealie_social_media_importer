---
name: swarm
description: "Fan out N parallel workers, drain them, and return one report. Use for /swarm, 'swarm this', or parallel coverage, races, gauntlets, and exploration."
disable-model-invocation: true
---

# Swarm

## Start

Open a todo list with one entry per phase before launching anything. Use `TodoWrite` when the session offers it. Otherwise keep the list in the reply and update it at each phase.

1. Frame
2. Fan out
3. Aggregate
4. Report

## Phase A: Frame

1. State the done predicate and the artifact or report the swarm must return.
2. Choose the shape. Partition into slices, race N workers on identical briefs, or mix both. For a race or mixed shape, declare `first pass`, `rank all`, or `best-of` before spawning.
3. Pick the worker model from the `swarm workers` line in `~/.claude/rules/pstack-models.md`. Claude Code loads that file into every session, so the line is normally in context. If it isn't, read the file. If the file or that line is missing, use `inherit-parent`. A value such as `sonnet xhigh` means `model: "sonnet"` and `effort: "xhigh"`. For `auto` or `inherit-parent`, omit `model` so the workers run on the parent model. If the `Agent` tool rejects the value, use the default and say so. For a model race, name each arm's model up front. Pick each subagent's effort for the job, per the Subagents section of the **poteto-mode** skill.
4. Give each worker its own writable output when it writes. When workers verify or measure commits, each brief names the exact SHAs. A measurement brief also names the method (sample count, what one sample is, order). The worker records both in its result.

## Phase B: Fan out

Spawn all N workers in one message with `subagent_type: "general-purpose"`, `isolation: "worktree"`, and the step 3 model, left unset for `auto` or `inherit-parent`. Each brief names the branch or commit the worker checks out. Leave out `isolation` only when the worker needs something that exists only in the user's own checkout, such as uncommitted changes.

When a worker must start from a non-default branch, tell it to check out that branch first.

Every brief stands alone. Include the goal, scope, exact slice or race arm, how to verify, and what to report. Reports use `PASS`, `ISSUES`, or `BLOCKED` with evidence. A worker that can prove a defect reports `ISSUES` and lists every issue it can prove, not only the first.

If a worker drops out, proceed with N-1 and note it.

## Phase C: Aggregate

Read the terminal results. Drop a result that does not record the SHAs and method its brief names, and respawn that worker once. After a second miss, record a gap. A gap does not count as a pass. For coverage, every required slice needs a result. For a race, apply the selection rule declared up front. Use first pass, rank all, or best-of. Do not paste raw worker dumps.

## Phase D: Report

Return one consolidated in-chat report with a result table, issue one-liners, gaps or dropouts, and the race rule when used.
