# Internal search metadata (2.5.6)

The dashboard body, styles, network buttons and stats interface are restored to
2.5.4. SEO changes are restricted to metadata and HTTP endpoints:

- Descriptive title and description in the document head.
- Canonical URL, Open Graph/Twitter preview metadata and WebSite/WebApplication JSON-LD.
- `/robots.txt` allowing public assets and rendering APIs, including search crawlers.
- `/sitemap.xml` listing only the original dashboard `/`.
- Permanent redirects from `www` (in TLS mode) and `/index.html` to the canonical site.

The additional network and article pages from 2.5.5 have been removed. Their URLs
redirect to `/` and no longer appear in the sitemap. There is no added body text,
hidden keyword text, crawler-specific HTML, or extra navigation. The existing
interactive data still loads through JavaScript.

Canonical URLs use `tls.public_url`, falling back to the local HTTP listen address
when it is empty. The checked-in share preview is not displayed in the dashboard.

## Owner setup

1. Verify the domain in Google Search Console and Yandex Webmaster using the DNS
   TXT records supplied by those services.
2. Submit `https://validatorclock.xyz/sitemap.xml` to both.
3. Inspect the root page, chosen canonical and JavaScript rendering in those services.
4. Track indexing, impressions, queries and clicks. The deployment itself does not
   verify ownership or submit the sitemap through the owner's accounts.

References:

- https://developers.google.com/search/docs/appearance/ai-features
- https://yandex.ru/support/webmaster/ru/yandex-indexing/rendering
- https://developers.openai.com/api/docs/bots
- https://docs.perplexity.ai/docs/resources/perplexity-crawlers

## Internal attribution

The aggregate attribution collection introduced in 2.5.5 remains internal, with its
existing authenticated `/stats/traffic` endpoint. The additional stats UI is removed.
Only `/` is now considered a public landing page; older aggregate records are retained.

A source count is recorded for a public `page_open` that starts a new visit. Only
fixed source labels and known page paths are persisted in the analytics aggregates;
raw referrer URLs and campaign names are not. Missing referrers are grouped as
`Direct or unknown`. This is an incomplete referral signal, not a measure of every
AI citation or search impression.

## Verification

The server tests pin the original dashboard body's SHA-256 to the pre-SEO version.
The browser check verifies the original heading and button controls, the absence
of added content, metadata in the head and the existing dashboard interactions.
