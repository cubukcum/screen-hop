# screen-hop — UI Implementation Spec

> Distilled from the Claude Design handoff in docs/design/screen-hop/. Source of truth for the M5 Slint UI (decision D12). Auto-generated from the design-extraction pass; edit the handoff + re-extract if the design changes.

## Consolidated tokens (light / dark)

| Token | Light | Dark | Usage |
|---|---|---|---|
| `Primary Background` | #f3f3f3 | #1f1f1f | Main page/container bg; --bg variable |
| `Secondary Background` | #eaeaea | #272727 | Headers, secondary sections; --bg-2 |
| `Layer/Card Background` | #fff | #2b2b2b | Cards, panels, modals; --layer |
| `Primary Text` | #1a1a1a | #fff | Body text, headings; --text |
| `Secondary Text` | #5c5c5c | rgba(255,255,255,.73) | Descriptions, labels; --text-2 |
| `Tertiary Text` | #8a8a8a | rgba(255,255,255,.46) | Hints, disabled text; --text-3 |
| `Border/Stroke` | rgba(0,0,0,.06)–.1 | rgba(255,255,255,.08) | Dividers, subtle borders; --stroke |
| `Control Stroke` | rgba(0,0,0,.16)–.2 | rgba(255,255,255,.16) | Button/input borders; --ctl-stroke |
| `Subtle Background` | #fbfbfb | rgba(255,255,255,.05) | Secondary buttons, disabled states; --subtle |
| `Accent Green (Primary)` | #107c41 | #4abc7c | Buttons, active states; --accent |
| `Accent Text` | #0e6b36 | #6fd49a | Text on accent; --accent-text |
| `Accent Soft Background` | #e8f3ec | rgba(74,188,124,.16) | Highlight backgrounds; --accent-soft |
| `Success/Online` | #0e6b36 | #4abc7c | Checkmarks, confirmed; --ok-text |
| `Success Background` | #ecf6ef | rgba(74,188,124,.16) | Success container; --ok-bg |
| `Warning/Amber` | #b26a00 | #e7a93c | Alerts, needs attention; --warn-accent |
| `Warning Background` | #fff7ea | #312a18 | Warning container; --warn-bg |
| `Error/Red` | #c42b1c | #e04a3c | Errors, danger actions; --danger |
| `Error Background` | #fdf1f0 | #371d1b | Error container; --danger-bg |
| `Brand Green (Work PC)` | #117c44 | #2c9e54 | Peer identity, logo gradient |
| `Brand Blue (Gaming PC)` | #3a6fb0 | #5a93d6 | Peer identity, monitor styling |
| `Brand Teal (Home-lab)` | #0e8a8a | N/A | Peer identity, server icon |
| `Status Dot Online` | #1aa251 | #4abc7c | Indicator for online status |
| `--accent` | #107c41 | #4abc7c | Primary interactive color (buttons, highlights, checkmarks, progress fills) |
| `--accent-text` | #0e6b36 | #6fd49a | Text color on or near accent elements; labels in accent context |
| `--accent-soft` | #e8f3ec | rgba(74,188,124,0.16) | Soft background for accent-related callout boxes and guidance sections |
| `--accent-soft-stroke` | #bfe0cb | rgba(74,188,124,0.36) | Border color for soft accent backgrounds |
| `--bg` | #f3f3f3 | #1f1f1f | Primary window background |
| `--bg-2` | #eaeaea | #272727 | Secondary background (header bar) |
| `--layer` | #fff | #2b2b2b | Card/container background (inputs, list items) |
| `--layer-2` | #f6f6f6 | #323232 | Secondary layer background (footer, subtle sections) |
| `--ok-text` | #0e6b36 | #74d08f | Success/confirmed status text color |
| `--ok-bg` | #ecf6ef | #16301e | Success/confirmed status background |
| `--warn-text` | #8a5300 | #f2c879 | Warning/needs-attention status text |
| `--warn-accent` | #b26a00 | #e7a93c | Warning icon/accent color |
| `--warn-bg` | #fff7ea | #312a18 | Warning status background |
| `--warn-stroke` | #efd9ae | #544326 | Warning status border |
| `--text` | #1a1a1a | #ffffff | Primary text color |
| `--text-2` | #5c5c5c | rgba(255,255,255,0.73) | Secondary text (descriptions, subtitles) |
| `--text-3` | #8a8a8a | rgba(255,255,255,0.46) | Tertiary text (labels, hints, inactive states) |
| `--stroke` | rgba(0,0,0,0.06–0.1) | rgba(255,255,255,0.09) | Border color for containers, dividers |
| `--ctl-stroke` | rgba(0,0,0,0.16–0.2) | rgba(255,255,255,0.16) | Border color for interactive controls (inputs, buttons) |
| `--subtle` | #fbfbfb | rgba(255,255,255,0.05) | Subtle background for secondary buttons, inactive controls |
| `--subtle-press` | #e8e8e8 | rgba(255,255,255,0.07) | Pressed/hover state for subtle backgrounds (progress track background) |
| `Work (Primary)` | #117c44 | #4abc7c | PC owner color, accent bar, status indicators |
| `Gaming (Secondary)` | #3a6fb0 | #5a93d6 | PC owner color, monitor tiles, connection lines |
| `Home-lab (Tertiary)` | #0e8a8a | #0e8a8a | Server/alt PC color, accent elements |
| `Accent (Brand)` | #107c41 | #4abc7c | Primary action, spinners, active states, logo gradient |
| `Danger` | #c42b1c | #e04a3c | Error dialogs, destructive actions |
| `Warn` | #b26a00 | #e7a93c | Caution callouts, stranded states |
| `Text Primary` | #1a1a1a | #ffffff | Main headings, body copy |
| `Text Secondary` | #5c5c5c | rgba(255,255,255,.73) | Descriptions, meta info |
| `Text Tertiary` | #8a8a8a | rgba(255,255,255,.46) | Labels, hints, disabled text |
| `Bg Primary` | #f3f3f3 | #1f1f1f | Main container backgrounds |
| `Bg Secondary` | #eaeaea | #272727 | Header, section dividers |
| `Surface` | #ffffff | #2b2b2b | Cards, tiles, elevated surfaces |
| `Accent Green` | #107c41 | #4abc7c | Primary action buttons, active toggles, highlights, status indicators |
| `Accent Dark Green` | #0e6b36 | #1aa251 | Active nav items, badge text, success indicators |
| `Accent Soft (Light Wash)` | #e8f3ec | rgba(74,188,124,.16) | Active nav background, badge backgrounds |
| `Background Primary` | #f3f3f3 | #1f1f1f | Main panel background |
| `Background Secondary` | #eaeaea | #272727 | Header bars, sidebar background |
| `Layer (Card)` | #fff | #2b2b2b | Settings group containers, peer cards |
| `Stroke/Border` | rgba(0,0,0,.06) | rgba(255,255,255,.08) | Dividers, subtle borders |
| `Danger/Error` | #b22a1c | #e04a3c | Revoke buttons, error text |
| `Danger Stroke` | #f0cbc7 | N/A | Error button border |
| `Warning Accent` | N/A | #e7a93c | Warning icon colors |
| `PC/Work Icon` | #117c44 | #4abc7c | Work/Office preset color coding |
| `Gaming Icon` | #3a6fb0 | #5a93d6 | Gaming preset color coding |
| `Server/Home-lab Icon` | #0e8a8a | #0e8a8a | Server/Home-lab preset color coding |
| `--danger` | #c42b1c | #e04a3c | Error/destructive red |
| `--danger-text` | #b22a1c | #ff99a0 | Danger text color |
| `--danger-bg` | #fdf1f0 | #371d1b | Error/alert background |
| `--danger-stroke` | #f0cbc7 | #5a2e2a | Error border |

---

## DESIGN SYSTEM & FOUNDATIONS

# screen-hop Design System — Complete Specification

## CSS Variable Tokens

### Core Color Palette (Light Theme — Defaults)

| Token | Light Value | Dark Value | Usage |
|-------|-------------|-----------|-------|
| `--bg` | `#f3f3f3` | `#1f1f1f` | Page/container background |
| `--bg-2` | `#eaeaea` | `#272727` | Secondary background (headers, dividers) |
| `--layer` | `#fff` | `#2b2b2b` | Card/panel background |
| `--layer-2` | `#f6f6f6` | N/A | Tertiary panel (footer background) |
| `--text` | `#1a1a1a` | `#fff` | Primary text color |
| `--text-2` | `#5c5c5c` | `rgba(255,255,255,.73)` | Secondary text (descriptions) |
| `--text-3` | `#8a8a8a` | `rgba(255,255,255,.46)` | Tertiary text (hints, labels) |
| `--stroke` | `rgba(0,0,0,.06)` to `rgba(0,0,0,.1)` | `rgba(255,255,255,.08)` | Borders, dividers |
| `--ctl-stroke` | `rgba(0,0,0,.16)` to `rgba(0,0,0,.2)` | `rgba(255,255,255,.16)` | Control element borders (inputs, buttons) |
| `--subtle` | `#fbfbfb` | `rgba(255,255,255,.05)` | Subtle backgrounds (disabled, secondary buttons) |
| `--subtle-press` | `#e8e8e8` | N/A | Progress bar track |
| `--accent` | `#107c41` | `#4abc7c` | Primary action (success green) |
| `--accent-press` | `#0c5e32` | N/A | Pressed state |
| `--accent-text` | `#0e6b36` | `#6fd49a` | Text on accent backgrounds |
| `--accent-soft` | `#e8f3ec` | `rgba(74,188,124,.16)` | Accent highlight/info background |
| `--accent-soft-stroke` | `#bfe0cb` | `rgba(74,188,124,.36)` | Accent soft borders |

### Status Colors

| Token | Light | Dark | Meaning |
|-------|-------|------|---------|
| `--ok-text` / `--ok-bg` | `#0e6b36` / `#ecf6ef` | `#4abc7c` / `rgba(74,188,124,.16)` | Success, confirmed, online |
| `--warn-text` / `--warn-accent` / `--warn-bg` / `--warn-stroke` | `#8a5300` / `#b26a00` / `#fff7ea` / `#efd9ae` | `#f2c879` / `#e7a93c` / `#312a18` / `#544326` | Warning, needs attention, amber |
| `--danger` / `--danger-text` / `--danger-bg` | `#c42b1c` / `#b22a1c` / `#fdf1f0` | `#e04a3c` / `#ff99a0` / `#371d1b` | Error, alert, red |

### Accent Color Variants (Brand)

- **Work (Green)**: `#117c44` (core), used in logo gradient `linear-gradient(160deg, #117c44, #0c5e32)`
- **Gaming (Blue)**: `#3a6fb0` (peer/monitor icon background)
- **Home-lab (Teal)**: `#0e8a8a` (peer/monitor icon background)
- **Trading (Active Green)**: `#117c44` variant with highlight

---

## Typography Scale

### Font Families

- **Display/UI**: `'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif` (headings, titles)
- **Body**: `'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif` (default)
- **Monospace**: `ui-monospace, 'Cascadia Mono', monospace` (codes, pairs, IPs)

### Size & Weight Scale

| Usage | Size | Weight | Letter-spacing |
|-------|------|--------|-----------------|
| Page title | 26px | 700 | -0.5px |
| Dialog/Card title | 17–18px | 600 | -0.2px to -0.3px |
| Section heading | 16px | 600 | normal |
| Subsection | 14px–14.5px | 600 | normal |
| Label/tag | 13px | 600 | normal |
| Body text | 12.5px | 400 | normal |
| Secondary body | 12px | 400 | normal |
| Helper text / hint | 11.5px–12px | 400 | normal |
| Small label / badge | 10.5px–11px | 700 | 0.4px–0.6px |
| Tiny UI label | 10px | 700 | 0.5px |
| Monospace (code) | 13px–21px | 600 | 1px (for pair codes) |

---

## Spacing Scale

All spacing derived from 1px baseline, using multiples:

| Value | Usage |
|-------|-------|
| 2px | Minimal gaps (icon/text) |
| 4px | Dense spacing |
| 6px–7px | Tight grouping |
| 8px–9px | Component padding (inside buttons, small containers) |
| 10px–12px | Standard padding, medium gaps |
| 13px–15px | Card/panel padding |
| 16px–18px | Content sections, dialog padding |
| 20px–22px | Large sections |

---

## Border Radius Scale

| Value | Usage |
|-------|-------|
| 2px | Alert badge |
| 3px | Monospace display (menu breadcrumbs) |
| 5px | Monospace breadcrumb items |
| 6px–7px | Buttons, inputs, small containers |
| 8px–9px | Medium cards, preset items |
| 10px–11px | Large cards, dialogs |
| 50% | Circular badges, status dots, toggle knobs |
| 7px–9px | Icon backgrounds (rounded square) |

---

## Shadow Scale

| Elevation | Value | Usage |
|-----------|-------|-------|
| Minimal | `0 1px 3px rgba(0,0,0,.08)` | Node cards in desk map |
| Subtle | `0 2px 6px rgba(0,0,0,.18)` | Logo badge |
| Card | `0 8px 30px rgba(0,0,0,.13)` | Dialog boxes, standard panels |
| Modal (light) | `0 16px 44px rgba(0,0,0,.17)` | Critical state dialogs |
| Modal (dark) | `0 16px 44px rgba(0,0,0,.55)` | Dark theme modals |
| Dark flyout | `0 14px 40px rgba(0,0,0,.5)` | Dark tray popover |

---

## Icon Set (SVG Symbols)

All icons use `viewBox="0 0 24 24"`, `stroke-width` varies, `stroke="currentColor"`:

