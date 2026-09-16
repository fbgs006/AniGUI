integrated video player - skip intro/outro button (netflix style), resume where left off
  - decided: web-based <video> player, not mpv overlay, because mobile (android/ios) portability matters
  - first step: feasibility spike, does the stream url actually play in a plain <video>+hls.js or does the source block it (cors/referer)
  - if spike fails, fall back to mpv overlay approach instead

bulk list management - mass-update anilist progress/status (mark a whole season watched, bulk move shows between planning/watching/dropped)

local watch-history export/stats - history.json already tracks what's watched but nothing surfaces it. stats view (hours watched, most-watched genres) + csv export

offline/queued downloads manager - downloads currently run one at a time via a modal. real queue: multiple episodes queued, progress on all of them, auto-retry on failure
