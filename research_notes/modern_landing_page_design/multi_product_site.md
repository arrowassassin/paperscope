# Multi-Product Developer Tool Site Research

**Research Date:** November 2026  
**Focus:** How developer tool startups (2-4 products) structure websites to showcase multiple products under one brand  
**Sources Analyzed:** Vercel, Supabase, PlanetScale, Railway, Render, Linear, Sindre Sorhus, ixtj.dev, makerhandle

---

## Executive Summary

The most successful multi-product developer tool sites share a **unified brand with clear product differentiation**. They use a **mega-menu navigation** as the primary product switcher, **dedicated product pages** for deep dives, and a **shared design system** with **product-specific visual identifiers** (icons, accent colors, illustrations). The pattern scales from 3 products (PlanetScale) to 15+ (Vercel, Railway, Render) while maintaining coherence.

---

## Navigation Patterns

### 1. Mega Menu with Categorized Products (Dominant Pattern)

**Vercel** — Three-column mega menu under "Products":
- **Agent Stack** (6 products): AI SDK, AI Gateway, Sandbox, Passport, Connect, eve
- **Core Platform** (7 products): Build & Deploy, Vercel CI, Fluid Compute, Content Delivery, Observability, Workflows, Security
- **Tools** (7 products): Next.js, Vercel Agent, Vercel Plugin, Open Source, Domains, v0

**Supabase** — Product dropdown with icon cards:
- **Core Products** (5): Database, Auth, Storage, Edge Functions, Compute, Realtime
- **Modules** (1+): Vector
- Each item shows icon + name + one-line description

**Railway & Render** — Nearly identical 4-column mega menus under "Product":
- **Agent execution**: Sandboxes, Workflows
- **Core compute**: Web services, Private services, Background workers, Cron jobs, Static sites
- **Managed data**: Shared filesystems, Postgres, Key Value, Persistent disks
- **Ship faster / Scale & secure**: Feature groupings (Native runtimes, PR previews, Private networking, Autoscaling, DDoS, HIPAA)

**PlanetScale** — Simple "Platform" dropdown with 3 products:
- Vitess (sharded MySQL)
- Postgres
- Neki (sharded Postgres)

### 2. Top-Level Product Links (Simpler Sites)

**Linear** (single product, extensive features): Features | Pricing | Changelog | Blog — no dropdown needed

**Sindre Sorhus** (personal): Apps | Blog | Now | About — "Apps" page lists all 64 products

**ixtj.dev** (indie portfolio): Home | Projects | Blog | About — "Projects" page lists all products

### 3. Mobile Navigation

All major sites collapse mega menus into **full-screen drawers** or **tabbed interfaces**:

- **Supabase mobile**: Horizontal tab bar (Product | Developers | Solutions) + vertical product cards with icons
- **Vercel mobile**: Full-screen drawer with same 3-column layout
- **Railway/Render mobile**: Accordion-style expandable sections
- **PlanetScale mobile**: Native `<details>/<summary>` dropdown

---

## Visual Differentiation Strategies

### Shared Design System + Product Identifiers

| Site | Shared System | Product Differentiation |
|------|---------------|------------------------|
| **Vercel** | Geist font, consistent spacing, dark/light themes | Product-specific landing pages; Next.js gets distinct black triangle logo; v0 has unique branding |
| **Supabase** | Green brand color, consistent card layouts | **Each product has unique icon + accent color**: Database (green cylinder), Auth (purple shield), Storage (blue bucket), Edge Functions (orange edge), Compute (purple server), Realtime (pink wave), Vector (teal sparkle) |
| **PlanetScale** | Orange brand, technical typography | Product pages use distinct hero illustrations; Vitess/Postgres/Neki each get dedicated visual treatment |
| **Railway** | Dark purple theme, canvas metaphor | Services share canvas UI; product icons in nav use consistent line style |
| **Render** | Blue/indigo, clean cards | Similar to Railway; product icons in mega menu use consistent 24px line icons |

### Icon Systems

- **Supabase**: Custom SVG icons per product (16-24px), used consistently in nav, docs, dashboard
- **Vercel**: Product logos where established (Next.js triangle, v0), otherwise text-only
- **Railway/Render**: Feather/Lucide-style line icons (24px) for uniformity
- **PlanetScale**: Illustrative hero graphics per product, not small icons

