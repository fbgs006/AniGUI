-- AniGUI player controls: previous/next episode + AniSkip (OP/ED) skipping.
-- Installed next to anigui-tracker.lua by bootstrap::install_mpv_script().
--
-- Only active when mpv was launched by AniGUI (it exports ANIGUI_EP); a plain
-- mpv session is left untouched.

local mp = require 'mp'
local utils = require 'mp.utils'
local msg = require 'mp.msg'

local ep = tonumber(os.getenv("ANIGUI_EP") or "")
if not ep then return end

local total = tonumber(os.getenv("ANIGUI_EP_TOTAL") or "") or 0
local mal_id = tonumber(os.getenv("ANIGUI_MAL_ID") or "")
local nav_file = os.getenv("ANIGUI_NAV_FILE")

-- 0 means the episode count is unknown (airing show): don't cap "next".
local has_prev = ep > 1
local has_next = total <= 0 or ep < total

-- Exposed so uosc can show/hide its buttons: `<user-data/anigui/has_next>...`
mp.set_property_native("user-data/anigui/has_prev", has_prev)
mp.set_property_native("user-data/anigui/has_next", has_next)

-- ─── Episode navigation ──────────────────────────────────────────────────────
-- Each episode is its own ani-cli run, so switching means: leave a request
-- file for the backend and quit. The backend relaunches the target episode.

local function navigate(action)
    if action == "next" and not has_next then
        mp.osd_message("This is the last episode", 2)
        return
    elseif action == "prev" and not has_prev then
        mp.osd_message("This is the first episode", 2)
        return
    end
    if not nav_file then return end

    local f = io.open(nav_file, "w")
    if not f then
        mp.osd_message("Can't switch episode", 2)
        return
    end
    f:write(utils.format_json({ action = action, from = ep }))
    f:close()

    mp.osd_message(action == "next" and "Loading next episode…" or "Loading previous episode…", 3)
    mp.commandv("quit")
end

mp.add_forced_key_binding(">", "next-episode", function() navigate("next") end)
mp.add_forced_key_binding("<", "prev-episode", function() navigate("prev") end)

-- ─── AniSkip ─────────────────────────────────────────────────────────────────

local LABELS = {
    ["op"] = "Opening",
    ["mixed-op"] = "Opening",
    ["ed"] = "Ending",
    ["mixed-ed"] = "Ending",
    ["recap"] = "Recap",
}
local FALLBACK_SKIP_SECONDS = 85

local segments = {}
local active_segment = nil

local function find_segment(t)
    for _, seg in ipairs(segments) do
        if t >= seg.start - 0.5 and t < seg.finish then return seg end
    end
    return nil
end

-- Marks segments as chapters so uosc draws them on the timeline. Skipped when
-- the file already carries its own chapters.
local function apply_chapters()
    if #segments == 0 or (mp.get_property_number("chapters", 0) or 0) > 0 then return end

    local chapters = {}
    local cursor = 0
    for _, seg in ipairs(segments) do
        if seg.start > cursor + 1 then
            chapters[#chapters + 1] = { time = cursor, title = "Episode" }
        end
        chapters[#chapters + 1] = { time = seg.start, title = seg.label }
        cursor = seg.finish
    end
    local duration = mp.get_property_number("duration")
    if not duration or cursor < duration - 1 then
        chapters[#chapters + 1] = { time = cursor, title = "Episode" }
    end
    mp.set_property_native("chapter-list", chapters)
end

local function fetch_skip_times()
    if not mal_id then return end

    local url = string.format(
        "https://api.aniskip.com/v2/skip-times/%d/%d?types=op&types=ed&types=recap&types=mixed-op&types=mixed-ed&episodeLength=0",
        mal_id, ep
    )
    mp.command_native_async({
        name = "subprocess",
        args = { "curl", "-s", "-m", "10", url },
        capture_stdout = true,
    }, function(ok, res)
        if not ok or not res or res.status ~= 0 then
            msg.warn("AniSkip request failed")
            return
        end
        local data = utils.parse_json(res.stdout or "")
        if not data or not data.results then return end

        local found = {}
        for _, r in ipairs(data.results) do
            local label = LABELS[r.skipType or ""]
            local interval = r.interval
            if label and interval and interval.startTime and interval.endTime then
                found[#found + 1] = {
                    kind = r.skipType,
                    label = label,
                    start = interval.startTime,
                    finish = interval.endTime,
                }
            end
        end
        table.sort(found, function(a, b) return a.start < b.start end)
        segments = found
        apply_chapters()
    end)
end

local function skip()
    local t = mp.get_property_number("time-pos")
    if not t then return end

    local seg = find_segment(t)
    if not seg then
        mp.commandv("seek", tostring(FALLBACK_SKIP_SECONDS), "relative")
        mp.osd_message("Skipped " .. FALLBACK_SKIP_SECONDS .. "s", 2)
        return
    end

    -- An ending that runs to the end of the file: jump straight to the next
    -- episode instead of sitting on the last frame.
    local duration = mp.get_property_number("duration")
    local is_ending = seg.kind == "ed" or seg.kind == "mixed-ed"
    if is_ending and has_next and duration and seg.finish >= duration - 2 then
        navigate("next")
        return
    end

    mp.commandv("seek", tostring(seg.finish), "absolute")
    mp.osd_message("Skipped " .. seg.label, 2)
end

mp.add_forced_key_binding("TAB", "skip", skip)

-- Prompt once each time playback enters a segment.
mp.observe_property("time-pos", "number", function(_, t)
    if not t then return end
    local seg = find_segment(t)
    if seg ~= active_segment then
        active_segment = seg
        if seg then
            mp.osd_message("Skip " .. seg.label .. "  —  press Tab", 4)
        end
    end
end)

mp.register_event("file-loaded", fetch_skip_times)
