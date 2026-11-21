# Apple-Level Developer Tool Landing Page Design Standards (2023-2026)

*Compiled from analysis of Linear, Vercel, Raycast, Warp, Arc, Notion, Stripe, Clerk, Apple HIG, and Evil Martians' 100+ dev tool study*

---

## Executive Summary: What Defines "Apple-Level" Design for Developer Tools

Apple-level design for developer tools is **not about decoration** — it's about **engineering precision applied to visual communication**. The defining characteristics across top-tier sites (Linear, Vercel, Raycast, Warp, Arc, Stripe, Clerk) are:

1. **Restraint as a system** — Every token, weight, color, and pixel serves a functional purpose. No decorative gradients, no "hero illustrations" that don't demonstrate product.
2. **Dark-mode-native or light-mode-native** — Not "both modes supported" but **one native medium** designed from the ground up (Linear/Raycast = dark-native; Vercel/Notion/Stripe = light-native; Warp = warm-dark-native).
3. **Typography as brand infrastructure** — Custom or heavily customized variable fonts with OpenType features (`cv01`, `ss01`, `ss03`, `liga`) enabled globally. Negative letter-spacing at display sizes is universal.
4. **Shadow-as-border / elevation-as-color** — Traditional CSS borders replaced by multi-layer shadow stacks (Vercel, Stripe, Clerk) or semi-transparent white borders on dark (Linear, Raycast).
5. **Color as semantic signal, never decoration** — One brand accent (Linear indigo, Vercel workflow trio, Raycast red, Stripe purple, Clerk purple). Everything else is a warm/cool neutral scale.
6. **Motion that guides, not decorates** — 100-450ms durations, spring physics for content swaps, `prefers-reduced-motion` gating, scroll-driven animations for non-essential effects.
7. **Performance as a design constraint** — Critical CSS inlined, font-display: swap, WOFF2 only, preload hero assets, no-JS fallbacks.

---

## 1. Color Systems: Dark/Light Mode, Accent Colors, Semantic Tokens

### 1.1 The "Native Medium" Philosophy

| Site | Native Mode | Background | Text | Philosophy |
|------|-------------|------------|------|------------|
| **Linear** | Dark | `#08090a` (marketing), `#0f1011` (panels) | `#f7f8f8` (not pure white) | Darkness as canvas; content emerges via luminance steps |
| **Raycast** | Dark | `#07080a` (blue-tinted near-black) | `#f9f9f9` | macOS-native instrument feel; positive letter-spacing for readability |
| **Warp** | Dark (warm) | Warm near-black (charred wood tone) | `#faf9f6` (warm parchment) | Campfire warmth; lifestyle brand meets dev tool |
| **Vercel** | Light | `#ffffff` (pure white) | `#171717` (not pure black) | Gallery emptiness; compiler-like minimalism |
| **Notion** | Light | `#ffffff` / `#f6f5f4` (warm white) | `rgba(0,0,0,0.95)` | Warm paper analogy; approachable minimalism |
| **Stripe** | Light | `#ffffff` | `#061b31` (deep navy) | Financial-grade warmth; luxury through lightness |
| **Clerk** | Both (class-based) | `#f7f7f8` / `#131316` | `#131316` / `#ffffff` | Seed system; purple anchor `#6c47ff` |
| **Arc** | Light (gradient) | Frosted glass over theme gradients | `#1a1a1f` | Translucency + single saturated gradient as mood |

### 1.2 Semantic Color Token Patterns

**All top systems use a 4-6 step neutral scale + 1 brand accent + semantic states:**

```css
/* Vercel pattern - Shadow-as-border neutrals */
--ink: #171717;           /* Primary text */
--body: #4d4d4d;          /* Secondary text */
--muted: #666666;         /* Tertiary */
--surface: #ffffff;       /* Cards */
--border-shadow: rgba(0,0,0,0.08) 0px 0px 0px 1px; /* Replaces border */

/* Linear pattern - Luminance stacking on dark */
--bg-marketing: #08090a;
--bg-panel: #0f1011;
--bg-surface: #191a1b;
--text-primary: #f7f8f8;
--text-secondary: #d0d6e0;
--text-muted: #8a8f98;
--border-subtle: rgba(255,255,255,0.05);
--border-standard: rgba(255,255,255,0.08);

/* Stripe pattern - Chromatic depth */
--heading: #061b31;       /* Deep navy, not black */
--label: #273951;
--body: #64748d;
--brand: #533afd;         /* Purple */
--shadow-brand: rgba(50,50,93,0.25);  /* Blue-tinted elevation */

/* Clerk pattern - Comprehensive semantic scale */
--purple-500: #6c47ff;    /* Brand anchor */
--gray-50: #f7f7f8;       /* Light root */
--gray-950: #131316;      /* Dark root */
--gray-200: #d9d9de;      /* Default border */
```

### 1.3 Dark Mode Implementation Patterns

| Approach | Used By | Implementation |
|----------|---------|----------------|
| **Class-based** (`.dark` ancestor) | Clerk, Arc, Vercel | Single CSS with `var(--light, ...) var(--dark, ...)` |
| **Media query only** | Linear (product), Raycast | `@media (prefers-color-scheme: dark)` |
| **Manual toggle + OS sync** | Stripe, Notion | Class + `prefers-color-scheme` listener |
| **No light mode** | Raycast (marketing) | Dark-only system by design |

