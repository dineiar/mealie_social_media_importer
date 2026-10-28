---
name: setup-pstack
description: Configure which models pstack uses per role and at what reasoning budget. Detects your available models and writes a rule file that Claude Code loads into every session, overriding the skill defaults. Use for /setup-pstack, "configure pstack models", "pstack budget", or changing pstack's model choices.
---

# Setup pstack

Write `~/.claude/rules/pstack-models.md`, a rule file that sets pstack's model per role. Claude Code loads every `.md` file in `~/.claude/rules/` into each new session, so the role lines are in context wherever pstack runs.

## Steps

### 1. Detect available models

The detected models are the `model` values the `Agent` tool schema lists (`opus`, `sonnet`, `haiku`, `fable`), each paired with an effort from its `effort` values (`low`, `medium`, `high`, `xhigh`, `max`). Write a real model as `<model> <effort>`, such as `opus xhigh`. The effort is the last token. Never write a model or effort the schema doesn't list. The aliases `inherit-parent` and `auto` are always valid even though they are not detected models.

### 2. Load current state

The default role-to-model mapping is the rule shape shown in step 5 below. If `~/.claude/rules/pstack-models.md` already exists, read it and treat its `# budget` line and its role values as the current choices. If it doesn't, but a Cursor rule from an earlier setup exists at `~/.cursor/rules/pstack-models.mdc`, read that instead and map its values: `claude-<family>-<version>-<effort>[-fast]` becomes `<family> <effort>`, and any non-Claude slug, such as `grok-4.7-xhigh-fast`, becomes `inherit-parent`. Otherwise start from the defaults. A line whose role is not in step 5, such as `how critics`, is from a retired role. Drop it.

### 3. Budget, map, and confirm

**(a) Ask for a budget.** Prefer `AskUserQuestion` over free text. In `-p` mode it is absent, so ask in plain text. Offer these four options with these exact labels, and name the current budget when the rule records one. With no rule, say that `large` matches the skill defaults.

- `unlimited — max reasoning`
- `large — xhigh reasoning`
- `medium — high reasoning`
- `small — medium reasoning`

**(b) Apply it.** Build the working table from the skill defaults, and on a re-run keep any role you changed by model, list, or alias (`inherit-parent`, `auto`). `unlimited`, `large`, `medium`, and `small` set the effort of every real model, panel entries included, to `max`, `xhigh`, `high`, or `medium`, on the ladder `max` > `xhigh` > `high` > `medium` > `low`. `inherit-parent` and `auto` do not change. So `unlimited` turns `opus xhigh` into `opus max`, `large` keeps the defaults, and `small` turns `opus xhigh` into `opus medium`.

**(c) Show the roles and confirm.** Show every role with its model. Also list each line step 2 dropped or mapped from a Cursor slug. Ask whether to accept as-is or change specific roles, offering the detected models plus `inherit-parent` and `auto` (both mean the role runs on the session model) as the options. Prefer `AskUserQuestion` over free text. It takes at most four options per question, so group roles that share a value and offer the likeliest models, leaving the rest to its free-text answer. For panel roles (arena runners, architect runners, interrogate reviewers) the value is a list, and one subagent runs per entry, alias entries included, so the list length sets the count. `arena cross-judge pool` is also a list, but Arena runs one judge. It tries only values on a model other than the session's, in list order, so order it by preference. An `inherit-parent` or `auto` value never judges. That is why the pool's default second entry is `sonnet xhigh` and not `inherit-parent`. `swarm workers` is the default model for every worker unless a race or comparison assigns another model per arm.

### 4. Validate

Every real model written must be a `model` value with an `effort` value from the `Agent` tool schema. `inherit-parent` and `auto` always pass. If a chosen value is not available, stop and ask again.

### 5. Write the rule

Write `~/.claude/rules/pstack-models.md` with no frontmatter. It holds the comment lines, a `# budget` line with the chosen label and its target effort, and one line per role, using the same labels poteto-mode uses. Overwrite the whole file so re-runs stay idempotent. Never put the role lines in `CLAUDE.md` or `AGENTS.md`. Shape:

```
# pstack model configuration. One line per role. Delete a line to fall back to the skill default.
# A value is `<model> <effort>`, such as `opus xhigh`, meaning Agent `model: "opus"` and `effort: "xhigh"`.
# `inherit-parent` or `auto` as a value: the role runs on the session model (omit Agent `model`). Alias entries in a panel list still count toward its fan-out.
# pstack skills live in `.claude/skills/<name>/SKILL.md`. The Skill tool refuses most of them, so to run "the **name** skill", Read that file and follow it.
# budget: large (xhigh)
feature, refactoring: inherit-parent
bug-fix: inherit-parent
perf-issue: inherit-parent
hillclimb: inherit-parent
judgment and prose: opus xhigh
hardest tasks: opus xhigh
how explorer: inherit-parent
how explainer: opus xhigh
why investigators: inherit-parent
why synthesizer: opus xhigh
reflect tooling: inherit-parent
reflect judgment, divergent, synthesizer: opus xhigh
arena runners: opus xhigh, inherit-parent
arena cross-judge pool: opus xhigh, sonnet xhigh
swarm workers: inherit-parent
architect runners: opus xhigh, inherit-parent
interrogate reviewers: opus xhigh, inherit-parent
```

If step 2 read a Cursor rule at `~/.cursor/rules/pstack-models.mdc`, tell the user it still exists and that Cursor still loads it.

### 6. Confirm

Tell the user the rule was written and that it applies to new sessions. Re-running this skill updates it.

### 7. Offer a verification skill (optional)

Check whether the project has a way to drive the real app for proof (a `verify-*` skill, or an existing harness). If not, offer once: "want a project-local verification skill, so agents can drive the app the way a user does and prove changes work? I can generate one with /create-verification-skill." On yes, `Read` `../create-verification-skill/SKILL.md` from this file and follow it. On no, move on without pushing.

### 8. Offer a formal verification skill (optional)

Check whether the project already has a formal verification skill (a project skill that runs a model checker, a proof, or another verifier against the code). If not, offer once: "want a formal verification skill, so agents can prove a key property of your code holds in every case in scope, not only the cases tests reach? I can generate one with /create-formal-verification-skill." On yes, `Read` `../create-formal-verification-skill/SKILL.md` from this file and follow it. On no, move on without pushing.
