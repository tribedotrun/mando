-- Remove the retired per-task ask and advisor subsystems and their history.

DELETE FROM timeline_events
WHERE event_type IN ('human_ask', 'human_ask_failed');

DELETE FROM cc_sessions
WHERE caller IN ('task-ask', 'advisor')
   OR caller LIKE 'task-ask:%'
   OR caller LIKE 'advisor:%';

DROP TABLE IF EXISTS ask_history;
