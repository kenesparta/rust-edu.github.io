# Implementation plan: Design foundation and framework removal

**Prerequisites:**

- Zola 0.22.1 installed (the only build tool — matches the locally verified binary and NFR-0006; no Node/npm, no
  additional package manager).
- git (submodule removal is a repository-level operation).
- No environment variables and no configuration changes required; `config.toml` is consumed as-is.
- **FRs this depends on:** None (PRD Section 8, Part 1 — FR-0001 has `Dependencies: None`). Note: FR-0007 (deployment
  pipeline) is the sibling Wave-1 foundational; it touches disjoint files (`.github/workflows/`), and this plan
  deliberately does not modify the CI workflow, so the two can land in either order without conflicts (the PRD's safe
  default is sequential).
- Reference: `docs/FR-0001-design-foundation-and-framework-removal/spec.md` for all technical details (tokens, shell
  structure, naming conventions, transition strategy).

### Phase 1: Framework removal and self-contained styles

**1. Submodule removal** — Remove both CSS framework submodules (`ext/bulma`, `ext/bulmaswatch`), their gitlink
entries, and the `.gitmodules` file so a fresh clone contains no external checkout step. See spec Section 4.3.

**2. Design token partial** — Create the single token definition point with the brand constants, the semantic color
set in its light and dark variants (dark emitted inert per the spec), the spacing and typography scales, and the
miscellaneous tokens. See spec Sections 4.2 and 4.5.

**3. Base element styles** — Create the element-default partial: minimal reset, body typography, headings, lists,
code, tables, images, link treatment, focus visibility, and skip-link reveal behavior, all consuming tokens only. See
spec Section 4.2.

**4. Stylesheet entry point** — Rewrite the stylesheet entry so it imports only the new project partials and compiles
through Zola's built-in pipeline to the unchanged public stylesheet URL, restoring a green build with no submodules.
See spec Sections 4.2 and the Transition strategy.

### Phase 2: Page shell

**5. Shell markup rebuild** — Rebuild the base template with the accessibility skeleton (skip link first, landmark
regions) and the semantic shell classes, preserving the existing template block names so all child templates keep
rendering, and removing the invalid nested-anchor markup. See spec Section 4.1.

**6. Config-driven navigation rendering** — Render the header navigation and both footer link groups from the
existing site-configuration entries in their configured order, opening external URLs in a new tab and dropping the
vestigial base-URL substitution. See spec Sections 4.1 and 4.4.

**7. Footer rebuild** — Rebuild the footer on the charcoal brand treatment with its two headed link groups, the CC0
license notice, and the source-repository link. See spec Section 4.1.

**8. Shell layout styles** — Create the layout partial styling the header, the centered readable content column, and
the footer with tokens and the documented class conventions. See spec Section 4.2.

### Phase 3: Responsive and accessible navigation

**9. Disclosure toggle markup** — Add the navigation toggle to the header as a real button with an accessible name
and expanded/collapsed semantics wired to the navigation list, with its icon drawn in CSS. See spec Section 4.1.

**10. Toggle behavior script** — Rewrite the site script to drive the disclosure state (activation toggles, Escape
closes and returns focus) and add the no-JS bootstrap so the menu only collapses when scripting is available. See
spec decision D-05.

**11. Responsive collapse styles** — Apply the single 768 px breakpoint: collapsed toggleable menu below it keyed to
the exposed state, full horizontal navigation with no toggle at and above it, verified against the 320–1920 px range.
See spec Sections 4.2 and 4.5.

### Phase 4: Dead code and documentation alignment

**12. Dead template removal** — Delete the unused home template and confirm nothing in templates, content, or
configuration references it. See spec Section 4.1.

**13. Icon markup removal** — Remove the never-loaded icon-library markup from the resource macro while keeping the
macro's rendered output (title link, optional author and description) intact. See spec Section 4.1.

**14. Quickstart correction** — Remove the submodule initialization step and its troubleshooting entry from the
README so the documented steps match the new clone-and-serve reality; the full documentation rewrite remains with
FR-0008. See spec Section 4.3.