**Key principle from Apple HIG:** Use semantic system colors (`label`, `secondaryLabel`, `systemBackground`, `secondarySystemBackground`, `tintColor`) that adapt automatically. Target 4.5:1 contrast minimum, 7:1 for small text.

---

## 2. Typography: Typefaces, Scales, Hierarchy

### 2.1 Font Families in Use (2024-2026)

| Site | Primary | Monospace | Notable Features |
|------|---------|-----------|------------------|
| **Linear** | Inter Variable | Berkeley Mono | `cv01`, `ss03` enabled globally; weight 510 signature |
| **Vercel** | Geist Sans | Geist Mono | `liga` globally; extreme negative tracking |
| **Raycast** | Inter | GeistMono | `calt`, `kern`, `liga`, `ss03`; **positive** tracking (+0.2px) |
| **Warp** | Matter (Regular) | Matter Mono / Geist Mono | Weight 400 dominance; uppercase labels with wide tracking |
| **Arc** | Inter (UI) / Argent CF (marketing) | Berkeley Mono | Serif for marketing voice; squircle radii |
| **Notion** | NotionInter (modified Inter) | - | `lnum`, `locl` on display; 4-weight system (400-700) |
| **Stripe** | sohne-var | SourceCodePro | `ss01` globally; **weight 300 for headlines** |
| **Clerk** | Suisse Intl + Geist (numbers) | Söhne Mono | Custom weight 450 (Book); tag-1 mono uppercase |

### 2.2 Universal Display Typography Patterns

**Negative letter-spacing at display sizes is non-negotiable:**

| Size | Linear | Vercel | Stripe | Notion | Clerk |
|------|--------|--------|--------|--------|-------|
| 72px | -1.584px | — | — | — | — |
| 64px | -1.408px | -2.4 to -2.88px | — | -2.125px | -0.035em |
| 56px | — | — | -1.4px | -1.875px | -0.035em |
| 48px | -1.056px | -2.4px | -0.96px | -1.5px | -0.035em |
| 40px | — | -1.28px | — | — | -0.015em |
| 32px | -0.704px | -1.28px | -0.64px | -0.625px | -0.015em |
| 24px | -0.288px | -0.96px | -0.22px | -0.25px | — |

**Tracking relaxation curve:** Aggressive negative at display → normal at body (16px) → positive for micro-labels (Clerk tag-1: +0.1em, Warp: +1.4-2.4px).

### 2.3 Weight Systems

| System | Weights Used | Philosophy |
|--------|--------------|------------|
| **Linear** | 300, 400, 510, 590 | 510 = signature "between" weight; no 700 |
| **Vercel** | 400, 500, 600 | Three weights only; 700 only for micro-badges |
| **Raycast** | 400, 500, 600 | 500 baseline for body (dark-mode legibility) |
| **Warp** | 400, 500 | **Regular (400) dominance**; Medium only for emphasis |
| **Stripe** | 300, 400 | **Weight 300 for headlines** — lightness as luxury |
| **Notion** | 400, 500, 600, 700 | Four-weight breadth for nuanced hierarchy |
| **Clerk** | 400, 450, 500, 600, 700 | Custom Book (450) weight for body |

### 2.4 OpenType Features as Brand Identity

| Feature | Linear | Vercel | Stripe | Raycast | Clerk | Purpose |
|---------|--------|--------|--------|---------|-------|---------|
| `cv01` | ✓ | | | | | Single-story 'a' |
| `ss01` | | | ✓ | | | Geometric alternates |
| `ss03` | ✓ | | | ✓ | | Clean geometric forms |
| `liga` | | ✓ | | ✓ | | Standard ligatures |
| `tnum` | | | ✓ | | | Tabular numerals |
| `calt` | | | | ✓ | | Contextual alternates |
| `kern` | | | | ✓ | | Kerning |

**Rule:** These are not optional — they transform generic fonts into brand typefaces.

---

## 3. Layout: Grid Systems, Spacing, Containers, Sections

### 3.1 Spacing Scales (All 8px Base)

| Site | Scale Values | Notable |
|------|--------------|---------|
| **Linear** | 1, 4, 7, 8, 11, 12, 16, 19, 20, 22, 24, 28, 32, 35 | 7px, 11px for optical micro-adjustments |
| **Vercel** | 1, 2, 3, 4, 5, 6, 8, 10, 12, 14, 16, 32, 36, 40 | Jumps 16→32 (no 20/24) |
| **Raycast** | 1, 2, 3, 4, 8, 10, 12, 16, 20, 24, 32, 40 | Standard 4px steps |
| **Warp** | 1, 4, 5, 8, 10, 12, 14, 15, 16, 18, 24, 26, 30, 32, 36 | Dense at small end |
| **Stripe** | 1, 2, 4, 6, 8, 10, 11, 12, 14, 16, 18, 20 | Every 2px from 4-12 (financial precision) |
| **Clerk** | 4px base (Tailwind scale) | space-1=4px, space-4=16px, space-8=32px |
| **Notion** | 2, 3, 4, 5, 6, 7, 8, 11, 12, 14, 16, 24, 32 | Fractional (5.6px, 6.4px) for micro-tuning |

