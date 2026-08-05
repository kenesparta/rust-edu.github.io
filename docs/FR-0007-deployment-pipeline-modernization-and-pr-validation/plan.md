# Implementation plan: Deployment pipeline modernization and PR validation

**Prerequisites:**

- Zola 0.22.1 available locally (the pin target and the parity reference for local-vs-CI verification; NFR-0006).
- git, plus a GitHub account with push access to the working repository; **repository admin access is required for
  Phase 4** (Pages settings, branch protection, branch deletion are settings operations, not file changes).
- Implementation-time lookups (values this plan cannot fix in advance): the current exact released version tag of
  each GitHub-maintained action to pin, and the SHA-256 checksum of the official Zola 0.22.1 Linux x86-64 release
  asset (computed once from the official download and committed — see spec D-01/D-02).
- No environment variables and no `config.toml` changes: the existing `[link_checker]` settings are consumed as-is
  (spec D-14).
- **FRs this depends on:** None (PRD Section 8, Part 1 — FR-0007 has `Dependencies: None`; it is a Wave-1
  foundational). Sibling relationship: FR-0001 (the other Wave-1 foundational, spec already generated) removes the
  CSS submodules and deliberately leaves the CI workflow to this FR; the delivery plan's working assumption is that
  FR-0001 merges first, but this plan's checkout tolerance makes either merge order safe (spec D-09), so neither FR
  blocks the other.
- Reference: `docs/FR-0007-deployment-pipeline-modernization-and-pr-validation/spec.md` for all technical details
  (workflow structure, pin policy, blocking/warn mapping, concurrency, error-code behavior, operator runbook).

### Phase 1: Toolchain source of truth and PR validation workflow

**1. Pinned-version source of truth** — Create the root version file holding the exact generator version and its
companion checksum file for the CI download, establishing the single source that both workflows and later the FR-0008
documentation reference. See spec Sections 4.2 and 4.4.

**2. PR validation workflow shell** — Create the CI workflow with its pull-request and manual-dispatch triggers,
read-only least-privilege permissions, per-PR concurrency that supersedes obsolete runs, and the pinned runner and
timeout envelope. See spec Section 4.1 and decisions D-04, D-06, D-07, D-08, D-11.

**3. Pinned generator installation step** — Add the installation step that reads the version file, downloads the
official release binary, verifies it against the pinned checksum before use, and asserts the installed version — the
same step shape the deployment workflow will reuse. See spec decision D-02.

**4. Blocking validation step** — Add the full site build including drafts as the blocking step, so build errors and
broken internal links fail the check with the offending file named in the log. See spec decision D-05 and the Error
handling table.

**5. Warn-only external link step** — Add the external link check as a non-blocking step that surfaces unreachable
links as warning annotations on the pull request and can never fail the check, completing the blocking/warn-only
split. See spec decision D-05.

### Phase 2: Deployment workflow

**6. Deploy workflow shell** — Create the deployment workflow with its push-to-main and manual-dispatch triggers,
the official Pages permission set, and the newest-wins concurrency policy that never cancels an in-flight
deployment. See spec Section 4.1 and decisions D-06, D-07, D-08.

**7. Build-and-package job** — Add the build job: transition-tolerant checkout, the shared pinned-generator
installation, the production build without drafts, Pages configuration, and upload of the built site as the
deployment artifact. See spec Section 4.1 and decisions D-09, D-12.

**8. Deploy job** — Add the deployment job gated on the build job, bound to the Pages environment so every
deployment is recorded in the repository's environment history, publishing the artifact through the official
deployment action. See spec Section 4.1.

### Phase 3: Legacy pipeline removal

**9. Legacy workflow deletion** — Delete the old workflow file, removing the unpinned third-party deploy action and
the publishing-branch model from the repository in the same change. See spec Sections 2 and 4.1.

**10. Badge correction** — Repoint the README build-status badge from the deleted workflow to the deployment
workflow so the badge stays truthful; every other documentation change remains with FR-0008. See spec Section 4.3.

### Phase 4: Pages migration and decommissioning (operator steps — repository settings, not files)

**11. Pages source switch and domain verification** — After the FR-0007 change is merged, switch the repository's
Pages source from the publishing branch to GitHub Actions, verify the custom domain and enforced HTTPS survived the
switch, and confirm the first Actions-based deployment is live — using a manual dispatch if the merge-time run failed
during the transition window. See spec Section 4.5, steps 1–5.

**12. Merge blocking enforcement** — Add the CI validation job as a required status check on the main branch so a
red check actually blocks merging, completing the PR-validation contract. See spec Section 4.5, step 6.

**13. Legacy decommissioning and transition cleanup** — Once Actions-based deployments are verified, delete the
retired publishing branch; and once FR-0001 is also on main, remove the submodule-tolerance flag from both checkout
steps to end the transition window. See spec Section 4.5, steps 7–8, and decision D-09.