### Color Coding

- **Supabase**: Most sophisticated — each product has a distinct accent hue used in badges, CTAs, doc sidebars
- **Vercel**: Monochrome brand; product identity via typography/logos
- **Railway/Render**: Single brand accent (purple/blue); differentiation via layout position and icons
- **Linear**: Single brand; feature pages use consistent styling

---

## Landing Page Structures

### 1. Single Marketing Page + Product Cards → Dedicated Pages

**PlanetScale** (cleanest example):
- Hero: "World's fastest cloud databases"
- Value props: Vitess, Postgres, Neki as three pillars
- Logo wall of tier-0 customers
- Each product links to `/vitess`, `/postgres`, `/neki` with full landing pages

**Supabase**:
- Hero: "The Postgres Development Platform"
- Product grid (6 cards) with icons + descriptions → each links to dedicated page
- "Build with Supabase" section showing integrations
- Customer logos

### 2. Mega Menu as Primary Discovery (Vercel, Railway, Render)

- Homepage focuses on **brand narrative** ("Agentic Infrastructure", "All-in-one intelligent cloud")
- Products discovered primarily through **mega menu exploration**
- Each product has **full landing page** (`/ai-sdk`, `/sandboxes`, `/web-services`)
- Cross-linking in footers and "Related products" sections

### 3. Personal/Indie Portfolio: Terminal or List View

**ixtj.dev**: Terminal-themed `ls ~/projects` showing:
```
miniEcho        Real-time subtitling for livestreams
BoxxCat         Community engagement assistant
→ 3 more: /projects/
```

**Sindre Sorhus /apps**: Categorized grid (Mac, iOS, Menu Bar, Vision, Free, Paid) with 64 apps

**makerhandle**: Standardized project cards with:
- Logo (auto-fetched favicon)
- Name + one-line description
- Status badge (Active, Building, Acquired, On hold, Discontinued)
- Tags (SaaS, Chrome extension, open source)
- Optional verified revenue
- Featured project shown larger at top

---

## Cross-Product CTAs & Integration Patterns

### 1. "Try X with Y" Bundling

| Site | Pattern |
|------|---------|
| **Vercel** | "Deploy Next.js on Vercel" — framework + platform synergy; v0 → Vercel deploy flow |
| **Supabase** | "Database + Auth + Storage" — all-in-one Postgres platform; Edge Functions + Database |
| **Railway** | Canvas shows services connected visually; "Add database" → "Add service" flow |
| **Render** | "Web service + Postgres + Redis" template; PR previews across services |
| **PlanetScale** | "Vitess + Insights" — database + observability |

### 2. Shared Account & Dashboard

- **All major platforms**: Single login → unified dashboard showing all products
- **Vercel**: Projects can use Next.js, AI SDK, Edge Functions, Domains simultaneously
- **Supabase**: One project = Database + Auth + Storage + Functions + Realtime
- **Railway/Render**: One project = multiple services (web, worker, cron, DB) on one canvas

### 3. Unified Pricing

- **Vercel**: Tiered (Hobby, Pro, Enterprise) — includes all products with usage limits
- **Supabase**: Free tier includes all products; Pro/Enterprise add usage + features
- **Railway/Render**: Pay-as-you-go per resource; no per-product pricing
- **PlanetScale**: Per-database pricing; Vitess/Postgres/Neki on same plans

### 4. Cross-Product Documentation

- **Supabase Docs**: Top nav "Products" dropdown mirrors marketing; each product has guides + reference
- **Vercel Docs**: Framework-specific (Next.js) + product-specific (AI SDK, Edge Functions)
- **Railway/Render Docs**: Service-type organization (Web Services, Databases, Background Workers)

---

## Footer Organization

### Enterprise/Platform Sites (Vercel, Supabase, Railway, Render)

**Multi-column grid by audience:**

```
PRODUCT          DEVELOPERS        COMPANY          RESOURCES
├── Product A    ├── Docs          ├── About        ├── Blog
├── Product B    ├── API Reference ├── Careers      ├── Changelog
├── Product C    ├── SDKs          ├── Press        ├── Status
├── Pricing      ├── CLI           ├── Security     ├── Community
└── Changelog    ├── Integrations  ├── Legal        └── Support
```

