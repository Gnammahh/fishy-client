# Epic reply fixtures

Replies shaped like Epic's real ones, with every extra field Epic sends, for `tests/epic_replies.rs`. All IDs, tokens and codes are fake. `{{SERVER}}` and `{{HASH}}` are filled in by the test.

To check whether Epic changed a reply (docs/EPIC-CHANGES.md): capture the new reply, **replace every token, code, account ID and email with fake text**, save it over the matching file and run `cargo test -p fishy-core --test epic_replies`. A failing test shows which field Fishy Client can no longer read.

| File | Epic call |
|---|---|
| `oauth-token.json` | `POST account-public-service…/account/api/oauth/token` |
| `library-items-page1.json`, `-page2.json` | `GET library-service…/library/api/public/items` (two pages, `nextCursor`) |
| `assets-windows.json` | `GET launcher-public-service…/launcher/api/public/assets/Windows?label=Live` |
| `catalog-bulk.json` | `GET catalog-public-service…/catalog/api/shared/namespace/{ns}/bulk/items` |
| `manifest-info.json` | `GET launcher-public-service…/launcher/api/public/assets/v2/platform/Windows/…/label/Live` |
| `manifest.json` | a game's file list from Epic's download servers (JSON form; most are binary) |
| `exchange-code.json` | `GET account-public-service…/account/api/oauth/exchange` (Play) |
