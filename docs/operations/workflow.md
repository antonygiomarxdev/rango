# Work Management Workflow

This is the operational workflow for planning and execution in Rango.

## Single Source of Truth by Layer

- `ROADMAP.md`: strategy, phase goals, success criteria.
- GitHub Milestones: release targets (`v0.x.y`).
- GitHub Issues: executable work items.
- Pull Requests: implementation and validation evidence.

## Issue Lifecycle

1. Create issue from template.
2. Assign labels:
   - one `phase:*` label
   - one `type:*` label
   - one `priority:*` label
   - optional `area:*` labels
3. Attach milestone.
4. Move status through labels (`status:ready`, `status:in-progress`, `status:blocked`, `status:done`).
5. Merge PR with `Closes #<issue>`.

## Branching Policy

All work happens on a dedicated branch tied to an issue. No direct commits to `main`.

- Branch name: `issue/<N>-<short-slug>` (e.g. `issue/35-pyo3-binding`).
- Branch off `main`. Rebase rather than merge from `main` when refreshing.
- One issue per branch. If scope grows, split into a follow-up issue and branch.
- One PR per branch. PR title mirrors the conventional commit type and references the issue (`Closes #N`).
- Delete the branch after merge.
- Hotfixes follow the same flow against an `area:*` issue with `priority:p0`.

A branch without a tracking issue is rejected at review.

## Required Fields for New Work

Every issue must include:

- problem statement and scope
- acceptance criteria
- test plan
- out-of-scope section
- roadmap phase mapping

## PR Rules

- PR must reference at least one issue.
- PR description must include:
  - what changed
  - risks and rollback notes
  - tests executed
- No merge if CI is failing.

## Repository Protection Baseline

`main` must stay protected with:

- pull request required (no direct push)
- at least 1 approving review
- stale review dismissal on new commits
- resolved conversations required
- required CI checks (Format, Clippy, Tests, Security Audit, Deny, Docs)
- linear history required
- force-push disabled
- branch deletion disabled

CODEOWNERS is defined in `.github/CODEOWNERS` to formalize ownership on critical surfaces.

## Label Taxonomy

- Phase:
  - `phase:1-durable-substrate`
  - `phase:2-control-plane`
  - `phase:3-security-governance`
  - `phase:4-semantic-projections`
  - `phase:5-advanced-retrieval`
- Type:
  - `type:feature`
  - `type:bug`
  - `type:docs`
  - `type:adr`
  - `type:chore`
- Priority:
  - `priority:p0`
  - `priority:p1`
  - `priority:p2`
- Status:
  - `status:ready`
  - `status:in-progress`
  - `status:blocked`
  - `status:done`
- Area:
  - `area:core`
  - `area:storage`
  - `area:sync`
  - `area:server`
  - `area:sdk`
  - `area:cli`
  - `area:docs`
  - `area:bindings`
  - `area:bench`
  - `area:integrations`
  - `area:observability`

## Cadence

- Weekly: review milestone burn and blocked items.
- Per release: close/retarget open issues and update `ROADMAP.md` only if phase intent changed.

## Documentation Discipline

- Architectural changes require ADR updates in `docs/adr/`.
- Phase-level direction changes require `ROADMAP.md` update.
- Process changes require this file update.
