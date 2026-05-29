-- 10.3 Hard 90-day cap: delete any change_log row older than 90 days
-- regardless of cursor state. A device offline for >90 days will fall
-- through to the full-snapshot fallback (data/PLAN.md §7.3) instead of
-- catching up via delta. See data/PLAN.md §10.3.
DELETE FROM change_log
WHERE changed_at < datetime('now', '-90 days')
