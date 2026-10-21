# Brainstorm — lens: unspoken needs

- **Make the instance silent by default (zero phone-home)** — SECURITY — value 5 — effort S — risk low — category: privacy
  Why: the user wants a public, self-hosted site; the pack lists import, ACME, update checks, webhooks and avatars, each of which can leak data. Assumption: no outbound-call inventory exists yet.
  Done when: a fresh instance run in a network-denied container passes its full e2e suite, and `gitter doctor` lists every outbound destination as "off" until enabled in settings.

- **Show an outbound-traffic ledger in admin** — FEATURE — value 4 — effort M — risk low — category: privacy
  Why: assumption: owners will ask "what does my box talk to?" once webhooks, mirrors, ACME and the runner exist. Nothing in the pack records this.
  Done when: every outbound request (host, purpose, bytes, time) appears in an admin page, and a test webhook produces exactly one entry.

- **Self-host avatars, fonts, and syntax assets with no third-party requests** — FIX — value 4 — effort S — risk low — category: privacy
  Why: the stack vendors marked, DOMPurify and highlight.js; identicons are already planned, which suggests no Gravatar. Assumption: an email-based avatar fallback will be tempting later.
  Done when: a Playwright run on every page records zero requests to other origins, and the check is a CI gate.

- **Hide emails by default with a per-commit privacy alias** — FEATURE — value 4 — effort M — risk med — category: privacy
  Why: a public site exposes author emails through commits and clone data. Users rarely think of this until they are scraped. Assumption: the planned accounts store emails.
  Done when: pushing with a user's `@noreply` alias maps to their profile, and the API and UI never show the real email to other users.

- **Strip metadata from uploads and rendered content** — SECURITY — value 3 — effort S — risk low — category: privacy
  Why: the pack plans avatars, release assets and issue attachments, and EXIF GPS in images is a classic leak. Assumption: image uploads exist.
  Done when: an upload with GPS EXIF is served with the EXIF removed, verified by a test.

- **Add private-repo access logs the owner can read** — FEATURE — value 3 — effort M — risk low — category: privacy
  Why: the user plans guest-pass links and read-only sharing; owners will want to know who looked and when. Assumption: audit-log infrastructure exists, since a tamper-evident log was proposed.
  Done when: opening a guest link creates an owner-visible entry with time and token label, and that entry is not visible to other users.

- **Give analytics counts without storing visitor identity** — FEATURE — value 3 — effort S — risk low — category: privacy
  Why: the traffic, clone and star counts people want normally imply tracking. Assumption: a view counter is wanted.
  Done when: a repo shows daily view counts, and the database holds no IP or user-agent rows older than 24 hours, confirmed by schema test.

- **Offer a "delete me completely" that also scrubs git history identity** — FEATURE — value 4 — effort L — risk high — category: privacy
  Why: account export and deletion are listed, but commits keep names and emails forever. People expect erasure to cover that. Assumption: this is a legal and trust issue on a public instance.
  Done when: deleting an account rewrites their identity to a placeholder in mirrored or forked metadata, or the UI states precisely what remains, and a test covers both paths.

- **Encrypt secrets and backups with a key the owner holds** — SECURITY — value 4 — effort M — risk med — category: privacy
  Why: CI secrets and backups sit in a single volume, so a stolen disk or backup file exposes them. Assumption: secrets are stored in SQLite.
  Done when: the database file contains no plaintext secrets, and a backup restores only with the owner's key.

- **Wild: Add a "paranoid mode" toggle that turns the box into a Tor onion service** — FEATURE — value 3 — effort M — risk med — category: privacy
  Why: wild. A bundled onion address lets activists and private groups host without DNS, a certificate, or a public IP.
  Done when: enabling the toggle prints a working `.onion` address and a clone through Tor succeeds.

- **Wild: Support per-repo end-to-end-encrypted issues and comments** — FEATURE — value 2 — effort L — risk high — category: privacy
  Why: wild. Collaborators share a key, so even the server admin cannot read discussion of sensitive reports such as undisclosed vulnerabilities.
  Done when: issue text stored in SQLite is ciphertext, and two members decrypt it in the browser while an admin query shows only bytes.

- **Wild: Add a "redaction rehearsal" that scans your public repos for what a scraper would learn** — FEATURE — value 3 — effort M — risk low — category: privacy
  Why: wild. It reports emails, internal hostnames, tokens, home-directory paths and timezone patterns found in history, and offers fixes.
  Done when: running it on a seeded repo flags all five planted leaks and links each to the commit.

- **Make CI logs and artifacts private by default, with explicit public toggles** — SECURITY — value 4 — effort S — risk low — category: privacy
  Why: on a public repo, logs often contain internal paths, usernames and env dumps. Assumption: logs currently inherit repo visibility.
  Done when: an anonymous request for a log on a public repo returns 404 until the owner enables public logs, with a test for both states.

- **Honor a "do not index" setting and serve robots/headers consistently** — FEATURE — value 3 — effort S — risk low — category: privacy
  Why: self-hosters running a public site for a small group often do not want search engines or AI crawlers to index it. Assumption: no robots handling is planned.
  Done when: the setting adds `noindex` headers and `robots.txt` rules on all pages, including raw and archive endpoints, verified by curl.