### 3.2 Container Widths

| Site | Max Width | Breakpoints |
|------|-----------|-------------|
| **Linear** | ~1200px | Standard |
| **Vercel** | ~1200px | Standard |
| **Raycast** | ~1200px (1204px breakpoint) | |
| **Warp** | ~1500px | Cinematic wide |
| **Stripe** | ~1080px | Tighter for density |
| **Notion** | ~1200px | |
| **Clerk** | 40rem (640px) → 80rem (1280px) → 96rem (1536px) | Responsive max-widths per breakpoint |

### 3.3 Section Anatomy (Evil Martians Pattern)

Every marketing section follows:
1. **Eyebrow/Overline** — Tag style, uppercase, mono, brand color (e.g., "New Release", "SDKs")
2. **H2 Heading** — Sentence case, tight tracking, weight 600-700
3. **Body Copy** — Secondary color, max-width constrained (~2xl/48rem)
4. **Optional CTA** — Primary or ghost button

### 3.4 Hero Section Patterns (2025 Data from 100+ Sites)

| Pattern | Frequency | Examples |
|---------|-----------|----------|
| **Centered composition** | ~80% | Linear, Vercel, Raycast, Clerk |
| **Side-by-side (text left, visual right)** | ~15% | Classic SaaS |
| **Animated product UI** | High | Linear, Raycast |
| **Static product screenshot** | High | Most early-stage |
| **Switchable multi-UI** | Medium | Products with multiple modes |
| **Live product embed** | Low | Narrow-scope tools (Pixelcut) |
| **Code snippet** | Medium | SDKs, libraries (Tailwind) |
| **Abstract/no visual** | Low | Pre-launch, infra (Recraft) |

**Eyebrow/Banner usage:** ~60% use eyebrows (new release, funding) or full-width banners for momentum signaling.

---

## 4. Motion: Scroll Animations, Transitions, Micro-interactions

### 4.1 Duration Standards

| Context | Linear | Vercel | Raycast | Warp | Stripe | Clerk |
|---------|--------|--------|---------|------|--------|-------|
| Hover | — | — | **opacity 0.6** (not color) | Subtle | — | **100ms** (instant) |
| Button press | — | — | — | — | — | **150ms** (fast) |
| Standard UI | — | — | — | — | — | **200ms** |
| Moderate | — | — | — | — | — | **300ms** (shimmer) |
| Slow/Theme | — | — | — | — | — | **450ms** |
| Page expand | — | — | — | — | — | — |

### 4.2 Easing Functions

| Name | Value | Used By |
|------|-------|---------|
| **Standard Apple** | `cubic-bezier(0.33, 1, 0.68, 1)` | Clerk (ease-out-cubic), Apple HIG |
| **Spring (Apple)** | `spring(mass 1, stiffness 100, damping 15)` | Apple HIG |
| **Button shimmer** | `cubic-bezier(0.4, 0.36, 0, 1)` | Clerk |
| **Bounce reveal** | `cubic-bezier(0.175, 0.885, 0.32, 1.275)` | Clerk (hover arrows) |
| **Arc window** | `cubic-bezier(0.32, 0.72, 0, 1)` | Arc (Apple spring-style) |

### 4.3 Spring Physics Configs (Motion/React)

| Context | Stiffness | Damping | Mass |
|---------|-----------|---------|------|
| Button content swap | 200 | 20 | 0.8 |
| Drawer/modal | 300 | 30 | 0.5 |
| Nav indicator | 400 | 50 | — |

### 4.4 Scroll-Driven Animations (2024+ Standard)

**CSS-only, performant, respects `prefers-reduced-motion`:**

```css
/* View timeline - element entering viewport */
@supports (animation-timeline: view()) {
  @media (prefers-reduced-motion: no-preference) {
    .hero-title {
      animation: fade-in-up linear both;
      animation-timeline: view(block 20% 80%);
      animation-range: entry 0% cover 50%;
    }
  }
}

/* Scroll progress timeline - page scroll position */
@supports (animation-timeline: scroll()) {
  @media (prefers-reduced-motion: no-preference) {
    .nav-bar {
      animation: nav-solidify linear both;
      animation-timeline: scroll(root);
      animation-range: 0 200px;
    }
  }
}

/* Reduced motion fallback */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    animation-timeline: auto !important; /* Detach from scroll/view */
  }
}
```

### 4.5 Motion Principles (Apple HIG + Top Sites)

- **Guide attention, don't decorate** — Motion communicates state, hierarchy, causality
- **100-450ms range** — Instant (100ms) for hover, Standard (200ms) for UI, Slow (450ms) for theme/context switches
- **Spring for content swaps** — Drawers, tabs, command palettes use spring physics
- **Opacity transitions for dark-mode buttons** — Raycast, Linear use opacity 0.6 hover, not color shifts
- **Respect reduced-motion at timeline level** — `animation-timeline: auto` detaches scroll-driven animations

---

## 5. Visual Effects: Glassmorphism, Gradients, Shadows, Borders, Noise

### 5.1 Glassmorphism / Frosted Glass (Arc, Clerk, Modern)

