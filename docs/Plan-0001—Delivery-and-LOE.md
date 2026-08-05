# Plan 0001 — Revitalization Delivery Plan and Level of Effort

Companion to [Spec 0001 — Rust-Edu Website Revitalization](Spec-0001—Rust-Edu-Website-Revitalization.md) (approved
2026-08-04). The spec defines WHAT; the per-feature technical specs define HOW; this document defines the delivery
sequence, the pull-request series, and the level of effort (LOE).

**Wave 1 technical specs (generated, ready to implement):**

- [FR-0001 Design foundation and framework removal](FR-0001-design-foundation-and-framework-removal/spec.md) ·
  [plan](FR-0001-design-foundation-and-framework-removal/plan.md)
- [FR-0007 Deployment pipeline modernization and PR validation](FR-0007-deployment-pipeline-modernization-and-pr-validation/spec.md) ·
  [plan](FR-0007-deployment-pipeline-modernization-and-pr-validation/plan.md)

Wave 2 and Wave 3 technical specs are deliberately deferred until Wave 1 is implemented — specs are richer when the
codebase already shows the new patterns (per the spec-writer's same-wave rule).

## 1. Current-state assessment (why this work)

- Presentation is built on two vendored CSS-framework git submodules (`ext/bulma`, `ext/bulmaswatch`); a fresh clone
  does not build without an extra step, which is the top documented contributor failure. Upstream issue #1 has
  questioned the framework since 2022.
- Deployment relies on an unpinned third-party CI action; no Zola version is pinned anywhere.
- Latent defects: a search index is built but has no UI; an icon references a library that is never loaded; one
  template is dead code; framework markup leaks into content files; the navbar contains invalid nested-anchor markup.
- Community contributions exist but stall (upstream PRs #3 and #9 pending; issue #5 open for the resources catalog);
  issue #6 asks for a way to follow news.

## 2. Delivery strategy — the PR series

Each PR maps to one FR, has a single concern, and leaves the site fully buildable and deployable (NFR-0011). Target
branch: upstream `main` via fork PRs. Every PR description should carry before/after screenshots when visuals change.

| PR | FR      | Contents                                                                                 | Size   | Depends on | Upstream issues addressed |
| -- | ------- | ---------------------------------------------------------------------------------------- | ------ | ---------- | ------------------------- |
| 1  | FR-0001 | Remove submodules; design tokens; page shell; accessible nav; delete dead code            | Large (mostly deletions + new Sass) | —          | #1 (framework), setup friction |
| 2  | FR-0007 | Official Pages workflows; pinned Zola 0.22.1; PR build + link checks; operator runbook    | Small  | — (tolerates either order with PR 1) | reproducibility, supply chain |
| 3  | FR-0002 | Rebuild all page templates; content model contracts; strip framework markup from content; 404 | Medium | PR 1       | —                         |
| 4  | FR-0003 | Dark mode: pre-paint theme resolution, header toggle, persistence                         | Small  | PR 1       | —                         |
| 5  | FR-0004 | Atom feed for news + autodiscovery + subscribe link                                       | Tiny   | PR 3       | #6                        |
| 6  | FR-0005 | /search page consuming the build-time index (vanilla JS)                                  | Small  | PR 1, PR 3 | latent unused index       |
| 7  | FR-0006 | Workshop section revival: nav entry, event-list landing, future-event recipe              | Small  | PR 3       | —                         |
| 8  | FR-0008 | README rewrite, CONTRIBUTING with content recipes, PR template                            | Small  | PR 1, 2, 3 | contribution friction     |
| 9  | FR-0009 | Resource catalog: integrate upstream #3 + #9, curation pass                               | Tiny   | PR 3       | #3, #5 (enables), #9      |

Sequencing notes:

- PRs 1–2 are Wave 1 (foundational). The FR-0007 workflow keeps submodule checkout tolerant of either merge order, but
  the planned order is PR 1 → PR 2.
- PRs 3–4 are Wave 2; PR 4 only needs PR 1, so it can be prepared in parallel with PR 3.
- PRs 5–9 are Wave 3 and are mutually independent once PR 3 lands; they can be opened in any order or simultaneously.
- The Pages settings flip (deploy source: `gh-pages` branch → GitHub Actions) is an operator step by a repo admin,
  documented in the FR-0007 runbook; schedule it with PR 2's merge.

## 3. Milestones

- **M1 — Self-contained and reproducible** (after PRs 1–2): no submodules, no third-party actions, pinned toolchain,
  PR validation active. Contributor setup drops to 3 commands.
- **M2 — Rebuilt and themed** (after PRs 3–4): every page on the new hand-written design, light and dark, content
  decoupled from styling.
- **M3 — Feature-complete revitalization** (after PRs 5–9): feed, search, workshop section, contributor docs, and the
  expanded resources catalog live. Upstream issues #1 and #6 closable; #3 and #9 integrated; #5 answered with an
  ongoing contribution recipe.

## 4. Level of effort

Unit: **ideal engineering days** (focused days for one experienced contributor familiar with static sites; includes
implementation, self-review against the spec's acceptance checks, and PR preparation; excludes upstream review
latency).

| FR      | Scope                              | LOE (days) | Dominant cost                                            |
| ------- | ---------------------------------- | ---------- | -------------------------------------------------------- |
| FR-0001 | Foundation & framework removal     | 2.5 – 3.5  | Token system + accessible nav + contrast verification    |
| FR-0007 | CI/deploy modernization            | 1.0 – 1.5  | Workflow split, checksum-pinned toolchain, runbook       |
| FR-0002 | Template rebuild + content model   | 2.5 – 3.5  | 7+ templates, prose/code typography, content cleanup     |
| FR-0003 | Dark mode                          | 1.0 – 1.5  | Pre-paint resolution + AA contrast QA in both themes     |
| FR-0004 | Atom feed                          | 0.5        | Config + validation                                      |
| FR-0005 | Search page                        | 1.5 – 2.0  | Result UX, a11y live region, failure states              |
| FR-0006 | Workshop revival                   | 0.5 – 1.0  | Landing restructure + event recipe                       |
| FR-0008 | Contributor docs                   | 1.0 – 1.5  | Recipes with build-verified examples                     |
| FR-0009 | Resources expansion                | 0.5        | Two entries + curation pass                              |
| **Total** |                                  | **11 – 15.5** |                                                       |

Calendar projection: at ~50% allocation, Wave 1 lands in week 1–2, Wave 2 in week 3–4, Wave 3 in week 5–6 — **roughly
4–6 calendar weeks end to end**, dominated by upstream review turnaround rather than build time.

Estimation assumptions:

- One primary contributor; parallel contributors compress Wave 3 substantially (its five PRs are independent).
- Upstream maintainers respond within the calendar window; the fork can deploy its own Pages preview to de-risk visual
  review.
- No scope additions mid-wave; new ideas enter as post-M3 follow-ups.

## 5. Risks and mitigations

- **Visual-refresh subjectivity** — a redesign invites opinion churn. Mitigation: PR 1/PR 3 carry before/after
  screenshots and a fork-hosted live preview; the refresh keeps the existing brand palette (spec AC-0001.04) to bound
  the discussion.
- **Upstream review latency** — 9 PRs need maintainer attention. Mitigation: PRs are small and independent; the CFP
  momentum (upstream PR #13, merged here) shows active maintainers; M1 PRs deliver immediate maintainer value
  (reproducible CI) to build trust early.
- **Pages settings flip requires admin** — the workflow file cannot change the Pages source. Mitigation: FR-0007's
  8-step operator runbook; the old pipeline keeps working until the flip is verified.
- **Either-order merges in Wave 1** — FR-0007's workflow tolerates the submodules until FR-0001 lands (documented
  transition assumption D-09 in its spec).
- **Content PR attribution** — integrating upstream #3/#9 must credit the original authors (note in PR 9's
  description, e.g. `Co-authored-by` or explicit credit).

## 6. Operating model after M3

- Adding a news post, resource, or workshop event = one file, one PR, validated automatically (CONTRIBUTING recipes).
- Changing navigation/footer = one edit in `config.toml`.
- Site look = tokens in one Sass partial; no framework updates ever.
- Toolchain = git + pinned Zola; version bumps are a one-line change in `.zola-version` (plus checksum).

## 7. Next actions

1. Implement Wave 1 from the generated specs (`eng-implement-feature` skill: FR-0001, then FR-0007), opening PRs 1–2.
2. Coordinate the Pages-source flip with an upstream admin when PR 2 merges.
3. Generate Wave 2 technical specs (FR-0002, FR-0003) once Wave 1 is merged, then implement PRs 3–4.
4. Generate Wave 3 technical specs (FR-0004/0005/0006/0008/0009) after Wave 2, then implement PRs 5–9.
5. After M3: close upstream #1/#6, comment on #3/#9 with the integrated entries, and post the contribution recipes on
   issue #5.
