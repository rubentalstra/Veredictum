# Vendored openEHR spec docs: RM

- Source: https://github.com/openEHR/specifications-RM
- Ref: Release-1.1.0
- Commit: `355eb63e201c6b805c44c8809c70d63f79d54f92`
- License: CC-BY-SA 3.0 Unported — the upstream `LICENSE` is vendored verbatim alongside
  this file, from the same pinned commit. Root reference copies:
  `LICENSE-CC-BY-SA-3.0` / `LICENSE-APACHE-2.0`.
- Vendored by: `scripts/vendor/spec-docs.sh` (text formats only: adoc md txt csv json yaml yml robot xml opt g4)
- Plus the 34 UML class-diagram SVG(s) under `docs/UML/diagrams/` that
  the vendored chapters reference as `image::{uml_diagrams_uri}/<name>.svg`,
  taken from the same pinned commit. Referenced files only — the upstream
  diagram directory is not mirrored wholesale.
- Plus the 68 per-document figure(s) under
  `docs/<doc_name>/diagrams/` and `docs/<doc_name>/images/` that the vendored
  chapters reference as `image::{diagrams_uri}/<name>` /
  `image::{images_uri}/<name>`, taken byte-for-byte from the same pinned
  commit. Referenced files only — the upstream figure directories are not
  mirrored wholesale.
- Unreferenced figures, UML `.xmi`/`.mdzip`, XSDs and other binaries
  excluded — fetch from the repo at the pinned commit.

Do not hand-edit files under this directory; re-run the script instead.