```css
/* Arc-style frosted card */
.glass-card {
  background: rgba(255, 255, 255, 0.7);
  backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.4);
  border-radius: 16px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
}

/* Clerk dark-mode glass */
.glass-dark {
  background: rgba(20, 20, 25, 0.6);
  backdrop-filter: blur(20px);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

/* Fallback for no backdrop-filter */
@supports not (backdrop-filter: blur(1px)) {
  .glass-card { background: rgba(30, 30, 50, 0.85); }
}
```

**Noise/texture overlay to prevent banding:**
```css
.glass-card::after {
  content: "";
  position: absolute; inset: 0; border-radius: inherit;
  background-image: url("data:image/svg+xml,<svg>...feTurbulence...</svg>");
  opacity: 0.02; pointer-events: none;
}
```

### 5.2 Shadow Systems (Multi-Layer = Depth)

**Vercel — Shadow-as-border (light mode):**
```css
/* Ring border replaces CSS border */
.box-border { box-shadow: 0 0 0 1px rgba(0,0,0,0.08); }

/* Full card stack */
.card {
  box-shadow:
    0 0 0 1px rgba(0,0,0,0.08),      /* Border */
    0 2px 2px rgba(0,0,0,0.04),       /* Ambient */
    0 8px 8px -8px rgba(0,0,0,0.04),  /* Depth */
    0 0 0 1px #fafafa;                /* Inner glow */
}
```

**Stripe — Chromatic (blue-tinted) shadows:**
```css
.card {
  box-shadow:
    rgba(50,50,93,0.25) 0 30px 45px -30px,  /* Brand shadow (far) */
    rgba(0,0,0,0.1) 0 18px 36px -18px;       /* Neutral shadow (near) */
}
```

**Clerk — 4-layer bento card:**
```css
.bento-card {
  box-shadow:
    0 1px 1px rgba(0,0,0,0.05),
    0 4px 6px rgba(34,42,53,0.04),
    0 24px 68px rgba(47,48,55,0.05),
    0 2px 3px rgba(0,0,0,0.04);
}
```

**Linear — Dark mode luminance borders (no shadows for elevation):**
```css
.card {
  background: rgba(255,255,255,0.02);
  border: 1px solid rgba(255,255,255,0.08);
  /* Elevation = background opacity step: 0.02 → 0.04 → 0.05 */
}
```

**Raycast — macOS-native double-ring + inset:**
```css
.button {
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.1),   /* Top highlight */
    0 0 0 1px rgba(255,255,255,0.25),      /* Outer ring */
    inset 0 -1px 0 rgba(0,0,0,0.2);        /* Bottom shadow */
}
```

### 5.3 Gradients: Brand, Not Decoration

| Site | Gradient Use | Colors |
|------|--------------|--------|
| **Stripe** | Hero decorative, dark sections | Ruby `#ea2261` → Magenta `#f96bee` |
| **Clerk** | Glow halos under demos, timeline | Purple `#6c47ff` → Sky `#3ad4fd` |
| **Arc** | **Entire browser chrome mood** | User-selectable: Peach-coral, Violet-fuchsia, Mint-cyan |
| **Vercel** | Hero atmospheric wash | Pastel multi-color (barely visible) |
| **Linear** | None (achromatic system) | N/A |
| **Raycast** | Hero diagonal stripes | Raycast Red `#FF6363` pattern |
| **Warp** | None (photography instead) | N/A |

**Rule:** Gradients serve brand mood or data visualization — never as button fills or card backgrounds on top-tier sites.

### 5.4 Border Philosophy

| Approach | Sites | Implementation |
|----------|-------|----------------|
| **Shadow-as-border** | Vercel, Stripe, Clerk | `box-shadow: 0 0 0 1px rgba(...)` |
| **Semi-transparent white** | Linear, Raycast, Arc | `rgba(255,255,255, 0.05-0.4)` |
| **Whisper-thin dark** | Notion | `1px solid rgba(0,0,0,0.1)` |
| **Conservative solid** | Stripe (light) | `#e5edf5` (blue-tinted gray) |

---

## 6. Hero Patterns: Interactive Demos, Live Code, Animations

### 6.1 Hero Visual Hierarchy (Evil Martians 2025)

```
[Eyebrow]          ← Momentum signal (optional)
[H1 Headline]      ← 48-72px, tight tracking, sentence case
[Subtitle]         ← 18-20px, relaxed line-height, secondary color
[CTA Primary]      ← Brand color, specific verb ("Start building", "Deploy now")
[CTA Secondary]    ← Ghost/outlined, "View docs" or "GitHub"
[Visual Element]   ← Centered below copy
```

### 6.2 Visual Element Types (Ranked by Effectiveness)

| Type | Effort | Conversion | Best For |
|------|--------|------------|----------|
| **Animated product UI** | High | High | UI-heavy tools (Linear, Raycast) |
| **Live code editor** | High | Highest | SDKs, APIs (Clerk, craftzdog tutorial) |
| **Static screenshot** | Low | Medium | Early-stage, resource-constrained |
| **Switchable tabs** | Medium | Medium | Multi-mode products |
| **Code snippet** | Low | Medium | Libraries, infra (Tailwind) |
| **Abstract/WebGL** | High | Variable | Brand differentiation (Arc, Substrate) |
| **Video background** | Medium | Low | Avoid — performance, accessibility issues |

