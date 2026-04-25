# Stream fixtures for broken-session detection

Real `.jsonl` snippets (or minimal synthetic reductions) used by
`tests/broken_session_detection.rs` to exercise
`stream_broken_session_symptom` and `detect_image_dimension_blocked` against
the shapes CC actually emits.

Each fixture is **one CC session**: a single `system/init` event followed by
the events that describe the failure mode. Real streams were collected from
`~/.mando/state/cc-streams/` across 1355 sessions and reduced to the minimum
content the detector inspects.

| File | Shape | Expected |
|---|---|---|
| `fp_worker86_2_skill_template.jsonl` | Skill template with "rate limit" + tool_use_error + clean result | `None` (primary FP regression) |
| `fp_worker86_2_tool_error.jsonl` | Edit-before-Read tool_use_error, clean result | `None` |
| `killed_in_flight.jsonl` | No result + last tool_result kill signature | `SessionInterrupted` (86-1 regression) |
| `killed_in_flight_trailing_system.jsonl` | Kill signature + trailing system events | `SessionInterrupted` |
| `rate_limit_real.jsonl` | Real "You've hit your limit" result | `RateLimitAborted` |
| `idle_timeout_real.jsonl` | Real "API Error: Stream idle timeout" result | `StreamIdleTimeout` |
| `no_conversation_errors_array.jsonl` | Real `errors[]` shape | `NoConversationFound` |
| `mock_529.jsonl` | Sandbox mock error | generic `cc_is_error` |
| `api400_advisor.jsonl` | Real "Advisor tool result" result | generic `cc_is_error` |
| `not_logged_in.jsonl` | Real "Not logged in" result | generic `cc_is_error` |
| `request_timed_out.jsonl` | Real "Request timed out" result | generic `cc_is_error` |
| `content_filtering.jsonl` | Real "content filtering policy" result | generic `cc_is_error` |
| `clarifier_spawn_fail.jsonl` | `write_error_result` shape (513/616 corpus cases) | generic `cc_is_error` |
| `image_dimension.jsonl` | **Synthetic** — dimension limit tool_result. No real corpus match; matches CC's documented error shape. | `detect_image_dimension_blocked=true` |
| `worker_thinking_mentions_rate_limit.jsonl` | Assistant thinking + clean result | `None` |

## Provenance

Real samples were collected 2026-04-22 from `~/.mando/state/cc-streams/` by
bucketing all `type:"result" && is_error:true` events by text content
(see PR #960 plan for the full corpus sweep). The 54 rate-limit events,
9 idle-timeout events, and 513 spawn-failure events collapsed to the canonical
strings used here.

`image_dimension.jsonl` is synthetic: the corpus has zero matches for the
dimension-limit pattern, because in practice Mando workers don't submit
oversized images. The fixture tracks CC's documented error shape so the guard
isn't dead if CC does emit one in the future. See
`rust/crates/global-claude/src/stream.rs::detect_image_dimension_blocked`.