- **Vercel**: Adds "Frameworks", "Marketplace", "Partners"
- **Supabase**: Adds "GitHub", "Discord", "Twitter" social links
- **Railway/Render**: Adds "Migration guides" (Heroku, Railway ↔ Render)

### Personal/Indie Sites

**Sindre Sorhus**: Apps | Help | About | Follow (social links) | Quote

**ixtj.dev**: Minimal colophon (© 2026 ixtj.dev) + floating command palette

**makerhandle**: Product | Resources | Built for | Compare | Company + CTA banner

---

## Indie Hacker / Personal Portfolio Patterns (2-4 Products)

### makerhandle's Recommended Structure (Purpose-Built for This)

**Project Entry Schema:**
```yaml
name: "miniEcho"
logo: (auto-fetched favicon)
description: "Real-time subtitling for livestreams"
status: "Active"  # Active | Building | Acquired | On hold | Discontinued
url: "https://miniecho.com"
tags: ["SaaS", "Real-time", "AI"]
revenue: "$2.3k MRR"  # verified from Stripe
```

**Ordering Strategy (Status-First):**
1. **Featured project** — largest card, top position
2. **Active projects** — highest traction first
3. **Building projects** — shows momentum
4. **Acquired projects** — strong signal
5. **On hold / Discontinued** — at bottom, kept for record

**Key Principles from makerhandle article:**
- One-line descriptions: lead with user, not tech ("Booking page for dog groomers" > "Next.js scheduling app")
- Include failures — "5 discontinued + 1 success tells better story than 1 success alone"
- Automate updates: revenue from Stripe/Paddle, favicons auto-fetched
- Monthly 5-minute review habit

### ixtj.dev (Terminal Aesthetic)

- `ls ~/projects` metaphor
- Each project: name + muted description + link
- "3 more: /projects/" progressive disclosure
- Blog + contact in same terminal session

### Sindre Sorhus (Scale: 64 Apps)

- Categorized `/apps` page (Mac, iOS, Menu Bar, Vision, Free, Paid, Timeline)
- `/open-source` for npm packages
- Minimal home page → two CTAs: "Apps" and "Code"
- Personal brand (unicorn) throughout

---

## Technical Implementation Patterns

### 1. Multi-Zone / Microfrontend Architecture

**Vercel**: Next.js Multi-Zones — independent apps for marketing, dashboard, docs, each product
- Shared design system package (`@vercel/design-system`)
- Independent deployments, shared authentication

**Supabase**: Single Next.js app with route groups; products = route segments
- `/database`, `/auth`, `/storage` each have own layout + nav highlighting

### 2. Shared Component Libraries

All major sites use internal design systems:
- **Vercel**: Geist UI components, `next-themes` for dark mode
- **Supabase**: Radix UI primitives + custom components, CSS variables for theming
- **Railway/Render**: Tailwind + custom components, CSS-in-JS (Styled Components/Emotion)
- **Linear**: Custom React components with CSS variables, Framer Motion

### 3. Content Management

- **Vercel/Supabase/Railway/Render**: Markdown/MDX + Next.js for docs; CMS (Sanity/Contentful) for marketing
- **PlanetScale**: Static HTML + JS (Vite/Remix), Sanity for blog
- **Personal sites**: Astro (ixtj.dev), custom static (Sindre), or platform (makerhandle)

---

## Decision Framework for Your Site

### Choose Mega Menu If:
- 4+ products with clear categories
- Products are used together (shared account/dashboard)
- Technical audience expects deep navigation
- **Examples**: Vercel, Supabase, Railway, Render

### Choose Product Cards on Homepage If:
- 2-4 products, distinct value props
- Want immediate visual comparison
- Simpler brand narrative
- **Examples**: PlanetScale, Linear (features)

### Choose Personal/Indie Style If:
- 2-6 products, personal brand matters
- Products vary in maturity (some building, some dead)
- Want to show journey + personality
- **Examples**: ixtj.dev, Sindre Sorhus, makerhandle

---