### 6.3 Interactive Hero Implementation Patterns

**Self-typing code editor (craftzdog pattern):**
```javascript
// Types automatically, syncs with video, hands over on user input
// Real editor component, not simulation
```

**Live SDK playground (Clerk pattern):**
```jsx
// Embedded <SignIn /> with appearance prop, real auth flow
// Shows actual product in hero
```

**WebGL/Three.js hero (Arc, Substrate):**
```javascript
// Code-rendered 3D, no model files, no textures
// Responds to mouse (drag-rotate, hover-glow)
// Falls back to static image if WebGL unavailable
```

**Scroll-driven hero reveal:**
```css
@media (prefers-reduced-motion: no-preference) {
  .hero-title {
    animation: fade-in-up linear both;
    animation-timeline: view(block 10% 50%);
  }
  .hero-visual {
    animation: scale-in linear both;
    animation-timeline: view(block 20% 80%);
  }
}
```

---

## 7. Component Patterns: Cards, Buttons, Code Blocks, Navigation, Footers

### 7.1 Button Systems (All Sites)

| Variant | Linear | Vercel | Raycast | Warp | Stripe | Clerk | Notion |
|---------|--------|--------|---------|------|--------|-------|--------|
| **Primary** | Brand indigo `#5e6ad2` | Dark `#171717` | Translucent white pill | Dark pill `#353534` | Purple `#533afd` | Purple `#6c47ff` | Blue `#0075de` |
| **Ghost** | `rgba(255,255,255,0.02)` + border | White + shadow-border | Transparent + inset shadow | — | Transparent + border | Text-only purple | Transparent + underline |
| **Secondary** | Subtle `rgba(255,255,255,0.04)` | — | Bordered `rgba(255,255,255,0.1)` | Frosted tag | Outlined purple | Navigation pill | Gray `rgba(0,0,0,0.05)` |
| **Destructive** | — | — | — | — | — | — | — |
| **Radius** | 6px | 6px | 6px / 86px (pill) | 50px (pill) | 4px | 6px (md) | 4px |
| **Shadow** | Multi-layer dark | Ring + ambient | macOS inset + ring | Minimal | Chromatic 2-layer | 4-layer with highlight | 4-layer card |

**Universal CTA copy pattern:** Specific verbs ("Start building", "Deploy now", "Start deploying") — never "Get started" or "Sign up".

### 7.2 Card Patterns

| Site | Background | Border | Radius | Shadow | Hover |
|------|------------|--------|--------|--------|-------|
| **Linear** | `rgba(255,255,255,0.02)` | `rgba(255,255,255,0.08)` | 8-22px | Inset + layered | Bg opacity ↑ |
| **Vercel** | `#ffffff` | Shadow-border | 8-12px | 4-layer stack | Shadow intensifies |
| **Raycast** | `#101111` | `rgba(255,255,255,0.06)` | 12-20px | Double-ring + warm glow | Border opacity ↑ |
| **Warp** | Dark + photography | `rgba(226,226,226,0.35)` | 8-14px | Minimal (borders only) | Minimal |
| **Stripe** | `#ffffff` | `#e5edf5` | 4-8px | Blue-tinted 2-layer | Shadow ↑ |
| **Clerk** | White / `#131316` | Inset white (dark) | 16px (2xl) | 4-layer bento | Shadow ↑ |
| **Notion** | `#ffffff` | `rgba(0,0,0,0.1)` | 12-16px | 4-layer sub-0.05 | Shadow ↑ |
| **Arc** | `rgba(255,255,255,0.7)` + blur | `rgba(255,255,255,0.4)` | 16px | `0 8px 32px rgba(0,0,0,0.08)` | Glass ↑ |

### 7.3 Code Blocks

| Site | Font | Size | Line Height | Theme |
|------|------|------|-------------|-------|
| **Linear** | Berkeley Mono | 14px | 1.5 | Dark, low contrast |
| **Vercel** | Geist Mono | 16px | 1.5 | Light, high contrast |
| **Raycast** | GeistMono | 14px | 1.6 | Dark, syntax highlighted |
| **Stripe** | SourceCodePro | 12px | **2.0** (generous) | Light, `tnum` for numbers |
| **Clerk** | Söhne Mono | 14px (mono-1) | — | Syntax: purple funcs, sky strings |
| **Arc** | Berkeley Mono | 13px | 1.5 | Dark, command bar style |

### 7.4 Navigation Patterns

| Pattern | Sites | Details |
|---------|-------|---------|
| **Sticky transparent → solid** | Vercel, Clerk, Stripe | `backdrop-filter: blur(12-40px)` on scroll |
| **Dark sticky (always)** | Linear, Raycast | Blends with dark page |
| **Not sticky** | Notion, Warp | Clean, scrolls away |
| **Sidebar-first** | Arc (product) | 240px → 56px collapsible |
| **Mobile** | All | Hamburger → slide-out/drawer |

**Nav link typography:** 13-16px, weight 500-600, tight tracking, uppercase only for Clerk tag-1.

### 7.5 Footer Patterns

