# DSH Web GUI — design system ledger

Status: seeded from audit evidence 2026-08-25; dials pending user confirmation.

## Palette

Owned by `packages/client/ui-theme/src/styles/design-platform.css`. Static ramps per hue:
amber · blue · deepseek (brand anchor) · green · neutral · neutral-bluish · red — full
50–950 ramps with a parallel `body[data-ds-dark-theme]` mapping block. Feature packages
consume `--dsw-alias-*` semantic tokens only; literal colors in feature CSS are violations.
Spectrum variables are restated as `var(--dsw-*)` references in ui-primitives so the token
sheets stay the single color authority.

## Type

Figma weight 510 → rendered 500 (non-variable webfont snap note, design-platform.css:1).
Type scale and roles live in ui-theme sheets; mono reserved for code/terminal/diff.

## Spacing

Token-driven via theme aliases; component-local custom properties allowed for layout
contracts only (docs/web-styling.md).

## Radius / shadow / elevation

Declared in ui-theme global sheets (`base.css`, `gradient-shadow-text.css`,
`scrollbar.css`, `shiki.css`). Audit did not restate values here yet — TBD after
restyle work touches them.

## Motion

Tokens exist: `--ds-transition-duration-fast|default|slow` (0.1/0.2/0.3s) +
`--ds-ease-in-out` (ui-theme base.css). Reduced-motion honored in ~20 component sheets,
each hand-rolling its own media query — no central kill switch. Known violation:
`AppFrame.module.css:71` animates `left` (layout property).

## Icons

ONE family: hand-drawn inline SVG set in `ui-primitives/src/icons/index.tsx`
(~70 glyphs, `Icon<Name><Style><Size>` naming). Zero emoji in shipped source
(verified by sweep; hits were test fixtures only).

## Primitives inventory (ui-primitives)

Button · Input · Menu · Modal · Tooltip · HoverCard · Pill · DisclosureRow · StateDot ·
Toast · RiskConfirmation · ConnectionBanner · OnboardingSurface · TerminalBlock (ANSI) ·
DiffBlock · ReadBlock · SearchBlock · JsonTree · WebBlock · clipboard/copy-feedback hooks ·
anchored-position hooks · icons · brand marks.
Gaps noted for future needs: no Tabs primitive, no virtualized list, no Skeleton, no
EmptyState, no Select/Combobox (ModelSelect hand-builds its menu).

## Decisions log

See `decisions.jsonl`.