| ID | Depicts | Stroke-width |
|----|---------|--------------|
| `i-monitor` | Desktop monitor | 1.8 |
| `i-pc` | PC tower | 1.8 |
| `i-server` | Server rack | 1.8 |
| `i-search` | Magnifying glass | 1.8 |
| `i-gear` | Settings cog | 1.7 |
| `i-chev-r` | Chevron right | 1.8 |
| `i-chev-d` | Chevron down | 1.8 |
| `i-check` | Checkmark ✓ | 2.1 |
| `i-close` | X / close | 1.8 |
| `i-plus` | Plus | 1.8 |
| `i-alert` | Triangle warning | 1.8 |
| `i-info` | Info circle | 1.8 |
| `i-refresh` | Circular arrow / reload | 1.8 |
| `i-power` | Power button | 1.8 |
| `i-keyboard` | Keyboard | 1.6 |
| `i-identify` | Concentric circles | 1.7 |
| `i-preset` | Bookmark | 1.7 |
| `i-more` | Ellipsis (3 dots) | solid fill |
| `i-arrow-r` | Arrow right | 1.8 |
| `i-tiles` | Grid (4 squares) | 1.7 |
| `i-net` | Network / WiFi | 1.6 |
| `i-sun` | Sun | 1.7 |
| `i-moon` | Crescent moon | 1.7 |
| `i-logo` | screen-hop custom mark | 1.9 |
| `i-copy` | Copy/duplicate | 1.7 |
| `i-link` | Chain link | 1.7 |
| `i-qr` | QR code | 1.6 |
| `i-lock` | Padlock | 1.7 |
| `i-bolt` | Lightning bolt | 1.7 |

---

## Animation & Keyframes

| Name | Duration | Effect |
|------|----------|--------|
| `sh-spin` | 1s linear infinite | Full 360° rotation (loading spinners) |
| `sh-flash` | 1.1s ease-in-out infinite | Opacity pulse: 25% → 100% → 25% (flashing monitors) |
| `sh-dash` | N/A | Stroke dash animation (not used in static preview) |
| `sh-pulse` | 1.6s ease-out infinite | Box-shadow expansion pulse: `0 0 0 0 rgba(17,124,68,.4)` → `0 0 0 12px rgba(17,124,68,0)` (active label glow) |

---

## Component Styles

### Buttons

#### Primary (Accent)
- Height: 32–34px
- Padding: 0 16–18px
- Border-radius: 6px
- Background: `var(--accent, #107c41)`
- Text: `#fff` 13px weight 600
- Border: 1px solid transparent
- Cursor: pointer

#### Secondary (Subtle)
- Height: 30–32px
- Padding: 0 13–16px
- Border-radius: 6px
- Background: `var(--subtle, #fbfbfb)`
- Text: `var(--text, #1a1a1a)` 13px weight 600
- Border: 1px solid `var(--ctl-stroke, rgba(0,0,0,.16))`
- Cursor: pointer

#### Danger
- Background: `var(--danger, #c42b1c)`
- Text: `#fff`

### Input Fields

- Height: 30–34px
- Border-radius: 6px
- Border: 1px solid `var(--ctl-stroke, rgba(0,0,0,.16))`
- Background: `var(--layer, #fff)`
- Padding: 0 11px
- Font-size: 12.5–13px
- Placeholder color: `var(--text-3, #8a8a8a)`
- Monospace for codes: `font-family: ui-monospace`

### Cards/Panels

- Border-radius: 9–10px
- Background: `var(--layer, #fff)`
- Border: 1px solid `var(--stroke, rgba(0,0,0,.08))`
- Padding: 13–16px
- Box-shadow: varies (see Shadow Scale)

### Dialog Windows

- Border-radius: 9px
- Background: `var(--layer, #fff)` (light) / `var(--layer, #2b2b2b)` (dark)
- Border: 1px solid `var(--stroke, rgba(0,0,0,.08))`
- Box-shadow: `0 16px 44px rgba(0,0,0,.17)` (light) or `0 16px 44px rgba(0,0,0,.55)` (dark)
- Header height: 36px
- Footer padding: 13–18px

### Badges/Tags

- Padding: 3–6px 8–11px
- Border-radius: 8–11px (pill: 13px)
- Font-size: 10.5–11.5px
- Font-weight: 600–700
- Colors: status-specific (green, amber, etc.)

### Status Indicators

- Dot size: 5–7px diameter, `border-radius: 50%`
- Background colors: green `#1aa251` (online), amber `#e7a93c` (warning), gray `#8a8a8a` (offline)
- Glow ring (optional): `box-shadow: 0 0 0 3px` with soft color

---

## Layout Patterns

### Dialog (Onboarding, Critical States)
- Max-width: 430–660px (varies by content)
- Title area: 36px header with logo icon + title + close button (x)
- Content area: 16–18px padding
- Footer: 13px top border, 13–18px padding, flex row with back/next or cancel/action buttons
- Stepper: 12px vertical padding, flex row with numbered badges, flex connectors

### Tray Flyout (Dark Theme)
- Width: 360–392px
- Header: logo (27px), title ("screen-hop"), status badge, theme toggle
- Divider: 1px `rgba(255,255,255,.08)`
- Section: 9px vertical padding, 6px horizontal padding for label, 8–9px item padding
- Presets: flex wrap, 6px gap, 12.5px font

### Settings Panel
- Header: 36px dark background
- Sections: 16px padding with 18px margins between
- List items: 11–13px padding, 1px dividers

### Desk Map (Patchbay)
- PC nodes: 188px wide × 84px tall, rounded 10px, flex row
- Monitor cards: 230px wide × 84px tall
- SVG overlay: Bezier curves (opacity .15 background, .3 stroke for active)
- Circle nodes: 4.5px radius

---

## Copy & Literal Strings (Sample)

- **App name**: "screen-hop" (with hop in secondary color)
- **Dialog titles**: "Pair another PC", "Confirm your monitors", "Teach each PC its input on each monitor", etc.
- **Button text**: "Pair", "Next: monitors", "Next: calibrate", "Finish setup", "Apply", "Re-apply", "Switch and go blind", "Got it", etc.
- **Status labels**: "DDC/CI", "Paired", "online", "offline", "Switched", "Learning…", "reachable", "unreachable", etc.
- **Keyboard shortcuts**: Ctrl+Alt+1…4 (presets)

---

## Responsive Notes

- **Fluent Design**: Acrylic, rounded corners, clear layer separation
- **Windows 11 Theme**: Uses system font (Segoe UI Variable), follows Fluent design tokens
- **Dark Mode**: Full dark theme with adjusted opacity and color temperature (cooler shadows, brighter accents)
- **Peer Color Coding**: Each PC/monitor gets a unique accent (green, blue, teal) for visual identity in lists and desk map



