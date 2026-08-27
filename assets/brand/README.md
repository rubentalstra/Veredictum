# Veredictum brand assets

The visual identity (owner decision 2026-08-26, direction A "Signal"): a **verdict seal** — the circular stamp an assayer strikes after testing, whose V doubles as the checkmark the instrument's verdicts stand for — wearing the openEHR community's palette (`tokens.css`: teal #258BB0 and signal orange #FF861C, read off openehr.org). The ring wording names openEHR descriptively (what is being tested); the mark is NOT an openEHR Foundation endorsement and never appears beside the Foundation's own logo without their blessing.

| File | Use |
|---|---|
| `veredictum-icon.svg` | Primary icon (the seal). Carries its own ground — works on light and dark. App icon, avatars, social. |
| `veredictum-icon-mono.svg` | Single-colour variant (`currentColor`) for stamps, badges, no-colour contexts. |
| `favicon.svg` | Favicon master: ground + V-check only (the knurled edge is illegible below 32 px). |
| `veredictum-seal.svg` | The FULL certification mark (keurmerk): ring wording + motto. Certificates, the website, anywhere it appears at 200 px or larger. |
| `veredictum-seal-card.svg` | The seal card: the certificate a verified deployment displays, and the 1280×640 social-preview master. |
| `tokens.css` | The palette as CSS custom properties — the single source for brand colours. |
| `design-directions.html` | The four-direction design study the choice was made from (owner decision 2026-08-26: direction A, "Signal"). Reference only. |

## The rasters (generated, never hand-edited)

`favicon-16.png`, `favicon-32.png`, `favicon-48.png`, `favicon.ico`,
`apple-touch-icon.png`, `icon-192.png` and `icon-512.png` are RENDERED from the
two SVG masters above by `scripts/render/brand-icons.sh` (#84). Re-run that
script after changing a master; never edit a raster by hand, and never add a
raster with no SVG source, because the mark then forks between the file people
edit and the file browsers fetch. The console's `app/veredictum-console/public/`
directory symlinks these rather than copying them, the same way `seal.svg` does.

Pending, tracked on FerroEHR#2789's identity checklist: the icon + wordmark
lockups (light/dark/auto), and the 1280×640 social-preview master with its PNG
render.