| Pattern | Sites | Details |
|---------|-------|---------|
| **Dark footer on light page** | Most (Vercel, Stripe, Clerk) | Visual closure, 4-5 column grid |
| **Light footer on dark page** | Linear, Raycast | Consistent with page theme |
| **Minimal** | Warp, Arc | Just links, copyright |
| **Changelog/blog preview** | Mature (Stripe, Clerk) | Signal: "we're shipping" |

---

## 8. Performance: Critical CSS, Fonts, Images, No-JS Fallbacks

### 8.1 Critical CSS Inlining

```html
<head>
  <!-- Inline critical CSS for above-fold -->
  <style>
    /* Hero, nav, typography base, button primary */
    :root { --color-bg: #fff; --color-text: #171717; ... }
    .hero { padding: 80px 24px; text-align: center; }
    .btn-primary { background: #171717; color: #fff; ... }
    /* ~10-14KB target */
  </style>
  
  <!-- Non-blocking rest -->
  <link rel="preload" as="style" href="/styles.css" onload="this.rel='stylesheet'">
  <noscript><link rel="stylesheet" href="/styles.css"></noscript>
</head>
```

### 8.2 Font Loading Strategy (Best Practice 2024)

```html
<!-- Preload critical font (WOFF2 only) -->
<link rel="preload" as="font" type="font/woff2" crossorigin href="/fonts/geist-var.woff2">

<!-- Font-face with font-display: swap -->
@font-face {
  font-family: 'Geist';
  src: url('/fonts/geist-var.woff2') format('woff2');
  font-display: swap;           /* Text visible immediately */
  font-weight: 100 900;         /* Variable font range */
  font-stretch: 25% 151%;
}

/* Fallback metrics to prevent layout shift */
@font-face {
  font-family: 'Geist Fallback';
  src: local('Arial');
  ascent-override: 90%;
  descent-override: 25%;
  line-gap-override: 0%;
  size-adjust: 105%;
}
```

**Key rules:**
- WOFF2 only (modern browser support is universal)
- `font-display: swap` mandatory
- Preload hero font
- Variable fonts preferred (single file, full weight range)
- Define fallback metrics to eliminate CLS

### 8.3 Image Optimization

```html
<!-- Hero image: priority load, proper sizing -->
<img 
  src="/hero.avif" 
  alt="Product dashboard"
  width="1200" height="800"
  fetchpriority="high"
  loading="eager"
  decoding="async"
>

<!-- Responsive images for feature screenshots -->
<picture>
  <source type="image/avif" srcset="/shot-800.avif 800w, /shot-1200.avif 1200w">
  <source type="image/webp" srcset="/shot-800.webp 800w, /shot-1200.webp 1200w">
  <img src="/shot-1200.png" width="1200" height="800" alt="Feature" loading="lazy" decoding="async">
</picture>
```

### 8.4 No-JS / Progressive Enhancement

```css
/* Base styles work without JS */
.hero { opacity: 1; } /* Visible by default */

/* Enhanced with JS */
.js .hero { opacity: 0; }
.js .hero.is-loaded { opacity: 1; transition: opacity 300ms; }

/* Scroll animations only with support */
@supports (animation-timeline: view()) {
  @media (prefers-reduced-motion: no-preference) {
    .reveal { animation: fade-in linear both; animation-timeline: view(); }
  }
}

/* Focus visible polyfill not needed - :focus-visible is baseline */
:focus-visible { outline: 2px solid var(--focus-color); outline-offset: 2px; }
```

### 8.5 Core Web Vitals Targets (Apple-Level)

| Metric | Target | Technique |
|--------|--------|-----------|
| **LCP** | < 2.5s | Preload hero font + image, inline critical CSS, priority hints |
| **CLS** | < 0.1 | Font fallback metrics, explicit width/height on images, reserve space for dynamic content |
| **INP** | < 200ms | Minimize main-thread work, defer non-critical JS, use CSS for interactions |
| **TTFB** | < 800ms | Edge caching, streaming HTML, minimal server processing |

---

## 9. Synthesis: Reusable Design Token System

### 9.1 Unified Token Structure (CSS Custom Properties)

