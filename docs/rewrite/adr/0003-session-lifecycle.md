# ADR 0003: Rust-Planned Session Lifecycle

## Status

Accepted for the staged Rust rewrite.

## Context

Authentication response parsing and failed-response classification already have
versioned Rust owners, but the live session lifecycle is still split between
application startup, password login, WebView login, guest entry, and logout.
Those paths mutate `hasSignedIn`, `needSignIn`, `gallerySite`, profile fields, and
WebView cookies independently.

Two legacy concurrency properties make a direct flag-for-flag port unsafe:

- Android's `CookieManager.removeAllCookies(null)` starts asynchronous removal,
  so the existing password login can issue its request before old cookies are
  gone.
- `postLogin()` uses `GlobalScope` and launches profile/configuration work in
  parallel, so a delayed result from an older attempt can overwrite a newer
  account or logout.

The existing `eh_domain::contracts::SessionState` is an account-capability DTO
with an established field shape. Reinterpreting that type as a navigation state
would silently change its Rust/Kotlin wire contract.

## Decision

### Domain owner

`eh_domain::session::lifecycle` owns a pure V1 state planner with module identifier
`sessionLifecycle`. It introduces a separate `SessionLifecycleState`; the existing
account-capability `SessionState` remains unchanged.

The sanitized request contains:

- whether the Android CookieManager currently has both identity cookies;
- persisted `hasSignedIn` and `needSignIn` facts;
- an optional display username;
- the public or ExHentai site category;
- exactly one command: `restore`, `signInSucceeded`, `enterGuest`, or `signOut`.

Passwords, cookie names or values, headers, URLs, response bodies, API keys,
WebView data, and localized messages never enter the request or logs.

The response plan contains:

- `SessionLifecycleState` with mode `signInRequired`, `guest`, or `authenticated`
  and an optional username;
- the canonical site;
- whether Android must await complete cookie removal;
- a profile update of `keep`, `set`, or `clear`.

`signInSucceeded` without identity cookies returns a typed
`ApiEnvelope.err(RustError::NeedLogin)`, not a success plan and not a JNI transport
exception.

### Complete transition table

| Command/facts | Mode | Site | Clear cookies | Profile |
| --- | --- | --- | --- | --- |
| `restore`, `needSignIn=true` | `signInRequired` | public | only when identity cookies remain | `clear` |
| `restore`, signed-in flag plus identity cookies | `authenticated` | stored site | no | `keep` |
| `restore`, signed-in flag without identity cookies | `signInRequired` | public | no | `clear` |
| `restore`, no signed-in flag and no sign-in requirement | `guest` | public | only when identity cookies remain | `clear` |
| `signInSucceeded` with identity cookies | `authenticated` | command site | no | `set` when username is known, otherwise `keep` |
| `signInSucceeded` without identity cookies | typed `NeedLogin` error | n/a | no | n/a |
| `enterGuest` | `guest` | public | yes | `clear` |
| `signOut` | `signInRequired` | public | yes | `clear` |

An authenticated plan may never clear cookies or clear the profile. A
non-authenticated plan has no username, uses the public site, and clears the
profile. Missing usernames are valid for WebView authentication because the
best-effort profile request may fill them later.

### Android coordinator

`client/session/SessionController` owns platform orchestration, not business rules.
It uses one coroutine `Mutex` and a monotonically increasing in-process attempt
generation:

1. `beginSignIn()` plans logout, awaits cookie removal, applies the plan, and
   returns the new attempt generation.
2. Password or WebView authentication performs Android-owned HTTP/WebView work.
3. `finishSignIn()` accepts only the still-current generation, runs structured
   profile/configuration refresh without holding the mutation lock, then reacquires
   the lock, revalidates and consumes the one-shot generation, and applies one Rust
   plan.
4. Restore, guest entry, logout, and profile refresh use the same lock and
   invalidate older attempts.

The WebView is not created until cookie removal completes, and its successful
handoff is one-shot. `postLogin` is a suspending structured operation; it does not
use `GlobalScope`.

The Android CookieManager adapter exposes suspending removal and resumes only
from the platform completion callback. Session state is not published before
that callback.

### Failure and rollback

JNI/CBOR/linkage or plan-shape failure may execute the unchanged Kotlin behavior
only before cookie/profile/settings application begins. A typed `NeedLogin` is a
business result and is never converted into a legacy authenticated success. Once
application begins, errors propagate without replay.

`useRustCoreSessionLifecycle` defaults to `false`. The disabled path retains the
legacy state mutations while still using the awaited cookie-removal primitive.
That compatibility branch therefore preserves two known quirks: guest entry changes
only the site/sign-in requirement, and logout retains the cached avatar. Canonical V1
plans clear the complete profile and authenticated identity for both transitions.
The switch must remain available until native bridge, startup, password/WebView,
guest, logout, and rollback tests pass on Android.

## Alternatives Considered

- Moving credentials or cookie values into Rust was rejected because Android
  WebView/Ktor interoperability and secret persistence remain platform concerns.
- Reusing the existing `SessionState` name was rejected because its established
  capability fields describe a different contract.
- Keeping fire-and-forget cookie removal was rejected because it cannot prove
  account isolation during login replacement.
- Holding a mutex throughout WebView interaction was rejected because the user
  may remain in that flow indefinitely; the generation token provides the same
  stale-result protection without blocking logout.

## Consequences

- Session mode and normalization become host-testable Rust behavior with an
  independent 95% line/function coverage gate.
- Kotlin continues to own all secrets, HTTP, WebView, Settings, and CookieManager
  effects, but those effects now have one serialized coordinator.
- Login and logout become suspending operations, so all callers must use a
  lifecycle-aware coroutine scope.
- The JNI V1 manifest gains one exact symbol and Android instrumentation must
  execute the bridge before the rollout default can change.