**Open questions:**
- Are the exact font-weight values (600 vs 700) critical for Slint rendering, or should they be normalized to a smaller set (regular, medium, bold)?
- Should the monospace font fallback include platform-specific defaults (system-ui or OS-native mono fonts)?
- Are all animation keyframes (sh-spin, sh-pulse) needed for static UI renders, or only for interactive prototype?
- Should peer/monitor color palette (green #117c44, blue #3a6fb0, teal #0e8a8a) be extended to support arbitrary user-defined colors, or locked to these three?
- Dark theme opacity values (e.g., rgba(255,255,255,.73)) — should these be converted to hex equivalents for Slint compatibility?

---

## Onboarding Wizard - Complete Design Spec

# screen-hop Onboarding Wizard — Implementation Spec

## Overview
Windows 11 Fluent dialog-based wizard (4 steps, stepper navigation). All measurements in pixels unless noted. Light mode defaults shown; dark theme CSS variables provided separately.

---

## Container & Chrome

**Window Frame**
- Outer dimensions: 600px (Step 1) / 470px (Step 2) / 660px (Step 3) / 560px (Step 4)
- Height: ~580–650px (content-dependent)
- Border radius: 9px
- Box shadow: 0 8px 30px rgba(0,0,0,0.13)
- Border: 1px solid rgba(0,0,0,0.08)
- Background: var(--bg, #f3f3f3)
- Font family: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif
- Text color: var(--text, #1a1a1a)

**Header Bar**
- Height: 36px
- Background: var(--bg-2, #eaeaea)
- Border-bottom: 1px solid var(--stroke, rgba(0,0,0,0.06))
- Padding: 0 10px
- Display: flex, align-items: center

Header logo badge:
- Width/Height: 16px, border-radius: 4px
- Background: linear-gradient(160deg, #117c44, #0c5e32)
- Content: SVG icon i-logo, 11px × 11px, color: white

Header title text:
- Margin-left: 8px
- Font: 600 11.5px 'Segoe UI'
- Color: var(--text-2, #5c5c5c)
- Text: "Set up screen-hop"

Close button (top-right):
- Width/Height: 38px × 36px
- Flex-grow: right-aligned
- Background: transparent
- Border: none
- Color: var(--text-3, #8a8a8a)
- SVG: i-close, 13px × 13px

**Content Area**
- Padding: 14px 18px 16px

**Footer Bar**
- Display: flex, justify-content: space-between
- Padding: 13px 18px
- Border-top: 1px solid var(--stroke, rgba(0,0,0,0.07))
- Background: var(--layer-2, #f6f6f6)

---

## Stepper (All Steps Share Same Style)

**Container**
- Display: flex, align-items: center, gap: 6px
- Padding: 12px 18px 4px
- Font: 600 11.5px 'Segoe UI'

**Active Step (e.g., Step 1 in "Pair" screen)**
- Color: var(--accent-text, #0e6b36)
- Display: flex, align-items: center, gap: 6px

Active circle:
- Width/Height: 18px
- Border-radius: 50%
- Background: var(--accent, #107c41)
- Color: white
- Font: 600 10px
- Content: centered number (1, 2, 3, or 4)

**Inactive Step**
- Color: var(--text-3, #8a8a8a)
- Display: flex, align-items: center, gap: 6px

Inactive circle:
- Width/Height: 18px
- Border-radius: 50%
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.2))
- Font: 600 10px
- No fill

**Connector Lines (between steps)**
- Flex: 1
- Height: 1px
- Background: var(--stroke, rgba(0,0,0,0.1))

---

## Step 1: Pair PCs

### Content Title
- Font: 600 17px
- Letter-spacing: -0.2px
- Text: "Pair another PC"

Subtitle:
- Font: 500 12.5px
- Color: var(--text-2, #5c5c5c)
- Margin-top: 3px
- Text: "Every PC runs screen-hop as an equal peer — there is no server."

### Section: This PC's Code

**Container**
- Flex: 1
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))
- Border-radius: 9px
- Background: var(--layer, #fff)
- Padding: 13px 14px
- Display: flex, gap: 12px

**Label**
- Font: 700 10px, letter-spacing: 0.5px
- Color: var(--text-3, #8a8a8a)
- Text: "THIS PC'S CODE"

**Code Display**
- Font: ui-monospace, 'Cascadia Mono', monospace
- Font-size: 21px
- Font-weight: 600
- Letter-spacing: 1px
- Color: var(--text, #1a1a1a)
- Margin-top: 8px
- Display: flex, align-items: center, gap: 8px
- Text: "HOP-4F2A-9C7E"

**Copy Button**
- Width/Height: 28px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--subtle, #fbfbfb)
- Color: var(--text-2, #5c5c5c)
- Cursor: pointer
- Icon: i-copy, 14px × 14px

**Instructions Text**
- Font: 500 11.5px
- Color: var(--text-2, #5c5c5c)
- Margin-top: 9px
- Line-height: 1.45
- Text: "On the other PC: open screen-hop → **Add PC** → type this code."

### QR Code Section
- Width: 96px
- Height: auto (contains QR + label)
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))
- Border-radius: 9px
- Background: var(--layer, #fff)
- Display: flex, flex-direction: column, align-items: center, justify-content: center, gap: 6px
- Padding: inside flexbox
- Color: var(--text-2, #5c5c5c)

QR Icon:
- SVG: i-qr, 42px × 42px

QR Label:
- Font: 10px
- Text: "Scan"

### Divider ("or join from this PC")
- Display: flex, align-items: center, gap: 12px
- Margin: 15px 0

Divider line (flex: 1):
- Height: 1px
- Background: var(--stroke, rgba(0,0,0,0.08))

Center text:
- Font: 500 11px
- Color: var(--text-3, #8a8a8a)
- Text: "or join from this PC"

### Join Code Input Row
- Display: flex, gap: 8px

**Input Field**
- Flex: 1
- Height: 34px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--layer, #fff)
- Padding: 0 11px
- Font: 13px
- Font-family: ui-monospace, monospace
- Color: var(--text-3, #8a8a8a)
- Placeholder: "HOP-____-____"

**Pair Button**
- Height: 34px
- Padding: 0 16px
- Border-radius: 6px
- Border: 1px solid transparent
- Background: var(--accent, #107c41)
- Color: white
- Font: 600 13px
- Cursor: pointer
- Text: "Pair"

### Details/Collapsible: Add Host Manually

**Summary (trigger)**
- Font: 600 12px
- Color: var(--accent-text, #0e6b36)
- Cursor: pointer
- List-style: none
- Display: flex, align-items: center, gap: 6px
- Icon: i-plus, 13px × 13px
- Text: "Add a host manually (IP / hostname)"
- Margin-top: 12px

**Expanded Content**
- Display: flex, gap: 8px
- Margin-top: 9px

IP input:
- Flex: 1
- Height: 32px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--layer, #fff)
- Padding: 0 11px
- Font: 12.5px
- Color: var(--text-3, #8a8a8a)
- Placeholder: "192.168.1.50"

Port input:
- Width: 78px
- Height: 32px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--layer, #fff)
- Padding: 0 11px
- Font: 12.5px
- Color: var(--text-3, #8a8a8a)
- Placeholder: "7341"

Add button:
- Height: 32px
- Padding: 0 14px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--subtle, #fbfbfb)
- Color: var(--text, #1a1a1a)
- Font: 600 12.5px
- Cursor: pointer
- Text: "Add"

### Network Discovery Section

**Header**
- Font: 700 10px, letter-spacing: 0.5px
- Color: var(--text-3, #8a8a8a)
- Margin-bottom: 8px
- Text: "FOUND ON YOUR NETWORK"

**Discovery Item (unpaired)**
- Display: flex, align-items: center, gap: 10px
- Padding: 9px 11px
- Border-radius: 8px
- Background: var(--layer, #fff)
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))
- Margin-bottom: 7px

Icon badge:
- Width/Height: 30px
- Border-radius: 7px
- Background: rgba(58,111,176,0.13) [Gaming PC color]
- Color: #3a6fb0
- Icon: i-pc, 17px × 17px

Details (flex: 1, min-width: 0):
- Name: font 600 13px, "Gaming"
- IP: font 500 11px, font-family: ui-monospace, color: var(--text-3, #8a8a8a), "192.168.1.42"

Pair button:
- Height: 30px
- Padding: 0 14px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--subtle, #fbfbfb)
- Color: var(--text, #1a1a1a)
- Font: 600 12px
- Cursor: pointer
- Text: "Pair"

**Discovery Item (paired)**
- Same layout as above

Icon badge:
- Background: rgba(14,138,138,0.14) [Home-lab server color]
- Color: #0e8a8a
- Icon: i-server, 17px × 17px

Details:
- Name: "Home-lab"
- IP: "192.168.1.17"

Status chip:
- Display: flex, align-items: center, gap: 5px
- Font: 600 12px
- Color: var(--ok-text, #0e6b36)
- Icon: i-check, 14px × 14px
- Text: "Paired"

### Footer Buttons

**Back Button**
- Height: 32px
- Padding: 0 16px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--subtle, #fbfbfb)
- Color: var(--text, #1a1a1a)
- Font: 600 13px
- Cursor: pointer
- Text: "Back"

**Next Button**
- Height: 32px
- Padding: 0 18px
- Border-radius: 6px
- Border: 1px solid transparent
- Background: var(--accent, #107c41)
- Color: white
- Font: 600 13px
- Cursor: pointer
- Text: "Next: monitors"

---

## Step 2: Discover & Confirm Monitors

### Content Title
- Font: 700 11.5px
- Color: var(--text-3, #8a8a8a)
- Margin-bottom: 10px
- Text: "Step 2 of 4 · Monitors"

Main title:
- Font: 600 17px
- Letter-spacing: -0.2px
- Text: "Confirm your monitors"

Subtitle:
- Font: 500 12.5px
- Color: var(--text-2, #5c5c5c)
- Margin-top: 3px
- Text: "We probe each display over DDC/CI. Uncheck any you don't want screen-hop to manage."

### Monitor List

**Container**
- Margin-top: 14px
- Display: flex, flex-direction: column, gap: 8px

**Monitor Item (checked, DDC/CI ready)**
- Display: flex, align-items: center, gap: 11px
- Padding: 11px 12px
- Border-radius: 9px
- Background: var(--layer, #fff)
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))

Checkbox:
- Width/Height: 20px
- Border-radius: 5px
- Background: var(--accent, #107c41)
- Color: white
- Icon: i-check, 13px × 13px

Monitor icon:
- Width/Height: 32px
- Border-radius: 7px
- Background: var(--subtle, #fbfbfb)
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))
- Color: var(--text-2, #5c5c5c)
- Icon: i-monitor, 18px × 18px

Details (flex: 1, min-width: 0):
- Name: font 600 13px, "Left 27""
- Model+port: font 500 11px, color: var(--text-3, #8a8a8a), "Dell U2723QE · DP-1 · 2560×1440"

Status chip:
- Display: flex, align-items: center, gap: 5px
- Font: 600 11px
- Color: var(--ok-text, #0e6b36)
- Background: var(--ok-bg, #ecf6ef)
- Padding: 3px 8px
- Border-radius: 11px
- Icon: i-check, 12px × 12px
- Text: "DDC/CI"

**Monitor Item (checked, ready)**
- Same as above, example: "Center 32" / "LG 32UN880 · HDMI-2 · 3840×2160"

**Monitor Item (unchecked, not responding)**
- Display: flex, align-items: center, gap: 11px
- Padding: 11px 12px
- Border-radius: 9px
- Background: var(--warn-bg, #fff7ea)
- Border: 1px solid var(--warn-stroke, #efd9ae)

Checkbox (empty):
- Width/Height: 20px
- Border-radius: 5px
- Background: var(--subtle, #fbfbfb)
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.2))

Monitor icon:
- Width/Height: 32px
- Border-radius: 7px
- Background: var(--layer, #fff)
- Border: 1px solid var(--warn-stroke, #efd9ae)
- Color: var(--warn-accent, #b26a00)
- Icon: i-monitor, 18px × 18px

Details:
- Name: "Right 27""
- Error message: font 500 11px, color: var(--warn-text, #8a5300), "Dell U2723QE · DP-2 · not responding to DDC/CI"

Fix button:
- Font: 600 11.5px
- Color: var(--warn-text, #8a5300)
- Background: transparent
- Border: none
- Cursor: pointer
- Text decoration: underline
- Text: "Fix"

### Info Summary
- Display: flex, align-items: center, gap: 8px
- Margin-top: 13px
- Font: 500 11.5px
- Color: var(--text-3, #8a8a8a)
- Icon: i-info, 14px × 14px
- Text: "3 detected · 2 ready · 1 needs DDC/CI enabled in its menu."

### Footer
- Same structure as Step 1
- Next button text: "Next: calibrate"

---

## Step 3: Calibrate (Hero Step)

### Content Title
- Font: 700 11.5px
- Color: var(--text-3, #8a8a8a)
- Margin-bottom: 10px
- Text: "Step 3 of 4 · Calibrate"

Main title:
- Font: 600 17px
- Letter-spacing: -0.2px
- Text: "Teach each PC its input on each monitor"

Subtitle:
- Font: 500 12.5px
- Color: var(--text-2, #5c5c5c)
- Margin-top: 4px
- Line-height: 1.5
- Text: "screen-hop can only read a PC's input code for a monitor **while that PC is the one showing on it**. Make each PC active on each monitor once. You may need to press the monitor's physical Input button to bootstrap the first time."

### Calibration Matrix

**Container**
- Margin-top: 15px
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))
- Border-radius: 10px
- Overflow: hidden
- Background: var(--layer, #fff)

**Grid Layout**
- Grid: grid-template-columns: 130px 1fr 1fr 1fr

**Header Row**

First cell (empty label):
- Padding: 10px 12px
- Font: 700 10.5px, letter-spacing: 0.4px
- Color: var(--text-3, #8a8a8a)
- Background: var(--layer-2, #f6f6f6)
- Border-bottom: 1px solid var(--stroke, rgba(0,0,0,0.07))

PC Name Cells (Work / Gaming / Home-lab):
- Padding: 10px 8px
- Text-align: center
- Font: 600 12px
- Background: var(--layer-2, #f6f6f6)
- Border-bottom: 1px solid var(--stroke, rgba(0,0,0,0.07))
- Border-left: 1px solid var(--stroke, rgba(0,0,0,0.05))

PC Indicator (inside each):
- Display: inline-flex, align-items: center, gap: 5px
- Width: 8px circle dot (border-radius: 50%)
- Colors: #117c44 (Work), #3a6fb0 (Gaming), #0e8a8a (Home-lab)
- Text (e.g.): "Work"

**Data Rows**

Monitor Name Cell (Left 27" / Center 32" / Right 27"):
- Padding: 13px 12px
- Font: 600 12.5px
- Display: flex, align-items: center, gap: 7px
- Border-bottom: 1px solid var(--stroke, rgba(0,0,0,0.06))
- Icon: i-monitor, 15px × 15px, color: var(--text-3, #8a8a8a)

Status Cells (per PC/monitor intersection):
- Padding: 13px 8px
- Text-align: center
- Border-bottom: 1px solid var(--stroke, rgba(0,0,0,0.06))
- Border-left: 1px solid var(--stroke, rgba(0,0,0,0.05))

Status: "Confirmed"
- Display: inline-flex, align-items: center, gap: 5px
- Font: 600 11.5px
- Color: var(--ok-text, #0e6b36)
- Icon: i-check, 15px × 15px

Status: "— unknown"
- Font: 500 11px
- Color: var(--text-3, #8a8a8a)

Status: "Learning…" (animated)
- Display: inline-flex, align-items: center, gap: 6px
- Font: 700 11px
- Color: var(--accent-text, #0e6b36)
- Background: var(--accent-soft, #e8f3ec)
- Padding: 10px 8px
- Border-bottom: 1px solid var(--stroke, rgba(0,0,0,0.06))
- Border-left: 1px solid var(--stroke, rgba(0,0,0,0.05))
- Icon: i-refresh, 14px × 14px, animation: sh-spin 1s linear infinite

### Progress Bar

**Container**
- Display: flex, align-items: center, gap: 12px
- Margin-top: 13px

Progress track:
- Flex: 1
- Height: 7px
- Border-radius: 4px
- Background: var(--subtle-press, #e8e8e8)
- Overflow: hidden

Progress fill:
- Width: 56% (5 of 9 learned)
- Height: 100%
- Background: var(--accent, #107c41)

Progress text:
- Font: 600 12px
- Color: var(--text-2, #5c5c5c)
- White-space: nowrap
- Text: "5 of 9 learned"

### Guidance Box

**Container**
- Display: flex, gap: 11px, align-items: flex-start
- Margin-top: 13px
- Padding: 12px 14px
- Border-radius: 9px
- Background: var(--accent-soft, #e8f3ec)
- Border: 1px solid var(--accent-soft-stroke, #bfe0cb)

Arrow icon:
- SVG: i-arrow-r, 18px × 18px
- Flex: none
- Margin-top: 1px
- Color: var(--accent-text, #0e6b36)

Text section:
- Title: font 600 13px, color: var(--accent-text, #0e6b36), "Now: make Home-lab show on Center 32""
- Description: font 500 11.5px, color: var(--text-2, #5c5c5c), margin-top: 2px, line-height: 1.45
- Text: "Switch Home-lab onto it (or press the monitor's Input button). screen-hop reads the input the moment it appears."

### Footer

**Left text**
- Font: 500 11.5px
- Color: var(--text-3, #8a8a8a)
- Text: "You can finish later — unknown pairs just can't be auto-switched yet."

**Right button**
- Height: 32px
- Padding: 0 18px
- Border-radius: 6px
- Border: 1px solid transparent
- Background: var(--accent, #107c41)
- Color: white
- Font: 600 13px
- Cursor: pointer
- Text: "Next: names"

---

## Step 4: Name Look-Alikes

### Content Title
- Font: 700 11.5px
- Color: var(--text-3, #8a8a8a)
- Margin-bottom: 10px
- Text: "Step 4 of 4 · Names"

Main title:
- Font: 600 17px
- Letter-spacing: -0.2px
- Text: "Name your look-alikes"

Subtitle:
- Font: 500 12.5px
- Color: var(--text-2, #5c5c5c)
- Margin-top: 3px
- Text: "Two identical **Dell U2723QE** share the same name. Flash each one to see which is which, then label it."

### Monitor Cards

**Container**
- Display: flex, gap: 12px
- Margin-top: 15px

**Card 1: Currently Flashing**
- Flex: 1
- Border: 2px solid var(--accent, #107c41)
- Border-radius: 10px
- Background: var(--layer, #fff)
- Padding: 13px
- Animation: sh-pulse 1.6s ease-out infinite

Monitor display area:
- Height: 62px
- Border-radius: 7px
- Background: linear-gradient(135deg, var(--accent, #107c41), var(--accent-press, #0c5e32))
- Display: flex, align-items: center, justify-content: center
- Color: white
- Animation: sh-flash 1.1s ease-in-out infinite
- Icon: i-bolt, 26px × 26px

Flash button:
- Width: 100%
- Height: 30px
- Margin-top: 11px
- Border-radius: 6px
- Border: 1px solid var(--accent-soft-stroke, #bfe0cb)
- Background: var(--accent-soft, #e8f3ec)
- Color: var(--accent-text, #0e6b36)
- Font: 600 12px
- Cursor: pointer
- Display: flex, align-items: center, justify-content: center, gap: 6px
- Icon: i-identify, 14px × 14px
- Text: "Flashing…"

Name input:
- Width: 100%
- Height: 32px
- Margin-top: 9px
- Border-radius: 6px
- Border: 1px solid var(--accent, #107c41)
- Background: var(--layer, #fff)
- Padding: 0 10px
- Font: 600 13px
- Value: "Left 27""

**Card 2: Idle**
- Flex: 1
- Border: 1px solid var(--stroke, rgba(0,0,0,0.08))
- Border-radius: 10px
- Background: var(--layer, #fff)
- Padding: 13px

Monitor display area:
- Height: 62px
- Border-radius: 7px
- Background: var(--subtle-press, #e8e8e8)
- Display: flex, align-items: center, justify-content: center
- Color: var(--text-3, #8a8a8a)
- Icon: i-monitor, 24px × 24px

Flash button:
- Width: 100%
- Height: 30px
- Margin-top: 11px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--subtle, #fbfbfb)
- Color: var(--text, #1a1a1a)
- Font: 600 12px
- Cursor: pointer
- Display: flex, align-items: center, justify-content: center, gap: 6px
- Icon: i-identify, 14px × 14px
- Text: "Flash to identify"

Name input:
- Width: 100%
- Height: 32px
- Margin-top: 9px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--layer, #fff)
- Padding: 0 10px
- Font: 600 13px
- Value: "Right 27""

### Info Box
- Display: flex, align-items: center, gap: 8px
- Margin-top: 13px
- Font: 500 11.5px
- Color: var(--text-3, #8a8a8a)
- Icon: i-info, 14px × 14px
- Text: "Flashing inverts the monitor briefly so you can spot it on your desk."

### Footer

**Back Button**
- Height: 32px
- Padding: 0 16px
- Border-radius: 6px
- Border: 1px solid var(--ctl-stroke, rgba(0,0,0,0.16))
- Background: var(--subtle, #fbfbfb)
- Color: var(--text, #1a1a1a)
- Font: 600 13px
- Cursor: pointer
- Text: "Back"

**Finish Button**
- Height: 32px
- Padding: 0 18px
- Border-radius: 6px
- Border: 1px solid transparent
- Background: var(--accent, #107c41)
- Color: white
- Font: 600 13px
- Cursor: pointer
- Text: "Finish setup"

---

## CSS Variables & Theming

### Light Mode (defaults, inline)
```
--bg: #f3f3f3
--bg-2: #eaeaea
--text: #1a1a1a
--text-2: #5c5c5c
--text-3: #8a8a8a
--stroke: rgba(0,0,0,0.06) to rgba(0,0,0,0.1) [varies by context]
--layer: #fff
--layer-2: #f6f6f6
--accent: #107c41
--accent-text: #0e6b36
--ctl-stroke: rgba(0,0,0,0.16) to rgba(0,0,0,0.2)
--subtle: #fbfbfb
--subtle-press: #e8e8e8
--accent-soft: #e8f3ec
--accent-soft-stroke: #bfe0cb
--ok-text: #0e6b36
--ok-bg: #ecf6ef
--warn-text: #8a5300
--warn-accent: #b26a00
--warn-bg: #fff7ea
--warn-stroke: #efd9ae
```

### Dark Mode (override values from renderVals script)
```
--bg: #1f1f1f
--bg-2: #272727
--layer: #2b2b2b
--layer-2: #323232
--subtle: rgba(255,255,255,0.05)
--subtle-press: rgba(255,255,255,0.07)
--stroke: rgba(255,255,255,0.09)
--ctl-stroke: rgba(255,255,255,0.16)
--text: #ffffff
--text-2: rgba(255,255,255,0.73)
--text-3: rgba(255,255,255,0.46)
--accent: #4abc7c
--accent-text: #6fd49a
--accent-soft: rgba(74,188,124,0.16)
--accent-soft-stroke: rgba(74,188,124,0.36)
--ok-text: #74d08f
--ok-bg: #16301e
--warn-text: #f2c879
--warn-accent: #e7a93c
--warn-bg: #312a18
--warn-stroke: #544326
```

---

## Animations

| Name | Effect |
|------|--------|
| `sh-spin` | Full 360° rotation, 1s linear infinite |
| `sh-flash` | Opacity pulse 0.25→1→0.25, 1.1s ease-in-out infinite |
| `sh-pulse` | Box-shadow expand/contract, 1.6s ease-out infinite |

---

## Responsive Notes

- Fixed pixel layouts; all dimensions absolute.
- Dialogs are centered on screen (not specified in design).
- QR code placeholder: actual QR generation external.
- Monitor icons: use SVG symbol i-monitor (18px default).
- PC/Server icons: use i-pc or i-server per context.
- All text is left-aligned unless explicitly centered (e.g., stepper, matrix cells).
- Scrolling: assume step content fits without vertical scroll in typical 1080p+ window.

---

## Implementation Notes for Slint (Rust GUI)

1. **Use CSS-like theming:** Map var(--*) to named color constants for light/dark switching.
2. **Stepper:** Render as horizontal flex layout; use border-radius circles with conditional fill (active/inactive).
3. **Matrix table:** Grid layout with 4 columns; header sticky if content scrolls.
4. **Animations:**
   - Spinning refresh: `RotationAnimation` in Slint.
   - Pulsing border: use `box-shadow` equivalent or `Rectangle` outline animation.
   - Flashing: toggle opacity on a timer.
5. **Buttons:** All use `Button` control; apply accent background for primary, subtle for secondary.
6. **Input fields:** `TextInput` with styled borders and placeholder text.
7. **Details/Collapsible:** Slint's `StandardListView` with expand toggle or custom conditional rendering.
8. **QR code:** Placeholder SVG or integrate external QR library (e.g., `qr-code` crate + Canvas).
9. **Icons:** Load SVG symbols from embedded `<symbol>` definitions; render via `Image` or native SVG support.
10. **Fonts:** Fallback chain: 'Segoe UI Variable Text' (or 'Segoe UI Variable Display' for header) → 'Segoe UI' → system-ui → sans-serif. Monospace: ui-monospace or 'Cascadia Mono'.


**Open questions:**
- QR code: Should it be generated via qr-code crate, embedded as SVG, or placeholder? Current spec shows SVG symbol i-qr as visual stand-in.
- Network discovery polling: How often should 'FOUND ON YOUR NETWORK' list refresh during Pair step? Design shows static mock.
- Keyboard navigation: Should Tab/Shift+Tab move between fields and buttons in defined order? Not specified in design.
- Accessibility: Should status chips (Confirmed, Learning, etc.) announce state via screen reader? Matrix table may benefit from ARIA roles.
- Platform-specific window chrome: Are Fluent drag bar, minimize/maximize/close buttons, and shadow handled by Slint's window decorations or custom?
- Monitor input capture: How is the live OSD guidance (e.g., 'Now: make Home-lab show on Center 32"') populated from backend state?
- Error recovery on Monitors step: 'Fix' button behavior—does it retry DDC/CI or open a help dialog? Not detailed in design.

---

## DESK-MAP / ROUTING / TRAY-FLYOUT: Hero Surfaces Design Spec

# Screen-Hop Hero Surfaces — Implementation Spec

## 1. TRAY FLYOUT (Expandable List Variant — Selected)

### 1.1 Overall Container
- **Size**: 392px wide × variable height
- **Corner radius**: 9px
- **Light theme**: Background `#f8f8fa`, border 1px `rgba(0,0,0,.09)`
- **Dark theme**: Background `#2b2b2b`, border 1px `rgba(255,255,255,.08)`
- **Box shadow**: Light: `0 12px 36px rgba(0,0,0,.16)` | Dark: `0 14px 40px rgba(0,0,0,.5)`
- **Font family**: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif

### 1.2 Header Row
- **Height**: 38px
- **Padding**: 12px 13px (light) | 13px 12px 11px 14px (dark)
- **Alignment**: Flex, centered, gap 9px–10px
- **Background**: Light: `#f8f8fa` | Dark: `#2b2b2b`

#### Logo Icon
- **Size**: 24px × 24px (light) | 27px × 27px (dark)
- **Corner radius**: 6px (light) | 7px (dark)
- **Background**: `linear-gradient(160deg,#117c44,#0c5e32)` (both themes)
- **Icon color**: `#fff`
- **Icon size**: 15px (light) | 17px (dark)

#### Title Text
- **Text**: "screen-" + "hop" (where "hop" is colored #0e6b36 light, #6fd49a dark)
- **Font size**: 14px
- **Font weight**: 600
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Flex**: 1 (grows to fill)

#### Status Badge
- **Layout**: Flex, centered, gap 5px
- **Font size**: 11px (light) | 11.5px (dark)
- **Font weight**: 600
- **Padding**: 3px 8px
- **Corner radius**: 11px
- **Background**: Light: `#ecf6ef` | Dark: `rgba(74,188,124,.16)`
- **Text**: "3 online" (or actual count)
- **Color**: Light: `#0e6b36` | Dark: `#74d08f`
- **Dot**: 6px circle, background Light: `#1aa251` | Dark: `#4abc7c`

#### Theme Toggle Icon
- **Type**: Sun or Moon icon (16px)
- **Color**: Light: varies | Dark: `rgba(255,255,255,.6)`
- **Clickable button**: 30px × 30px, transparent, border-radius 6px

### 1.3 Monitors Section

#### Header Label
- **Text**: "MONITORS"
- **Font size**: 10.5px
- **Font weight**: 700
- **Letter spacing**: 0.6px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **Padding**: 9px 12px (light) | 9px 6px 4px (dark)
- **Top margin**: Light: 0 | Dark: 0

#### Monitor Row
- **Height**: 64px (expandable)
- **Padding**: 8px 9px (light) | 8px 9px (dark)
- **Margin bottom**: 0 (rows stack with 0 gap)
- **Background**: Light: `#fff` with 1px border `rgba(0,0,0,.08)` | Dark: transparent
- **Border radius**: 7px (light) | 0 (dark, flush in list)
- **Cursor**: pointer

##### Monitor Icon Container
- **Size**: 34px × 34px
- **Background**: Light: `rgba({pc-color},.13)` | Dark: `rgba(255,255,255,.05)`
- **Border**: Light: none | Dark: 1px `rgba(255,255,255,.09)`
- **Border radius**: 8px
- **Icon color**: Based on PC owner (Work: `#117c44`, Gaming: `#3a6fb0`, Home-lab: `#0e8a8a`)
- **Icon size**: 18px

##### Monitor Name & Spec
- **Layout**: Flex column, flex 1
- **Title**: Font size 13.5px, weight 600, color Light: `#1a1a1a` | Dark: `#fff`
- **Subtitle**: Font size 11px, color Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **White space**: `nowrap`, `overflow: hidden`, `text-overflow: ellipsis`
- **Margin top**: 0

##### Owner Badge (Pill)
- **Layout**: Flex, centered, gap 6px
- **Padding**: 3px 8px (light) | 3px 8px 3px 7px (dark)
- **Border radius**: 13px
- **Background**: Light: `#fbfbfb` with 1px border `rgba(0,0,0,.08)` | Dark: `rgba(255,255,255,.05)` with 1px border `rgba(255,255,255,.09)`
- **Font size**: 12px (light) | 12px (dark)
- **Font weight**: 600
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Dot**: 7px circle, color matches PC
- **Gap to dot**: 6px

##### Chevron Icon
- **Type**: Chevron down
- **Size**: 15px (light) | 15px (dark)
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **Display**: Right-aligned in row

### 1.4 Presets Section

#### Separator
- **Height**: 1px
- **Background**: Light: `rgba(0,0,0,.07)` | Dark: `rgba(255,255,255,.08)`
- **Margin**: 0 2px
- **Top margin**: 0 (direct under monitors)

#### Presets Header
- **Text**: "PRESETS"
- **Font size**: 10px (light) | 10.5px (dark)
- **Font weight**: 700
- **Letter spacing**: 0.5px–0.6px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **Padding**: 9px 12px

#### Preset Buttons Row
- **Display**: Flex, wrap
- **Gap**: 6px–7px
- **Padding**: 0 12px 9px (light) | 0 14px 12px (dark)

##### Preset Button (Inactive)
- **Font size**: 12.5px (light) | 12.5px (dark)
- **Font weight**: 600
- **Padding**: 6px 10px (light) | 6px 10px (dark)
- **Border radius**: 8px (light) | 8px (dark)
- **Background**: Light: `#fff` | Dark: `rgba(255,255,255,.05)`
- **Border**: Light: 1px `rgba(0,0,0,.16)` | Dark: 1px `rgba(255,255,255,.16)`
- **Text color**: Light: `#1a1a1a` | Dark: `#fff`

##### Preset Button (Active)
- **Background**: Light: `#e8f3ec` | Dark: `rgba(74,188,124,.16)`
- **Border**: Light: 1px `#bfe0cb` | Dark: 1px `rgba(74,188,124,.36)`
- **Text color**: Light: `#0e6b36` | Dark: `#6fd49a`

### 1.5 Bottom Action Row (Optional)
- **Display**: Flex, space-between
- **Padding**: 11px 14px
- **Border top**: 1px `rgba(0,0,0,.07)`
- **Background**: Light: `#fff` | Dark: transparent
- **Text**: "Manage" / "Open desk map"
- **Font size**: 12.5px
- **Font weight**: 600
- **Color**: Light: `#5c5c5c` | Dark: `#0e6b36`

---

## 2. DESK-MAP / ROUTING HERO (Patchbay Layout)

### 2.1 Container
- **Size**: 720px wide × 480px tall (approx)
- **Corner radius**: 9px
- **Background**: Light: `#f3f3f3` | Dark: `#1f1f1f`–`#272727` (gradient fallback: linear-gradient(180deg, #242424, #1c1c1c))
- **Border**: 1px, Light: `rgba(0,0,0,.08)` | Dark: `rgba(255,255,255,.08)`
- **Box shadow**: Light: `0 16px 48px rgba(0,0,0,.16)` | Dark: `0 14px 40px rgba(0,0,0,.5)`
- **Font family**: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif

### 2.2 Window Header
- **Height**: 36px
- **Padding**: 0 10px
- **Background**: Light: `#eaeaea` | Dark: `#272727`
- **Border bottom**: 1px, Light: `rgba(0,0,0,.06)` | Dark: `rgba(255,255,255,.06)`
- **Layout**: Flex, centered

#### Logo Icon
- **Size**: 16px × 16px
- **Corner radius**: 4px
- **Background**: `linear-gradient(160deg,#117c44,#0c5e32)`
- **Icon color**: `#fff`
- **Icon size**: 11px

#### Title
- **Text**: "screen-hop — Desk map"
- **Font size**: 11.5px
- **Font weight**: 600
- **Color**: Light: `#5c5c5c` | Dark: `rgba(255,255,255,.6)`
- **Margin left**: 8px

### 2.3 Main Content Padding
- **Padding**: 15px 20px 18px (light) | 13px 16px (dark variant)

### 2.4 Title Section
- **Layout**: Flex, justify-between, margin-bottom 6px

#### Left Title Group
- **Title text**: "Desk map"
- **Font size**: 18px
- **Font weight**: 600
- **Letter spacing**: -0.3px
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Subtitle**: "Lines show who drives what. Click a monitor to re-route it."
- **Subtitle font size**: 12px
- **Subtitle color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.6)`
- **Margin top**: 1px

#### Status Badge
- **Text**: "All online"
- **Layout**: Flex, centered, gap 6px
- **Font size**: 12px
- **Font weight**: 600
- **Padding**: 5px 11px
- **Border radius**: 14px
- **Background**: Light: `#ecf6ef` | Dark: `rgba(74,188,124,.16)`
- **Text color**: Light: `#0e6b36` | Dark: `#74d08f`
- **Border**: Light: 1px `#bfe0cb` | Dark: transparent
- **Dot**: 7px circle, background Light: `#1aa251` | Dark: `#4abc7c`

### 2.5 Column Headers
- **Row height**: 20px
- **Position**: Absolute, relative to content start

#### "PEERS" Label
- **Font size**: 10px
- **Font weight**: 700
- **Letter spacing**: 0.5px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.6)`
- **Left**: 0

#### "MONITORS" Label
- **Font size**: 10px
- **Font weight**: 700
- **Letter spacing**: 0.5px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.6)`
- **Left**: 430px

### 2.6 Patchbay Canvas
- **Size**: 660px wide × 312px tall (light) | variable (dark)
- **Position**: Relative

#### SVG Paths (Connection Lines)
- **Stroke width (background)**: 9px
- **Stroke width (active)**: 3.2px
- **Stroke linecap**: round
- **Path opacity (background)**: 0.15
- **Colors**:
  - Work → monitors: `#117c44`
  - Gaming → monitors: `#3a6fb0`
  - Home-lab → monitors: `#0e8a8a`
- **End-point circles**: 4.5px radius, color matches line

#### Path Examples (Bezier curves)
```
M188,48 C 296,48 322,48 430,48        (Work → Left 27")
M188,150 C 296,150 322,150 430,150    (Gaming → Center 32")
M188,48 C 300,48 318,252 430,252      (Work → Right 27" curved)
```

### 2.7 PC/Peer Nodes (Left Column)

#### Node Container
- **Width**: 188px
- **Height**: 84px
- **Position**: Absolute, left 0
- **Top offsets**: 6px, 114px, 222px
- **Border radius**: 10px
- **Background**: Light: `#fff` | Dark: `#2b2b2b`
- **Border**: 1px, Light: `rgba(0,0,0,.08)` | Dark: `rgba(255,255,255,.09)`
- **Box shadow**: Light: `0 1px 3px rgba(0,0,0,.08)` | Dark: none
- **Padding**: 0 12px 0 15px
- **Layout**: Flex, centered, gap 11px
- **Overflow**: hidden

#### Left Accent Bar
- **Position**: Absolute left 0, top 0, bottom 0
- **Width**: 4px
- **Background**: Color matches PC (Work: `#117c44`, Gaming: `#3a6fb0`, Home-lab: `#0e8a8a`)

#### PC Icon Container
- **Size**: 36px × 36px
- **Border radius**: 9px
- **Background**: `rgba({pc-color},.13)`
- **Color**: Matches PC color
- **Icon**: PC or server icon, 20px
- **Flex**: none

#### PC Info
- **Flex**: 1, min-width 0

##### PC Name
- **Font size**: 14px
- **Font weight**: 600
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Display**: Flex, gap 6px, align-items center

##### "THIS PC" Badge (Work only)
- **Text**: "THIS PC"
- **Font size**: 9px
- **Font weight**: 700
- **Letter spacing**: 0.3px
- **Padding**: 1px 5px
- **Border radius**: 8px
- **Background**: Light: `#e8f3ec` | Dark: `rgba(17,124,68,.12)`
- **Color**: Light: `#0e6b36` | Dark: `#4abc7c`
- **Border**: Light: 1px `#bfe0cb` | Dark: none

##### Monitor Count
- **Font size**: 11px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **Margin top**: 2px

#### Status Dot
- **Size**: 9px circle
- **Flex**: none
- **Background**: `#1aa251` (online) | `#8a8a8a` (offline, not shown in active state)
- **Box shadow**: Light: `0 0 0 3px #ecf6ef` | Dark: `0 0 0 3px rgba(74,188,124,.16)`

### 2.8 Monitor Tiles (Right Column)

#### Tile Container
- **Width**: 230px
- **Height**: 84px
- **Position**: Absolute, left 430px
- **Top offsets**: 6px, 114px, 222px
- **Border radius**: 10px
- **Background**: Light: `#fff` | Dark: `#2b2b2b`
- **Border**: 1px, Light: `rgba(0,0,0,.08)` | Dark: `rgba(255,255,255,.09)`
- **Box shadow**: Light: `0 1px 3px rgba(0,0,0,.08)` | Dark: none
- **Padding**: 0 14px
- **Cursor**: pointer
- **Layout**: Flex column, justify-center, gap 5px

#### Tile Content Row
- **Display**: Flex, gap 9px, align-items center

##### Monitor Icon
- **Size**: 30px × 30px
- **Border radius**: 7px
- **Background**: `rgba({pc-color},.13)`
- **Color**: Matches driving PC
- **Icon**: Monitor, 18px
- **Flex**: none

##### Monitor Name & Spec
- **Flex**: 1, min-width 0

###### Name
- **Font size**: 13.5px
- **Font weight**: 600
- **Color**: Light: `#1a1a1a` | Dark: `#fff`

###### Spec
- **Font size**: 10.5px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **White space**: nowrap, overflow hidden, text-overflow ellipsis
- **Example**: "Dell U2723QE · DP-1 · 1440p"

##### Owner Badge (Pill)
- **Display**: Flex, centered, gap 6px
- **Padding**: 3px 8px (light) | 3px 8px (dark)
- **Border radius**: 13px
- **Background**: Light: `#fbfbfb` | Dark: `rgba(255,255,255,.05)`
- **Border**: Light: 1px `rgba(0,0,0,.08)` | Dark: 1px `rgba(255,255,255,.09)`
- **Font size**: 11.5px
- **Font weight**: 600
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Dot**: 7px circle, matches PC color

### 2.9 Helper Text
- **Margin top**: 6px
- **Display**: Flex, gap 8px, align-items center
- **Font size**: 11px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **Icon**: Info, 13px
- **Text**: "Hover a PC to highlight its cables · drag a cable end to another monitor to re-route. Live & interactive in screen-hop.dc.html."

---

## 3. SWITCHING / IN-PROGRESS STATE DIALOG

### 3.1 Dialog Container
- **Width**: 480px
- **Corner radius**: 9px
- **Background**: Light: `#fff` | Dark: `#2b2b2b`
- **Border**: 1px, Light: `rgba(0,0,0,.08)` | Dark: `rgba(255,255,255,.08)`
- **Box shadow**: Light: `0 16px 44px rgba(0,0,0,.17)` | Dark: `0 16px 44px rgba(0,0,0,.55)`
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Font family**: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif

### 3.2 Header Section
- **Padding**: 20px 22px 6px
- **Display**: Flex, gap 14px

#### Spinner Icon
- **Size**: 38px × 38px
- **Border radius**: 50%
- **Background**: Light: `#e8f3ec` | Dark: `rgba(74,188,124,.16)`
- **Color**: Light: `#107c41` | Dark: `#4abc7c`
- **Icon**: Refresh, 21px
- **Animation**: `sh-spin 1s linear infinite` (rotate 360°)
- **Flex**: none

#### Title Section
- **Flex**: 1

##### Main Title
- **Text**: "Switching Center 32""
- **Font size**: 16px
- **Font weight**: 600
- **Color**: Light: `#1a1a1a` | Dark: `#fff`

##### Source → Target Transition
- **Display**: Flex, gap 7px, align-items center
- **Margin top**: 6px
- **Font size**: 12px

###### Source Badge (Inactive)
- **Layout**: Flex, gap 5px, align-items center
- **Padding**: 2px 8px
- **Border radius**: 11px
- **Background**: Light: `#fbfbfb` | Dark: `rgba(255,255,255,.05)`
- **Border**: 1px, Light: `rgba(0,0,0,.08)` | Dark: `rgba(255,255,255,.09)`
- **Color**: Source PC color (Gaming: `#3a6fb0`)
- **Text**: "Gaming"
- **Dot**: 6px circle, color matches

###### Arrow Icon
- **Type**: Arrow right
- **Size**: 14px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`

###### Target Badge (Active)
- **Layout**: Flex, gap 5px, align-items center
- **Padding**: 2px 8px
- **Border radius**: 11px
- **Background**: Light: `#e8f3ec` | Dark: `rgba(74,188,124,.16)`
- **Border**: 1px, Light: `#bfe0cb` | Dark: `rgba(74,188,124,.36)`
- **Color**: Light: `#0e6b36` | Dark: `#4abc7c`
- **Text**: "Work"
- **Dot**: 6px circle

### 3.3 Progress Steps
- **Padding**: 12px 22px 4px
- **Display**: Flex column, gap 11px

#### Step Row
- **Display**: Flex, gap 10px, align-items center

##### Step Indicator

###### Completed Step
- **Size**: 18px circle
- **Background**: Light: `#ecf6ef` | Dark: `rgba(74,188,124,.16)`
- **Color**: Light: `#0e6b36` | Dark: `#4abc7c`
- **Icon**: Check, 12px

###### In-Progress Step
- **Size**: 18px circle
- **Border**: 2px, color Light: `#107c41` | Dark: `#4abc7c`
- **Border-right-color**: transparent
- **Animation**: `sh-spin 0.8s linear infinite`

###### Pending Step
- **Size**: 18px circle
- **Border**: 2px, Light: `rgba(0,0,0,.18)` | Dark: `rgba(255,255,255,.2)`

##### Step Text
- **Font size**: 13px (completed) | 13px weight 600 (in-progress) | 13px (pending)
- **Color**: Completed Light: `#5c5c5c` | In-progress Light: `#1a1a1a`, weight 600 | Pending Light: `#8a8a8a`

##### Retry Badge (In-Progress Only)
- **Margin left**: auto
- **Font size**: 11px
- **Font weight**: 600
- **Padding**: 2px 8px
- **Border radius**: 10px
- **Background**: Light: `#fff7ea` | Dark: `rgba(255,255,255,.1)`
- **Border**: 1px, Light: `#efd9ae` | Dark: `rgba(255,255,255,.2)`
- **Color**: Light: `#8a5300` | Dark: `#d4a976`
- **Text**: "Retrying · attempt 2 of 3"

### 3.4 Footer Section
- **Display**: Flex, justify-between, align-items center
- **Padding**: 16px 22px 18px
- **Margin top**: 8px

#### Info Text
- **Display**: Flex, gap 7px, align-items center
- **Font size**: 11.5px
- **Color**: Light: `#8a8a8a` | Dark: `rgba(255,255,255,.46)`
- **Icon**: Info, 14px
- **Text**: "Takes 1–3s. Some monitors are slow."

#### Cancel Button
- **Height**: 32px
- **Padding**: 0 16px
- **Border radius**: 6px
- **Border**: 1px, Light: `rgba(0,0,0,.16)` | Dark: `rgba(255,255,255,.16)`
- **Background**: Light: `#fbfbfb` | Dark: `rgba(255,255,255,.05)`
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Font size**: 13px
- **Font weight**: 600
- **Cursor**: pointer

---

## 4. CRITICAL STATE DIALOGS

### 4.1 "You'll Be Blind" Dialog

#### Header Icon
- **Size**: 38px circle
- **Background**: Light: `#fdf1f0` | Dark: `#371d1b`
- **Color**: Light: `#c42b1c` | Dark: `#ff99a0`
- **Icon**: Alert, 21px

#### Title
- **Text**: "This PC will go blind"
- **Font size**: 16px
- **Font weight**: 600

#### Body Text
- **Font size**: 12.5px
- **Line height**: 1.5
- **Color**: Light: `#5c5c5c` | Dark: `rgba(255,255,255,.73)`
- **Margin top**: 6px
- **Bold sections** (mentions of monitor names/PC names): Light: `#1a1a1a` | Dark: `#fff`

#### Warning Callout
- **Display**: Flex, gap 8px, align-items flex-start
- **Padding**: 9px 11px
- **Margin top**: 12px
- **Border radius**: 7px
- **Background**: Light: `#fff7ea` | Dark: `#312a18`
- **Border**: 1px, Light: `#efd9ae` | Dark: `#544326`
- **Icon**: Info, 15px, Light: `#b26a00` | Dark: `#e7a93c`
- **Text**: Font 12px, line-height 1.45, Light: `#8a5300` | Dark: `#f2c879`

#### Buttons (Footer)
- **Gap**: 9px
- **Padding**: 18px 22px 20px

##### Cancel Button
- **Height**: 32px, Padding 0 16px
- **Border radius**: 6px
- **Border**: 1px, Light: `rgba(0,0,0,.16)` | Dark: `rgba(255,255,255,.16)`
- **Background**: Light: `#fbfbfb` | Dark: `rgba(255,255,255,.05)`
- **Color**: Light: `#1a1a1a` | Dark: `#fff`
- **Font**: 13px, weight 600

##### Danger Button ("Switch and go blind")
- **Height**: 32px, Padding 0 16px
- **Border radius**: 6px
- **Border**: 1px, transparent
- **Background**: Light: `#c42b1c` | Dark: `#e04a3c`
- **Color**: `#fff`
- **Font**: 13px, weight 600

---

### 4.2 "Stranded" Dialog

#### Header Icon
- **Size**: 38px circle
- **Background**: Light: `#fff7ea` | Dark: `#312a18`
- **Color**: Light: `#b26a00` | Dark: `#e7a93c`
- **Icon**: Alert, 21px

#### Title
- **Text**: "Center 32" is stranded"
- **Font size**: 16px
- **Font weight**: 600

#### Body Text
- **Font size**: 12.5px
- **Line height**: 1.5
- **Color**: Light: `#5c5c5c` | Dark: `rgba(255,255,255,.73)`
- **Margin top**: 6px
- **Bold sections** (PC/monitor names): Light: `#1a1a1a` | Dark: `#fff`

#### Info Callout
- **Display**: Flex, gap 9px, align-items flex-start
- **Padding**: 11px
- **Margin top**: 12px
- **Border radius**: 7px
- **Background**: Light: `#eaeaea` | Dark: `#272727`
- **Icon**: Power, 16px, Light: `#5c5c5c` | Dark: `rgba(255,255,255,.73)`
- **Text**: Font 12px, line-height 1.5, Light: `#5c5c5c` | Dark: `rgba(255,255,255,.73)`
- **Bold instructions**: Light: `#1a1a1a` | Dark: `#fff`

#### Buttons
- **Gap**: 9px, Padding 18px 22px 20px

##### Secondary Button ("Retry detection")
- Same styling as Cancel

##### Primary Button ("Got it")
- **Background**: Light: `#107c41` | Dark: `#4abc7c`
- **Color**: Light: `#fff` | Dark: `#07210f`

---

## 5. TILE SIZE REFERENCE (Physical Desk Layout)

### Light Theme Tiles
- **Left 27" tile**: 130px wide × 84px tall
- **Center 32" tile**: 172px wide × 104px tall
- **Right 27" tile**: 130px wide × 84px tall
- **Border radius**: 8px
- **Border**: 2px, color matches PC
- **Background**: `#fff`
- **Box shadow**: 0 2px–3px 8px–12px `rgba(0,0,0,.1–.13)`
- **Owner badge padding**: 2px 8px (or 2px 7px), font-size 10–10.5px

### Dark Theme Tiles
- **Sizes same as above**
- **Background**: `#2b2b2b`
- **Border**: 2px, matches PC color
- **Owner badge styling**: Similar to light but with dark backgrounds

---

## 6. COLOR PALETTE — BY ENTITY

### PC/Owner Colors
| Owner | Light Hex | Dark Hex | Usage |
|-------|-----------|----------|-------|
| Work | `#117c44` | `#4abc7c` | Primary green |
| Gaming | `#3a6fb0` | `#5a93d6` | Blue |
| Home-lab | `#0e8a8a` | `#0e8a8a` (or `#9bc1ea`) | Teal/cyan |

### System Colors
| Purpose | Light | Dark |
|---------|-------|------|
| Accent (primary) | `#107c41` | `#4abc7c` |
| Accent bg | `#ecf6ef` | `rgba(74,188,124,.16)` |
| Accent border | `#bfe0cb` | `rgba(74,188,124,.36)` |
| Danger | `#c42b1c` | `#e04a3c` |
| Danger bg | `#fdf1f0` | `#371d1b` |
| Warn | `#b26a00` | `#e7a93c` |
| Warn bg | `#fff7ea` | `#312a18` |
| Text primary | `#1a1a1a` | `#fff` |
| Text secondary | `#5c5c5c` | `rgba(255,255,255,.73)` |
| Text tertiary | `#8a8a8a` | `rgba(255,255,255,.46)` |
| Bg primary | `#f3f3f3`–`#f8f8fa` | `#1f1f1f`–`#2b2b2b` |
| Bg secondary | `#eaeaea` | `#272727` |
| Bg layer | `#fff` | `#2b2b2b` |
| Border (subtle) | `rgba(0,0,0,.06–.09)` | `rgba(255,255,255,.08–.09)` |

---

## 7. TYPOGRAPHY & SPACING DEFAULTS

### Font Family
- **Preferred**: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif (main content)
- **Display**: 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif (titles)
- **Monospace** (codes): ui-monospace, 'Cascadia Mono', monospace

### Key Font Sizes
| Use | Size | Weight |
|-----|------|--------|
| Window title | 11.5px | 600 |
| Tray header | 14px | 600 |
| Dialog title | 16px | 600 |
| Monitor name (tile/row) | 13–13.5px | 600 |
| Section label | 10–10.5px | 700 |
| Body text | 12–12.5px | 400–500 |
| Small text | 11–11.5px | 400–600 |
| Tiny label | 10px | 700 |

### Spacing
| Element | Top/Left | Right/Bottom |
|---------|----------|--------------|
| Window padding | 15px 20px | 18px (light) |
| Container padding | 12–14px | 12–14px |
| Row gap (flex) | 7–14px | — |
| Section gap | 8–11px | — |
| Border radius | 6–14px | (context-dependent) |

---

## 8. INTERACTIVE STATES & ANIMATIONS

### Hover States
- **PC/Monitor rows**: Subtle background tint `rgba(0,0,0,.02–.04)` (light)
- **Preset buttons**: Border + bg lighten slightly
- **Cursor**: pointer on all clickable elements

### Active States (Preset)
- **Selected preset**: Green background `#e8f3ec` with border `#bfe0cb`

### Animations
- **Spinner (refresh icon)**: `@keyframes sh-spin { to { transform: rotate(360deg) } }` — 1s linear infinite
- **Loading border (circular)**: `@keyframes sh-spin` — 0.8s linear infinite (same rotation)
- **Pulse (status dot)**: `@keyframes sh-pulse { 0%, 100% { box-shadow: 0 0 0 0 rgba(...,.4) } 70% { box-shadow: 0 0 0 12px rgba(...,0) } }` — active state

### Disabled States
- **Opacity**: 0.5 or reduced color intensity
- **Cursor**: default (not pointer)
- **Border/bg**: Reduced contrast

---

## 9. EDGE CASES & VARIANTS

### Offline PC/Monitor
- **Dot**: `#8a8a8a` (gray)
- **Box shadow**: None or lighter
- **Text opacity**: Reduced
- **Clickable**: False (disabled)

### Stranded Monitor (Icon coloring)
- **Icon**: Same as owner but may show caution indicator
- **Border**: Amber/warn color option

### Drag-to-Reassign Interaction
- **Feedback**: Dragged cable endpoint becomes bold/solid
- **Target highlight**: Monitor tile border brightens to accent color
- **Drop zone**: Entire tile becomes drop target (full 230px wide)
- **Visual**: Connection line animates to new target on release

---

## 10. RESPONSIVE / CROSS-PLATFORM NOTES

### Windows 11 Fluent
- Accent color: `#107c41` (green, embedded in gradient logos)
- Corners: 9px radius (soft, matching Fluent design)
- Shadows: Emphasis on depth via shadow (not stroke)

### Dark Mode Implementation
- CSS variable override block at document root or per-element
- **Variables used**: `--bg`, `--bg-2`, `--text`, `--text-2`, `--text-3`, `--stroke`, `--layer`, `--accent`, `--accent-text`, `--ctl-stroke`, `--subtle`, `--warn-bg`, `--warn-stroke`, `--warn-text`, `--danger`, `--danger-bg`, `--danger-text`, `--ok-bg`, `--ok-text`

### macOS/GNOME Adaptation
- **Header**: Adjusted to platform chrome (menu bar, top bar)
- **Tray anchor**: Platform-specific (menu-bar for macOS, top-bar applet for GNOME)
- **Flyout arrow**: Triangle pseudo-element positioned above/below
- **Font family**: System-ui fallback ensures native rendering per OS
- **Accent**: Stays `#107c41` across platforms for brand consistency



**Open questions:**
- Should per-monitor 'target picker' (when clicking monitor to re-route) be a context menu or in-place dropdown menu? Spec shows dropdown chevron but interaction model not fully detailed.
- Drag-to-reassign cable endpoint—should there be a visual ghost/preview of the connection before drop, or only highlight the target tile?
- For very long monitor names or specs (>30 chars), should overflow be truncated with ellipsis, wrapped, or scrollable?
- Are the preset buttons in the tray flyout always visible, or should they be in a collapsible section below monitors?
- What is the keyboard shortcut display format for 'Capture layout' and other actions (Ctrl+Alt+# shown in screenshot)?
- Should offline/stranded PCs remain in the patchbay view, or be hidden until brought back online?
- For mobile/narrow viewports, should the routing hero collapse to a list view instead of the patchbay layout?

---

## SETTINGS Panel & MONITOR List Specification

# screen-hop SETTINGS Panel & MONITOR LIST — Implementation Spec

## Overview
The Settings interface is a two-column modal dialog accessible from the tray app. The left sidebar provides navigation between five sections (General, Paired peers, Manual hosts, Switching, About), and the right content panel displays settings for the selected section. All controls use Fluent Design principles with segmented controls, toggle switches, and inline action buttons.

---

## Color Tokens & Theme System

### Light Theme (Default)
| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#f3f3f3` | Main background |
| `--bg-2` | `#eaeaea` | Secondary background (headers, nav) |
| `--text` | `#1a1a1a` | Primary text |
| `--text-2` | `#5c5c5c` | Secondary text (labels, subtext) |
| `--text-3` | `#8a8a8a` | Tertiary text (hints, muted) |
| `--stroke` | `rgba(0,0,0,.06)` | Dividers, subtle borders |
| `--ctl-stroke` | `rgba(0,0,0,.16)` | Control borders |
| `--layer` | `#fff` | Card/layer background |
| `--subtle` | `#fbfbfb` | Subtle backgrounds |
| `--accent` | `#107c41` | Action/active state (Microsoft green) |
| `--accent-text` | `#0e6b36` | Accent text (darker green) |
| `--accent-soft` | `#e8f3ec` | Accent background (light green wash) |
| `--accent-soft-stroke` | `#bfe0cb` | Accent border (medium green) |
| `--ok-text` | `#1aa251` | Success/online indicator |
| `--danger-text` | `#b22a1c` | Error/revoke button text |
| `--danger-stroke` | `#f0cbc7` | Error button border |
| `--warn-bg` | `#312a18` | Warning background |
| `--warn-accent` | `#e7a93c` | Warning accent |
| `--warn-text` | `#f2c879` | Warning text |

### Dark Theme Overrides
| Token | Light | Dark | Usage |
|-------|-------|------|-------|
| `--bg` | `#f3f3f3` | `#1f1f1f` | Main background |
| `--bg-2` | `#eaeaea` | `#272727` | Secondary background |
| `--text` | `#1a1a1a` | `#fff` | Primary text |
| `--text-2` | `#5c5c5c` | `rgba(255,255,255,.73)` | Secondary text |
| `--text-3` | `#8a8a8a` | `rgba(255,255,255,.46)` | Tertiary text |
| `--stroke` | `rgba(0,0,0,.06)` | `rgba(255,255,255,.08)` | Dividers |
| `--ctl-stroke` | `rgba(0,0,0,.16)` | `rgba(255,255,255,.16)` | Control borders |
| `--layer` | `#fff` | `#2b2b2b` | Card/layer background |
| `--subtle` | `#fbfbfb` | `rgba(255,255,255,.05)` | Subtle backgrounds |
| `--accent` | `#107c41` | `#4abc7c` | Action color |
| `--accent-soft` | `#e8f3ec` | `rgba(74,188,124,.16)` | Accent wash |
| `--ok-text` | `#1aa251` | `#4abc7c` | Success indicator |
| `--danger` | — | `#e04a3c` | Error button |
| `--danger-bg` | — | `#371d1b` | Error alert background |

---

## SETTINGS Panel Layout

### Container
- **Position**: Absolute, `left:720px; top:1270px`
- **Size**: `width:720px` (fixed)
- **Styling**:
  - `border-radius: 9px`
  - `box-shadow: 0 12px 36px rgba(0,0,0,.14)`
  - `background: var(--bg, #f3f3f3)`
  - `border: 1px solid rgba(0,0,0,.08)`
  - `color: var(--text, #1a1a1a)`
  - `font-family: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif`

### Header Bar
- **Height**: `36px`
- **Padding**: `0 10px`
- **Background**: `var(--bg-2, #eaeaea)`
- **Border-bottom**: `1px solid var(--stroke, rgba(0,0,0,.06))`
- **Layout**: `display: flex; align-items: center`
- **Logo icon**: `width: 16px; height: 16px; border-radius: 4px`
  - Background: `linear-gradient(160deg, #117c44, #0c5e32)`
  - SVG inside: `width: 11px; height: 11px; color: #fff`
- **Title text**: `font-size: 11.5px; font-weight: 600; color: var(--text-2, #5c5c5c); margin-left: 8px`
  - **Literal**: "screen-hop — Settings"

### Main Content Area
- **Layout**: `display: flex; min-height: 430px`
- **Sidebar width**: `164px`
- **Content width**: flexible (flex:1)

#### Left Sidebar Navigation

**Styling**:
- `width: 164px; flex: none`
- `padding: 12px 8px`
- `background: var(--bg, #f3f3f3)`
- `border-right: 1px solid var(--stroke, rgba(0,0,0,.06))`

**Each nav item**:
- **Dimensions**: `display: flex; align-items: center; gap: 9px; padding: 8px 10px; border-radius: 6px`
- **Font**: `font-size: 13px; font-weight: 600 (active) or 500 (inactive)`

**Active nav item (e.g., "General")**:
- `background: var(--accent-soft, #e8f3ec)`
- `color: var(--accent-text, #0e6b36)`
- **Left accent bar**: `position: absolute; left: 0; top: 8px; bottom: 8px; width: 3px; border-radius: 2px; background: var(--accent, #107c41)`

**Inactive nav items**:
- `color: var(--text-2, #5c5c5c)`
- **Hover state** (visual only): subtle background

**Nav Items** (with icons, left-to-right):
1. `General` — gear icon (#i-gear)
2. `Paired peers` — link icon (#i-link)
3. `Manual hosts` — network icon (#i-net)
4. `Switching` — refresh icon (#i-refresh)
5. `About` — info icon (#i-info)

#### Right Content Panel

- `flex: 1; padding: 16px 20px; overflow: auto`

---

## SETTINGS SECTIONS

### Section 1: GENERAL

#### Header
- **Font**: `16px; font-weight: 600; margin-bottom: 12px`
- **Text**: "General"

#### Settings Group Container
- `border-radius: 9px`
- `background: var(--layer, #fff)`
- `border: 1px solid var(--stroke, rgba(0,0,0,.08))`
- `overflow: hidden`

#### Row 1: Start with Windows (Toggle)
- **Layout**: `display: flex; align-items: center; gap: 12px; padding: 13px 15px`
- **Icon**: 16px power icon (#i-power), `color: var(--text-2, #5c5c5c)`
- **Label**: `font-size: 13px; font-weight: 600; color: var(--text, #1a1a1a)`
  - **Literal**: "Start with Windows"
- **Description**: `font-size: 11.5px; color: var(--text-3, #8a8a8a); margin-top: 4px`
  - **Literal**: "Launch to the system tray on sign-in"
- **Toggle switch**: 
  - `width: 40px; height: 21px; border-radius: 11px`
  - **On state**: `background: var(--accent, #107c41)`
  - **Thumb**: `position: absolute; top: 3px; right: 3px; width: 15px; height: 15px; border-radius: 50%; background: #fff`
- **Divider after**: `height: 1px; background: var(--stroke, rgba(0,0,0,.06)); margin: 0 15px`

#### Row 2: Theme (Segmented Control)
- **Layout**: `display: flex; align-items: center; gap: 12px; padding: 13px 15px`
- **Icon**: 16px sun icon (#i-sun), `color: var(--text-2, #5c5c5c)`
- **Label**: `font-size: 13px; font-weight: 600`
  - **Literal**: "Theme"
- **Segmented control**:
  - `display: flex; border-radius: 7px; border: 1px solid var(--ctl-stroke, rgba(0,0,0,.16)); overflow: hidden`
  - `font-size: 12px; font-weight: 600`
  - **Segments** (3 options):
    1. "System" — `padding: 5px 11px; color: var(--text-2, #5c5c5c); cursor: pointer`
    2. "Light" (ACTIVE) — `padding: 5px 11px; background: var(--accent, #107c41); color: #fff; cursor: pointer`
    3. "Dark" — `padding: 5px 11px; color: var(--text-2, #5c5c5c); cursor: pointer`
- **Divider after**: `height: 1px; background: var(--stroke, rgba(0,0,0,.06)); margin: 0 15px`

#### Row 3: Global hotkeys (Toggle)
- **Layout**: `display: flex; align-items: center; gap: 12px; padding: 13px 15px`
- **Icon**: 16px keyboard icon (#i-keyboard), `color: var(--text-2, #5c5c5c)`
- **Label**: `font-size: 13px; font-weight: 600`
  - **Literal**: "Global hotkeys"
- **Description**: `font-size: 11.5px; color: var(--text-3, #8a8a8a)`
  - **Literal**: "Ctrl+Alt+1…4 apply presets from anywhere"
- **Toggle switch**: `width: 40px; height: 21px; border-radius: 11px; background: var(--accent, #107c41)`
  - **Thumb**: `position: absolute; top: 3px; right: 3px; width: 15px; height: 15px; border-radius: 50%; background: #fff`

---

### Section 2: PAIRED PEERS

#### Header
- **Font**: `16px; font-weight: 600; margin: 18px 0 12px`
- **Text**: "Paired peers"

#### Settings Group Container
- `border-radius: 9px`
- `background: var(--layer, #fff)`
- `border: 1px solid var(--stroke, rgba(0,0,0,.08))`

#### Peer Row Template (Repeating)

Each peer row contains:

**Row Layout**: `display: flex; align-items: center; gap: 11px; padding: 11px 15px`

**Colored Icon Badge**:
- `width: 30px; height: 30px; border-radius: 7px`
- **Flex**: `flex: none; display: flex; align-items: center; justify-content: center`
- **Icon**: 17px (#i-pc for PCs, #i-server for servers)

**Peer Information** (flex:1):
- **Peer name**: `font-size: 13px; font-weight: 600; color: var(--text, #1a1a1a)`
- **IP + key status**: `font-size: 11px; color: var(--text-3, #8a8a8a); font-family: ui-monospace, monospace; margin-top: 2px`
- **THIS PC badge** (on current machine only):
  - `font-size: 10px; font-weight: 700`
  - `background: var(--accent-soft, #e8f3ec)`
  - `color: var(--accent-text, #0e6b36)`
  - `padding: 1px 6px; border-radius: 8px; margin-left: 4px`
  - **Literal**: "THIS PC"

**Status Indicator** (left-to-right flex display):
- **Dot**: `width: 7px; height: 7px; border-radius: 50%; margin-right: 5px`
- **Text**: `font-size: 11.5px; font-weight: 600`
- **Colors**:
  - Online: `background: var(--ok-text, #1aa251); color: var(--ok-text, #0e6b36)`
    - **Literal**: "online"
  - Offline: `background: var(--text-3, #8a8a8a); color: var(--text-3, #8a8a8a)`
    - **Literal**: "offline 2m" (example)

**Revoke Button** (on non-THIS-PC rows):
- `height: 28px; padding: 0 12px; border-radius: 6px`
- `border: 1px solid var(--danger-stroke, #f0cbc7)`
- `background: transparent`
- `color: var(--danger-text, #b22a1c)`
- `font-size: 12px; font-weight: 600; cursor: pointer`
- **Literal**: "Revoke"

**Example Rows**:

1. **Work (THIS PC)**
   - Icon: PC with green background `rgba(17,124,68,.13); color: #117c44`
   - Name: "Work"
   - Badge: "THIS PC"
   - IP: "192.168.1.10 · paired key ✓"
   - Status: "online" (green dot)
   - No Revoke button

2. **Gaming**
   - Icon: PC with blue background `rgba(58,111,176,.13); color: #3a6fb0`
   - Name: "Gaming"
   - IP: "192.168.1.42 · paired key ✓"
   - Status: "online" (green dot)
   - Revoke button: yes

3. **Home-lab**
   - Icon: Server with cyan background `rgba(14,138,138,.14); color: #0e8a8a`
   - Name: "Home-lab"
   - IP: "192.168.1.17 · paired key ✓"
   - Status: "offline 2m" (gray dot)
   - Revoke button: yes

**Divider between rows**: `height: 1px; background: var(--stroke, rgba(0,0,0,.06)); margin: 0 15px`

---

### Section 3: SWITCHING

#### Header
- **Font**: `16px; font-weight: 600; margin: 18px 0 12px`
- **Text**: "Switching"

#### Row 1: Verify input after switching (Toggle)
- **Layout**: `display: flex; align-items: center; gap: 12px; padding: 13px 15px`
- **Icon**: 16px check icon (#i-check), `color: var(--text-2, #5c5c5c)`
- **Label**: `font-size: 13px; font-weight: 600`
  - **Literal**: "Verify input after switching"
- **Description**: `font-size: 11.5px; color: var(--text-3, #8a8a8a)`
  - **Literal**: "Re-read the monitor to confirm it actually changed"
- **Toggle**: `width: 40px; height: 21px; border-radius: 11px; background: var(--accent, #107c41)`
- **Divider after**: `height: 1px; background: var(--stroke, rgba(0,0,0,.06)); margin: 0 15px`

#### Row 2: DDC read timeout · retries (Input + Label)
- **Layout**: `display: flex; align-items: center; gap: 12px; padding: 13px 15px`
- **Icon**: 16px bolt icon (#i-bolt), `color: var(--text-2, #5c5c5c)`
- **Label**: `font-size: 13px; font-weight: 600`
  - **Literal**: "DDC read timeout · retries"
- **Description**: `font-size: 11.5px; color: var(--text-3, #8a8a8a)`
  - **Literal**: "Raise for slow monitors that need extra time"
- **Input display**:
  - `display: flex; align-items: center; height: 30px`
  - `border: 1px solid var(--ctl-stroke, rgba(0,0,0,.16)); border-radius: 6px; overflow: hidden`
  - `font-size: 12.5px`
  - **Timeout value**: `padding: 0 10px; color: var(--text-2, #5c5c5c)`
    - **Literal**: "500 ms"
  - **Separator**: `width: 1px; align-self: stretch; background: var(--stroke, rgba(0,0,0,.1))`
  - **Retry count**: `padding: 0 10px; color: var(--text-2, #5c5c5c)`
    - **Literal**: "×3"
- **Divider after**: `height: 1px; background: var(--stroke, rgba(0,0,0,.06)); margin: 0 15px`

#### Row 3: External fallback tool (File path + Browse button)
- **Layout**: `display: flex; align-items: center; gap: 12px; padding: 13px 15px`
- **Icon**: 16px link icon (#i-link), `color: var(--text-2, #5c5c5c)`
- **Label**: `font-size: 13px; font-weight: 600`
  - **Literal**: "External fallback tool"
- **Description**: `font-size: 11.5px; color: var(--text-3, #8a8a8a); font-family: ui-monospace, monospace`
  - **Literal**: "C:\Tools\ControlMyMonitor.exe"
- **Browse button**:
  - `height: 30px; padding: 0 13px; border-radius: 6px`
  - `border: 1px solid var(--ctl-stroke, rgba(0,0,0,.16))`
  - `background: var(--subtle, #fbfbfb)`
  - `color: var(--text, #1a1a1a)`
  - `font-size: 12.5px; font-weight: 600; cursor: pointer`
  - **Literal**: "Browse"

---

## MONITOR LIST (Canvas/Detail View)

### Container
- **Position**: Part of Flyout or Canvas view
- **Styling**: Similar to Settings modal
- **Background**: `var(--layer, #fff)` (light) or `#2b2b2b` (dark)
- `border-radius: 9px`
- `box-shadow: 0 12px 36px rgba(0,0,0,.14)` (light) or `0 14px 40px rgba(0,0,0,.5)` (dark)

### Monitor List Section Header
- **Font**: `10.5px; font-weight: 700; letter-spacing: 0.6px`
- **Color**: `color: rgba(255,255,255,.46)` (dark) or `var(--text-3, #8a8a8a)` (light)
- **Padding**: `padding: 9px 8px 7px`
- **Literal**: "MONITORS"

### Monitor Row Template

**Layout**: `display: flex; align-items: center; gap: 11px; padding: 8px 9px; border-radius: 7px`

#### Monitor Icon Badge
- `width: 34px; height: 34px; border-radius: 8px`
- **Light background**: `background: rgba(255,255,255,.05); border: 1px solid rgba(255,255,255,.09)` (dark mode)
- **Light mode**: `background: var(--subtle, #fbfbfb); border: 1px solid var(--stroke, rgba(0,0,0,.08))`
- **Icon**: 18px monitor icon (#i-monitor)
- **Color codes**:
  - Green (Work): `#117c44` or `#4abc7c` (dark)
  - Blue (Gaming): `#3a6fb0` or `#5a93d6` (dark)
  - Cyan (Home-lab): `#0e8a8a` or similar

#### Monitor Information (flex:1)
- **Name**: `font-size: 13.5px; font-weight: 600`
  - **Examples**: "Left 27\"", "Center 32\"", "Right 27\""
- **Details**: `font-size: 11px; color: var(--text-3, #8a8a8a)` (light) or `rgba(255,255,255,.46)` (dark)
  - **Format**: "Model · Connector · Resolution"
  - **Examples**: "Dell U2723QE · DP-1 · 1440p", "LG 32UN880 · HDMI-2 · 4K"

#### Preset Badge
- `display: flex; align-items: center; gap: 6px`
- `padding: 3px 8px 3px 7px`
- `border-radius: 13px`
- **Light mode**: `background: #fbfbfb; border: 1px solid rgba(0,0,0,.08)`
- **Dark mode**: `background: rgba(255,255,255,.05); border: 1px solid rgba(255,255,255,.09)`
- **Dot indicator**: `width: 7px; height: 7px; border-radius: 50%;` (color-coded)
- **Preset name**: `font-size: 12px; font-weight: 600` (Light dark mode: `#fff`)
  - **Examples**: "Work", "Gaming"

#### Chevron (clickable indicator)
- **Icon**: 15px chevron-down icon (#i-chev-d)
- **Color**: `rgba(255,255,255,.46)` (dark) or `var(--text-3, #8a8a8a)` (light)

### Example Monitor List (Dark Theme)

**Header**: "MONITORS" (light gray, uppercase, 10.5px)

1. **Left 27"**
   - Icon: Green badge with monitor icon
   - Name: "Left 27\" (white text)"
   - Details: "Dell U2723QE · DP-1 · 1440p" (light gray)
   - Badge: Green dot + "Work" (white)
   - Chevron

2. **Center 32"**
   - Icon: Blue badge with monitor icon
   - Name: "Center 32\""
   - Details: "LG 32UN880 · HDMI-2 · 4K"
   - Badge: Blue dot + "Gaming"
   - Chevron

3. **Right 27"**
   - Icon: Green badge with monitor icon
   - Name: "Right 27\""
   - Details: "Dell U2723QE · DP-2 · 1440p"
   - Badge: Green dot + "Work"
   - Chevron

---

## Typography & Spacing

### Font Family (Global)
- `'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif` (primary)
- `ui-monospace, monospace` (code/IP addresses)

### Font Sizes & Weights
- **Section headers**: `16px, font-weight: 600`
- **Nav items**: `13px, font-weight: 600 (active) / 500 (inactive)`
- **Setting labels**: `13px, font-weight: 600`
- **Descriptions**: `11.5px, color: var(--text-3, #8a8a8a)`
- **Monitor names**: `13.5px, font-weight: 600`
- **Monitor details**: `11px, color: muted`
- **Badge text**: `12px, font-weight: 600`
- **Captions (MONITORS)**: `10.5px, font-weight: 700, letter-spacing: 0.6px`

### Spacing
- **Row padding**: `13px 15px` (settings rows), `11px 15px` (peer rows)
- **Gap between icon & content**: `12px` (settings), `11px` (peers)
- **Section margin-top**: `18px` (between sections)
- **Section margin-bottom**: `12px` (before group container)

### Borders & Radii
- **Container border-radius**: `9px`
- **Group border-radius**: `9px`
- **Control border-radius**: `7px` (buttons), `6px` (small buttons), `11px` (toggle radius)
- **Icon badge border-radius**: `8px` (settings), `7px` (peers/monitors)
- **Nav border-radius**: `6px`

---

## Interactive States

### Toggle Switch
- **Off state**: `background: #ccc` or `rgba(0,0,0,.2)` (implied from design)
- **On state**: `background: var(--accent, #107c41)`
- **Thumb animation**: smooth transition

### Segmented Control
- **Active segment**: `background: var(--accent, #107c41); color: #fff`
- **Inactive segment**: `background: transparent; color: var(--text-2, #5c5c5c)`
- **Hover**: subtle opacity change

### Buttons
- **Primary action** (Apply): `background: var(--subtle, #fbfbfb); border: 1px solid var(--ctl-stroke, rgba(0,0,0,.16)); color: var(--text, #1a1a1a)`
- **Danger** (Revoke): `background: transparent; border: 1px solid var(--danger-stroke, #f0cbc7); color: var(--danger-text, #b22a1c)`
- **Browse**: `background: var(--subtle, #fbfbfb); border: 1px solid var(--ctl-stroke, rgba(0,0,0,.16))`

---

## Layout Constraints

- **Settings modal width**: `720px` (fixed)
- **Settings modal min-height**: `430px`
- **Sidebar width**: `164px` (fixed)
- **Content area**: flexible, with `overflow: auto` for vertical scrolling
- **Monitor list width**: varies, typically `400–450px` for detail panel
- **Peer row height**: `~66px` (icon + 2-line text)
- **Setting row height**: `~48px` (icon + label + control)

---

## Key Literal Strings

**Settings tabs (nav)**:
- General
- Paired peers
- Manual hosts
- Switching
- About

**General section**:
- "Start with Windows" + "Launch to the system tray on sign-in"
- "Theme"
- "Global hotkeys" + "Ctrl+Alt+1…4 apply presets from anywhere"
- Segmented: "System", "Light", "Dark"

**Paired peers section**:
- "Paired peers"
- "THIS PC" (badge)
- Status: "online" (with dot), "offline Xm"

**Switching section**:
- "Verify input after switching" + "Re-read the monitor to confirm it actually changed"
- "DDC read timeout · retries" + "Raise for slow monitors that need extra time"
- Display: "500 ms", "×3"
- "External fallback tool" + "C:\Tools\ControlMyMonitor.exe"
- "Browse" (button)

**Monitor list**:
- "MONITORS" (section header)
- Monitor names: "Left 27\"", "Center 32\"", "Right 27\""
- Details format: "Model · Connector · Resolution"
- Preset badges: "Work", "Gaming", etc.

---

## Responsive & Accessibility Notes

- All controls use high-contrast colors (WCAG AA compliant)
- Focus indicators should follow Fluent Design: typically a 2px outline in accent color
- Toggle switches should have explicit role `switch` and `aria-checked` state
- Segmented controls should use role `tablist` with `tab` items
- Icon spacing maintains visual alignment despite varying text lengths
- Monospace font for IP addresses and technical values ensures consistency



**Open questions:**
- Should the Settings modal be scrollable vertically if content exceeds ~430px, or does it expand?
- Are there hover/focus states for nav items that should be defined (currently only showing active state)?
- What is the inactive toggle background color (gray)? Design shows on/off but not the off-state styling.
- Should the 'Browse' button for external fallback tool trigger a file picker, and if so, does it filter for .exe files?
- Is there a maximum width for the Switching section when describing very long file paths?
- For dark mode, are the icon badge backgrounds (rgba 5% and 14%) semantic or just visual fallbacks?

---

## screen-hop Critical States & Dark Theme Specification

# screen-hop Critical States & Dark Theme Specification

## CRITICAL STATE DIALOGS

### D1: Switching Progress (In Progress, Can Retry)

**Dialog Size**: 430px wide  
**Shadow**: `0 16px 44px rgba(0,0,0,.17)` (light), `0 16px 44px rgba(0,0,0,.55)` (dark)  
**Background**: `#fff` (light), `#2b2b2b` (dark)  
**Border**: 1px solid `rgba(0,0,0,.08)` (light), `rgba(255,255,255,.08)` (dark)  
**Border Radius**: 9px  
**Text Color**: `#1a1a1a` (light), `#fff` (dark)

**Header Section**:
- Padding: 20px 22px 6px
- Layout: flex, gap 14px
- Icon: 38px circular, `--accent-soft` background (`#e8f3ec` light, `rgba(74,188,124,.16)` dark)
- Icon color: `--accent` (`#107c41` light, `#4abc7c` dark)
- Icon animation: `sh-spin 1s linear infinite`

**Title**: "Switching Center 32""
- Font size: 16px, weight 600

**Source→Target Pills**:
- Font size: 12px
- Padding: 2px 8px
- Border radius: 11px
- "Gaming" pill: `--subtle` background, `--stroke` border
- Arrow icon: `--text-3` color
- "Work" pill: `--accent-soft` background, `--accent-soft-stroke` border, `--accent-text` color

**Progress Items** (vertical, gap 11px):
1. ✓ "Sent DDC/CI input command" — 18px circular badge with checkmark, `--ok-bg` background, `--ok-text` color; text 13px, `--text-2` color
2. Spinner "Waiting for monitor to switch" — 18px circular spinner (border 2px `--accent`, border-right transparent, `sh-spin .8s`); text 13px weight 600; badge on right "Retrying · attempt 2 of 3" with 11px weight 600, `--warn-text` color (`#8a5300` light, `#f2c879` dark), `--warn-bg` background, `--warn-stroke` border
3. Empty circle "Verify active input" — 18px, `--ctl-stroke` border (rgba(0,0,0,.18) light), 13px text `--text-3` color

**Footer**:
- Padding: 16px 22px 18px
- Layout: flex, space-between
- Info: 11.5px text, `--text-3` color, info icon 14px
- Cancel button: 32px height, 0 16px padding, 6px radius, `--ctl-stroke` border, `--subtle` background, `--text` color, 13px weight 600

---

### D2: You'll Be Blind (Red Warning)

**Dialog Size**: 430px wide  
**Icon**: 38px circular, `--danger-bg` background (`#fdf1f0` light, `#371d1b` dark), `--danger` icon color (`#c42b1c` light, `#e04a3c` dark)  
**Icon Type**: Alert triangle

**Title**: "This PC will go blind"

**Body Text** (12.5px, line-height 1.5, `--text-2` color):
"Sending **Right 27"** to Gaming leaves **Work** (this PC) with no display. You won't see this screen after the switch."

**Warning Box**:
- Padding: 9px 11px
- Border radius: 7px
- Background: `--warn-bg` (`#fff7ea` light, `#312a18` dark)
- Border: 1px solid `--warn-stroke` (`#efd9ae` light, `#544326` dark)
- Icon: 15px, `--warn-accent` color, margin-top 1px
- Text: 12px, line-height 1.45, `--warn-text` color (`#8a5300` light, `#f2c879` dark)
- Content: "Keep at least one monitor on Work, or have another PC ready to switch it back."

**Footer Buttons**:
- Layout: flex, justify-end, gap 9px
- Padding: 18px 22px 20px
- **Cancel**: 32px height, 0 16px padding, 6px radius, `--ctl-stroke` border, `--subtle` background, `--text` color
- **Switch and go blind** (Primary): 32px height, 0 16px padding, 6px radius, no border, `--danger` background (`#c42b1c` light, `#e04a3c` dark), white text

---

### D3: Stranded (Amber Warning — Physical Button Required)

**Dialog Size**: 430px wide  
**Icon**: 38px circular, `--warn-bg` background (`#fff7ea` light, `#312a18` dark), `--warn-accent` icon color (`#b26a00` light, `#e7a93c` dark)  
**Icon Type**: Alert triangle

**Title**: "Center 32" is stranded"

**Body Text** (12.5px, line-height 1.5, `--text-2` color):
"It's driven by **Home-lab**, which is offline or asleep. Software can't pull a monitor away from an unreachable PC."

**Physical Button Guidance Box**:
- Padding: 11px
- Border radius: 7px
- Background: `--bg-2` (`#eaeaea` light, `#272727` dark)
- Icon: 16px power icon, `--text-2` color, margin-top 1px
- Text: 12px, line-height 1.5, `--text-2` color
- Content: "Press the monitor's **Input / Source** button on the bezel and choose this PC's connector (**HDMI-2**)."

**Footer Buttons**:
- Layout: flex, justify-end, gap 9px
- Padding: 18px 22px 20px
- **Retry detection**: 32px height, 0 16px padding, 6px radius, `--ctl-stroke` border, `--subtle` background, `--text` color
- **Got it** (Primary): 32px height, 0 16px padding, 6px radius, no border, `--accent` background, white text

---

### D4: DDC/CI Disabled (Red Error)

**Dialog Size**: 430px wide  
**Icon**: 38px circular, `--danger-bg` background (`#fdf1f0` light, `#371d1b` dark), `--danger` icon color (`#c42b1c` light, `#e04a3c` dark)  
**Icon Type**: Close (X) icon

**Title**: "Right 27" isn't responding"

**Body Text** (12.5px, line-height 1.5, `--text-2` color):
"This monitor ignored DDC/CI commands. It's usually off in the on-screen menu."

**Menu Path Guidance**:
- Margin: 4px 22px 0
- Padding: 12px 14px
- Border radius: 8px
- Background: `--layer-2` (`#f6f6f6` light, `#323232` dark)
- Border: 1px solid `--stroke`
- Header: 10.5px weight 700, letter-spacing 0.4px, `--text-3` color, margin-bottom 8px
- Content: 12.5px monospace, `--text` color, flex layout with gap 8px
  - Each breadcrumb button: padding 2px 8px, 5px radius, `--subtle` background, `--stroke` border
  - Icons between: 13px, `--text-3` color
  - Final pill (On): `--accent-soft` background, `--accent-soft-stroke` border, `--accent-text` color, weight 600
  - Path: **Menu** › **Others** › **DDC/CI** › **On**

**Footer Buttons**:
- Layout: flex, justify-end, gap 9px
- Padding: 16px 22px 20px
- **Skip this monitor**: 32px height, 0 16px padding, 6px radius, `--ctl-stroke` border, `--subtle` background, `--text` color
- **Retry** (Primary): 32px height, 0 16px padding, 6px radius, no border, `--accent` background, white text

---

### D5: Preset Partial Failure (Amber — Best-Effort Per-Monitor)

**Dialog Size**: 440px wide  
**Header Icon**: 34px circular, `--warn-bg` background, `--warn-accent` icon color  
**Icon Type**: Alert triangle

**Title**: "Applied "Trading" — 2 of 3 switched"

**Subtitle** (12px, `--text-2` color):
"One monitor didn't respond. Retry it below."

**Status Items** (vertical, no outer border, each with top border 1px `--stroke`):

| Icon | Monitor | Status | Color | Action |
|------|---------|--------|-------|--------|
| ✓ (24px, `--ok-bg` bg, `--ok-text` color) | Center 32" → Gaming | Switched | `--ok-text` | — |
| ✓ (24px, `--ok-bg` bg, `--ok-text` color) | Left 27" → Work | Switched | `--ok-text` | — |
| ✗ (24px, `--danger-bg` bg, `--danger` color) | Right 27" → Work | DDC/CI timeout | `--danger-text` | Retry button (28px, 0 12px padding) |

- Item padding: 10px 0
- Font sizes: 13px for title, 11px for status
- Retry button: 28px height, 0 12px padding, 6px radius, `--ctl-stroke` border, `--subtle` background, `--text` color, 12px weight 600

**Footer**:
- Padding: 14px 22px 18px
- Layout: flex, space-between
- Info: 11px text, `--text-3` color, info icon 13px
- Message: "Presets are best-effort, not all-or-nothing."
- **Done** (Primary): 32px height, 0 18px padding, 6px radius, no border, `--accent` background, white text, 13px weight 600

---

## DARK THEME — COMPLETE TOKEN SET

### CSS Variable Tokens

| Token | Light Value | Dark Value | Usage |
|-------|------------|-----------|-------|
| `--bg` | `#f3f3f3` | `#1f1f1f` | Canvas/window background |
| `--bg-2` | `#eaeaea` | `#272727` | Secondary background (headers, dividers) |
| `--layer` | `#fff` | `#2b2b2b` | Card/panel background |
| `--layer-2` | `#f6f6f6` | `#323232` | Secondary layer (guidance boxes) |
| `--subtle` | `#fbfbfb` | `rgba(255,255,255,.05)` | Inactive button backgrounds, disabled states |
| `--subtle-hover` | N/A | `rgba(255,255,255,.085)` | Hover state for subtle elements |
| `--subtle-press` | `#e8e8e8` | `rgba(255,255,255,.07)` | Pressed state for subtle elements |
| `--stroke` | `rgba(0,0,0,.08)` | `rgba(255,255,255,.09)` | Borders, dividers (neutral) |
| `--ctl-stroke` | `rgba(0,0,0,.16)` | `rgba(255,255,255,.16)` | Control borders (buttons, inputs) |
| `--kbd-bg` | N/A | `rgba(255,255,255,.1)` | Keyboard hint backgrounds |
| `--text` | `#1a1a1a` | `#ffffff` | Primary text |
| `--text-2` | `#5c5c5c` | `rgba(255,255,255,.73)` | Secondary text (descriptions) |
| `--text-3` | `#8a8a8a` | `rgba(255,255,255,.46)` | Tertiary text (labels, hints) |
| `--accent` | `#107c41` | `#4abc7c` | Primary action color (green) |
| `--accent-hover` | N/A | `#57c886` | Hover state for accent buttons |
| `--accent-press` | `#0c5e32` | `#3fa86c` | Pressed state for accent buttons |
| `--accent-text` | `#0e6b36` | `#6fd49a` | Text on accent backgrounds or accent indicators |
| `--on-accent` | N/A | `#07210f` | Text/foreground on accent backgrounds (dark only) |
| `--accent-soft` | `#e8f3ec` | `rgba(74,188,124,.16)` | Soft accent background (badges, highlights) |
| `--accent-soft-stroke` | `#bfe0cb` | `rgba(74,188,124,.36)` | Stroke for soft accent |
| `--menu` | N/A | `#2d2d2d` | Menu/dropdown backgrounds |
| `--danger` | `#c42b1c` | `#e04a3c` | Error/destructive action color (red) |
| `--danger-text` | `#b22a1c` | `#ff99a0` | Text for danger contexts |
| `--danger-bg` | `#fdf1f0` | `#371d1b` | Danger background (alerts, errors) |
| `--danger-stroke` | `#f0cbc7` | `#5a2e2a` | Danger border |
| `--warn-text` | `#8a5300` | `#f2c879` | Warning text |
| `--warn-accent` | `#b26a00` | `#e7a93c` | Warning accent/icon color |
| `--warn-bg` | `#fff7ea` | `#312a18` | Warning background |
| `--warn-stroke` | `#efd9ae` | `#544326` | Warning border |
| `--ok-text` | `#0e6b36` | `#74d08f` | Success text |
| `--ok-bg` | `#ecf6ef` | `#16301e` | Success background |
| `--shadow` | N/A | `rgba(0,0,0,.5)` | Standard shadow (dark only) |
| `--shadow-strong` | N/A | `rgba(0,0,0,.62)` | Strong shadow for modals (dark only) |

### Per-Component Dark Theme Differences

**Dialogs** (when `{{ D }}` is applied to the entire dialog container):
- Shadow increases to `0 16px 44px rgba(0,0,0,.55)` (from `.17`)
- Border becomes `1px solid rgba(255,255,255,.08)` (from `rgba(0,0,0,.08)`)
- Text color: `#fff` (from `#1a1a1a`)
- Nested text emphasis (bold): explicitly set to `color:#fff` in dark dialogs

**Flyout/Tray**:
- Background: `#2b2b2b` (same as `--layer`)
- Divider: `1px solid rgba(255,255,255,.08)`
- Header: uses `--text` (`#fff`), status badge `--accent-text` (`#6fd49a`)
- Monitor items: icon backgrounds use light rgba overlays (`rgba(255,255,255,.05)`)

**Icon Colors** (preserved across themes when using PC/monitor identity colors):
- Work: `#117c44` (light), `#4abc7c` (dark)
- Gaming: `#3a6fb0` (light), `#5a93d6` (dark)
- Home-lab: `#0e8a8a` (light), same in dark UI contexts

### Typography (Unchanged in Dark)

- Font family: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif
- Font weight mapping: 600 (semibold), 700 (bold) — same in both themes
- Font sizes: 10px (small labels) to 18px (page titles) — same in both themes
- Letter spacing: 0.4px–1.5px on labels — same in both themes
- Line height: 1.45–1.5 for body text — same in both themes

### Animation (Same in Both Themes)

- `sh-spin 1s linear infinite` — rotating refresh icon
- `sh-flash 1.1s ease-in-out infinite` — opacity pulse (25% → 100%)
- Durations and timing unchanged

### Spacing & Sizing (Same in Both Themes)

- Dialog padding: 20px 22px (header), 12px 22px (content), 16px 22px 18px (footer)
- Border radius: 9px (dialogs), 6px (buttons), 7px (inline badges)
- Icon sizes: 38px (dialog headers), 34px (partial success), 18px (progress circles)
- Button height: 32px (standard), 28px (compact inline)
- Gap/margin units: 8px, 9px, 11px, 12px (consistent)



**Open questions:**
- Should the preset partial-failure dialog display as amber or yellow? (Currently amber with `--warn-accent`)
- For the 'You'll be blind' dialog in dark mode, should the danger icon use `#e04a3c` (current `--danger`) or the lighter `#ff99a0` (`--danger-text`)?
- Are the PC identity colors (Work: #117c44 light / #4abc7c dark; Gaming: #3a6fb0 / #5a93d6) system-generated or user-configurable?
- Should the 'on-accent' text color `#07210f` (dark only) ever apply to light theme, or is it dark-exclusive?

---