```css
:root {
  /* === SPACING (4px base) === */
  --space-1: 4px; --space-2: 8px; --space-3: 12px; --space-4: 16px;
  --space-5: 20px; --space-6: 24px; --space-8: 32px; --space-10: 40px;
  --space-12: 48px; --space-16: 64px; --space-20: 80px; --space-24: 96px;

  /* === RADIUS === */
  --radius-sm: 4px;    /* Buttons, inputs, badges */
  --radius-md: 6px;    /* Standard components */
  --radius-lg: 8px;    /* Cards */
  --radius-xl: 12px;   /* Featured cards */
  --radius-2xl: 16px;  /* Hero cards, modals */
  --radius-full: 9999px; /* Pills, badges */

  /* === TYPOGRAPHY === */
  --font-sans: 'Inter Variable', system-ui, sans-serif;
  --font-mono: 'Berkeley Mono', ui-monospace, monospace;
  --font-display: 'Inter Variable', system-ui, sans-serif; /* Or custom */
  
  /* OpenType features */
  --font-features-base: 'cv01' 1, 'ss03' 1, 'liga' 1, 'kern' 1;
  --font-features-display: 'cv01' 1, 'ss03' 1, 'liga' 0, 'ss02' 1, 'ss08' 1;
  
  /* Scale (fluid, clamp-based) */
  --text-display: clamp(48px, 5vw, 72px);
  --text-h1: clamp(32px, 4vw, 48px);
  --text-h2: clamp(24px, 3vw, 32px);
  --text-h3: 20px;
  --text-body-lg: 18px;
  --text-body: 16px;
  --text-sm: 14px;
  --text-xs: 12px;
  --text-micro: 10px;

  /* Tracking scale */
  --tracking-display: -0.022em;  /* ~-1.58px at 72px */
  --tracking-h1: -0.015em;
  --tracking-h2: -0.009em;
  --tracking-h3: -0.012em;
  --tracking-body: 0;
  --tracking-tag: 0.1em;  /* Uppercase labels */

  /* === MOTION === */
  --duration-instant: 100ms;
  --duration-fast: 150ms;
  --duration-standard: 200ms;
  --duration-moderate: 300ms;
  --duration-slow: 450ms;
  --ease-standard: cubic-bezier(0.33, 1, 0.68, 1);
  --ease-spring: cubic-bezier(0.175, 0.885, 0.32, 1.275);
  --ease-out-cubic: cubic-bezier(0.33, 1, 0.68, 1);

  /* === SHADOWS (Light mode default) === */
  --shadow-ring: 0 0 0 1px rgba(0,0,0,0.08);
  --shadow-card: var(--shadow-ring), 0 2px 2px rgba(0,0,0,0.04), 0 8px 8px -8px rgba(0,0,0,0.04), 0 0 0 1px #fafafa;
  --shadow-elevated: rgba(50,50,93,0.25) 0 30px 45px -30px, rgba(0,0,0,0.1) 0 18px 36px -18px;
  --shadow-focus: 0 0 0 2px var(--focus-color);

  /* === Z-INDEX === */
  --z-dropdown: 100; --z-sticky: 200; --z-modal: 300; --z-toast: 400; --z-tooltip: 500;
}

/* === DARK MODE OVERRIDES === */
@media (prefers-color-scheme: dark) {
  :root {
    /* Linear-style dark tokens */
    --color-bg: #08090a;
    --color-bg-panel: #0f1011;
    --color-bg-surface: #191a1b;
    --color-text: #f7f8f8;
    --color-text-secondary: #d0d6e0;
    --color-text-muted: #8a8f98;
    --color-border: rgba(255,255,255,0.08);
    --color-border-subtle: rgba(255,255,255,0.05);
    --color-accent: #7170ff;  /* Linear indigo */
    --color-accent-hover: #828fff;
    
    /* Shadow overrides for dark */
    --shadow-ring: 0 0 0 1px rgba(255,255,255,0.08);
    --shadow-card: 0 0 0 1px rgba(255,255,255,0.08), inset 0 0 0 1px rgba(255,255,255,0.05);
    --shadow-focus: 0 0 0 2px var(--focus-color);
  }
}

/* === CLASS-BASED DARK MODE (Clerk pattern) === */
.dark {
  --color-bg: #131316;
  --color-bg-panel: #1a1a1f;
  --color-bg-surface: #212126;
  --color-text: #ffffff;
  --color-text-secondary: #9394a1;
  --color-text-muted: #747686;
  --color-border: #2f3037;
  --color-accent: #6c47ff;  /* Clerk purple */
  --color-accent-hover: #6430f7;
}
```

### 9.2 Component Recipes Using Tokens

```css
/* Primary Button */
.btn-primary {
  background: var(--color-accent);
  color: var(--color-text-on-accent, #fff);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  font: 500 var(--text-body) var(--font-sans);
  font-feature-settings: var(--font-features-base);
  border: none;
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-standard),
              box-shadow var(--duration-fast) var(--ease-standard);
}
.btn-primary:hover { background: var(--color-accent-hover); }
.btn-primary:focus-visible { box-shadow: var(--shadow-focus); }

/* Ghost Button */
.btn-ghost {
  background: transparent;
  color: var(--color-text);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  font: 500 var(--text-body) var(--font-sans);
  font-feature-settings: var(--font-features-base);
}
.btn-ghost:hover { background: var(--color-bg-surface); }

/* Card */
.card {
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  padding: var(--space-6);
  transition: box-shadow var(--duration-standard) var(--ease-standard);
}
.card:hover { box-shadow: var(--shadow-elevated); }

/* Hero Headline */
.hero-title {
  font: 510 var(--text-display) var(--font-display);
  font-feature-settings: var(--font-features-display);
  letter-spacing: var(--tracking-display);
  line-height: 1.0;
  color: var(--color-text);
  margin-bottom: var(--space-4);
}

/* Eyebrow/Tag */
.eyebrow {
  font: 500 var(--text-xs) var(--font-mono);
  font-feature-settings: 'tnum' 1;
  letter-spacing: var(--tracking-tag);
  text-transform: uppercase;
  color: var(--color-accent);
  margin-bottom: var(--space-2);
}

/* Code Block */
.code-block {
  font: 400 var(--text-sm) var(--font-mono);
  line-height: 1.6;
  background: var(--color-bg-panel);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  overflow-x: auto;
}
```

---