## Recommended Architecture for 2-4 Product Portfolio

```
/
├── index.html                    # Brand hero + product cards (3-4) → /products/:slug
├── products/
│   ├── product-a/                # Dedicated landing page
│   ├── product-b/
│   ├── product-c/
│   └── product-d/
├── blog/                         # Cross-product articles
├── about/                        # Maker story
├── now/                          # Current focus (indie hacker pattern)
└── /dashboard (if applicable)    # Unified product dashboard
```

**Navigation:**
- Desktop: Horizontal nav with "Products" mega menu (2 columns × 2 rows) OR top-level product links
- Mobile: Drawer with product cards (icon + name + description)

**Visual System:**
- Shared: Typography, spacing, dark/light mode, button styles
- Per-product: Unique 24px icon + accent color (used in badges, CTAs, doc sidebar)
- Product pages: Shared layout, product-specific hero illustration

**Cross-Linking:**
- Footer: Product grid (4 columns)
- Product pages: "Also try [Product B]" cards
- Dashboard: Unified project view

**Automation (from makerhandle):**
- Favicons: Auto-fetch + serve from your CDN
- Revenue: Webhook from payment provider (if applicable)
- Status: Manual toggle, but with "last updated" timestamp

---

## Notable Anti-Patterns to Avoid

| Anti-Pattern | Problem | Better Approach |
|--------------|---------|-----------------|
| Separate subdomains per product (productA.brand.com) | Fragments SEO, auth, brand equity | Subpaths (/product-a) with shared auth |
| No visual differentiation | Products blur together | Unique icon + accent color per product |
| Mega menu with >15 items ungrouped | Cognitive overload | Categorize (Agent Stack / Core Platform / Tools) |
| Stale portfolio (discontinued projects hidden) | Looks like you only ship once | Show failures with "Discontinued" badge + lesson learned |
| Different design systems per product | Inconsistent UX, higher maintenance | Shared component library + product tokens |
| Deep linking only to homepage | Misses product-specific intent | Deep link to `/products/:slug` from everywhere |

---

## References

### Primary Sources (HTML Analysis)
- Vercel.com — Homepage, `/products`, `/ai-sdk`, `/nextjs`, mega menu structure
- Supabase.com — Homepage, `/database`, `/auth`, `/docs`, mobile nav
- PlanetScale.com — Homepage, `/vitess`, `/postgres`, `/neki`, nav structure
- Railway.app — Homepage, mega menu, canvas UI, `/docs`
- Render.com — Homepage, mega menu, `/docs`, `/templates`
- Linear.app — Homepage, `/features`, nav, design system
- SindreSorhus.com — Homepage, `/apps`, `/open-source`, `/about`
- ixtj.dev — Homepage, `/projects`, `/blog`, terminal UI
- makerhandle.com/blog/indie-hacker-portfolio — Article + platform patterns

### Design System References
- kage.design/component/planetscale-navigation
- kage.design/component/linear-navigation
- kage.design/designs/railway-landing-page
- iter0.com/blog/railway-homepage-decoded
- shadcn.io/design/planetscale/raw

### Technical Articles
- "How Vercel adopted microfrontends" (Vercel Blog)
- "How product design works at PlanetScale" (PlanetScale Blog)
- "Building Railway on Railway" (Railway Blog)
- "Enhanced navigation in the Render Dashboard" (Render Changelog)
- Supabase multi-zone architecture discussions (GitHub/Next.js)

---

## Quick Reference: Product Count vs. Navigation Choice

| Product Count | Recommended Nav | Example |
|---------------|-----------------|---------|
| 2 | Top-level links or 2-card hero | PlanetScale (3), Linear (1+features) |
| 3-4 | Mega menu (2×2) OR product cards on home | Supabase (6), Railway (10+) |
| 5-8 | Mega menu with categories | Vercel (20+), Render (10+) |
| 8+ | Categorized mega menu + search | Vercel, AWS, GCP |
| Personal (2-6) | `/projects` page with status badges | ixtj.dev, makerhandle |
| Personal (10+) | Categorized `/apps` or `/open-source` | Sindre Sorhus |

---

*Research compiled for paperscope/modern_landing_page_design — use as reference for multi-product developer tool site architecture.*