## 10. Implementation Checklist (Apple-Level Quality Bar)

### Design Tokens
- [ ] Single source of truth for colors, spacing, typography, radius, motion
- [ ] Semantic naming (`--color-text`, not `--color-white`)
- [ ] Dark mode via CSS custom properties (class or media query)
- [ ] OpenType features defined as tokens
- [ ] Fluid type scale with `clamp()`

### Typography
- [ ] Variable font (WOFF2) with preload
- [ ] `font-display: swap` + fallback metrics
- [ ] OpenType features enabled globally (`cv01`, `ss03`, `liga`)
- [ ] Negative tracking at display sizes
- [ ] Consistent weight system (3-4 weights max)
- [ ] Line-height tightens at scale (1.0→1.5)

### Color
- [ ] One brand accent, used only for CTAs/links/active states
- [ ] 4-6 step neutral scale with warm/cool intent
- [ ] Semantic aliases for all roles
- [ ] 4.5:1 contrast minimum (7:1 for small text)
- [ ] No decorative gradients

### Layout
- [ ] 8px base spacing scale
- [ ] Max-width container ~1200px (or responsive per breakpoint)
- [ ] Section vertical rhythm: 64-120px between major sections
- [ ] Centered hero, 2-3 column feature grids
- [ ] Eyebrow → H2 → Body → CTA section anatomy

### Components
- [ ] Button variants: Primary, Ghost, Secondary (max 3)
- [ ] Card: surface, border, radius, shadow stack, hover
- [ ] Code block: mono font, generous line-height, scrollable
- [ ] Nav: sticky with backdrop-blur, mobile drawer
- [ ] Focus styles: visible, 2px offset, brand color

### Motion
- [ ] Durations: 100/150/200/300/450ms
- [ ] Easing: `cubic-bezier(0.33, 1, 0.68, 1)` standard
- [ ] Springs for content swaps (stiffness 200-400, damping 20-50)
- [ ] `@media (prefers-reduced-motion: reduce)` disables all non-essential
- [ ] Scroll-driven animations gated by `@supports` + reduced-motion

### Performance
- [ ] Critical CSS inlined (<14KB)
- [ ] Non-critical CSS preload + onload swap
- [ ] Hero font + image preloaded with `fetchpriority="high"`
- [ ] All images: AVIF/WebP, explicit dimensions, lazy-load below fold
- [ ] No layout shifts (font fallbacks, image dimensions, reserved space)
- [ ] LCP < 2.5s, CLS < 0.1, INP < 200ms

### Accessibility
- [ ] Semantic HTML structure
- [ ] Focus visible on all interactive elements
- [ ] Color not sole conveyor of meaning
- [ ] Reduced motion respected
- [ ] Alt text for all images
- [ ] ARIA labels for icon-only buttons

---

## 11. References & Sources

### Design System Extractions (DESIGN.md files)
- Linear: `github.com/nexu-io/open-design/design-systems/linear-app/DESIGN.md`
- Vercel: `github.com/nexu-io/open-design/design-systems/vercel/DESIGN.md`
- Raycast: `github.com/nexu-io/open-design/design-systems/raycast/DESIGN.md`
- Warp: `github.com/nexu-io/open-design/design-systems/warp/DESIGN.md`
- Arc: `github.com/nexu-io/open-design/design-systems/arc/DESIGN.md`
- Notion: `github.com/nexu-io/open-design/design-systems/notion/DESIGN.md`
- Stripe: `github.com/nexu-io/open-design/design-systems/stripe/DESIGN.md`
- Clerk: `clerk.com/design.md`

### Research & Analysis
- Evil Martians: "We studied 100 dev tool landing pages—here's what really works in 2025" (evilmartians.com/chronicles)
- Evil Martians: "Devs in mind: how to design interfaces for developer tools"
- OpenDesign benchmarks: `designmd.co`, `opendesign.ai`, `shadcn.io/design`

### Apple Human Interface Guidelines
- Color: `developer.apple.com/design/human-interface-guidelines/color`
- Typography: `developer.apple.com/design/human-interface-guidelines/typography`
- Dark Mode: `developer.apple.com/design/human-interface-guidelines/dark-mode`
- Motion: `developer.apple.com/design/human-interface-guidelines/motion`

### Modern CSS & Performance
- CSS Scroll-Driven Animations: `developer.chrome.com/blog/scroll-triggered-animations`, `css-scroll-driven.com`
- `prefers-reduced-motion`: `css-scroll-driven.com/accessibility`, `rohullah.me/blog/prefers-reduced-motion`
- Critical CSS: `web.dev/articles/extract-critical-css`, `web.dev/articles/font-best-practices`
- Font loading: `web.dev/articles/font-best-practices`, `frontendhorizon.com/blog/critical-css-and-font-loading`
- Glassmorphism: `web-animation-skills/skills/glassmorphism`, `csstools.io/blog/glassmorphism-guide`

### Hero & Component Patterns
- craftzdog/interactive-demo-tutorial (self-typing editor hero)
- rasengan-dev/sharingan (animated hero sections)
- 21st.dev community components (dev tool landing blocks)
- launchkit.evilmartians.io (open source dev tool template)

---

*Last updated: November 2026 | Compiled for Naadi / Paperscope research*