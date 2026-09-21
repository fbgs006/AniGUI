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

if os.getenv("ANIGUI_FULLSCREEN") == "1" then
    mp.set_property_bool("fullscreen", true)
end

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

-- Autoplay: when the episode plays through to its end, roll on to the next one.
-- Only a natural end counts ("eof"); quitting or navigating by hand must not.
if os.getenv("ANIGUI_AUTOPLAY") == "1" and has_next and nav_file then
    mp.register_event("end-file", function(event)
        if event.reason ~= "eof" then return end
        local f = io.open(nav_file, "w")
        if not f then return end
        f:write(utils.format_json({ action = "next", from = ep }))
        f:close()
    end)
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


-- ─── On-screen button (Netflix-style) ────────────────────────────────────────
-- A clickable pill at the bottom right while something skippable is playing:
-- "Skip Opening" during an OP, "Next Episode" during an ED or the end credits.

local CREDITS_FALLBACK_SECONDS = 25

local COLOR_BG = "0E0A0B"      -- ASS colours are BGR
local COLOR_TEXT = "EEF1F4"
local COLOR_ACCENT = "4D5FFF"

local function is_ending(seg)
    return seg.kind == "ed" or seg.kind == "mixed-ed"
end

local function has_ending_segment()
    for _, seg in ipairs(segments) do
        if is_ending(seg) then return true end
    end
    return false
end

local function seek_past(seg)
    mp.commandv("seek", tostring(seg.finish), "absolute")
    mp.osd_message("Skipped " .. seg.label, 2)
end

-- What the button should offer at time `t` (nil = no button). `key` tells two
-- offers apart so the button isn't rebuilt on every time-pos tick.
local function wanted_button(t)
    local seg = find_segment(t)
    if seg then
        if is_ending(seg) and has_next then
            return { key = seg, label = "Next Episode  ▶", action = function() navigate("next") end }
        end
        return { key = seg, label = "Skip " .. seg.label, action = function() seek_past(seg) end }
    end

    -- No AniSkip ending known: still offer "Next Episode" over the final credits.
    local duration = mp.get_property_number("duration")
    if has_next and duration and not has_ending_segment() and t >= duration - CREDITS_FALLBACK_SECONDS then
        return { key = "credits", label = "Next Episode  ▶", action = function() navigate("next") end }
    end
    return nil
end

local overlay = mp.create_osd_overlay("ass-events")
local button = nil    -- current offer, or nil
local rect = nil      -- {x, y, w, h} in OSD pixels while the button is shown
local hovered = false

local function rounded_rect(w, h, r)
    -- Corners are cubic curves whose control points sit on the corner itself.
    return string.format(
        "m %d 0 l %d 0 b %d 0 %d 0 %d %d l %d %d b %d %d %d %d %d %d l %d %d b 0 %d 0 %d 0 %d l 0 %d b 0 0 0 0 %d 0",
        r, w - r,
        w, w, w, r,
        w, h - r,
        w, h, w, h, w - r, h,
        r, h,
        h, h, h - r,
        r,
        r
    )
end

local function render()
    if not button then
        overlay:remove()
        rect = nil
        return
    end

    local w, h = mp.get_osd_size()
    if not w or w == 0 or not h or h == 0 then return end

    local s = h / 720
    local bh = math.floor(52 * s)
    local bw = math.floor((#button.label * 11 + 70) * s)
    local x = math.floor(w - bw - 48 * s)
    local y = math.floor(h - bh - 170 * s)
    rect = { x = x, y = y, w = bw, h = bh }

    local fill = hovered and COLOR_ACCENT or COLOR_BG
    local text = hovered and COLOR_BG or COLOR_TEXT
    local fill_alpha = hovered and "00" or "30"

    overlay.res_x = w
    overlay.res_y = h
    overlay.data = string.format(
        "{\\an7\\pos(%d,%d)\\bord2\\shad0\\1c&H%s&\\1a&H%s&\\3c&H%s&\\p1}%s{\\p0}\n" ..
        "{\\an5\\pos(%d,%d)\\bord0\\shad0\\b1\\fs%d\\1c&H%s&}%s",
        x, y, fill, fill_alpha, COLOR_ACCENT, rounded_rect(bw, bh, math.floor(8 * s)),
        math.floor(x + bw / 2), math.floor(y + bh / 2), math.floor(26 * s), text, button.label
    )
    overlay:update()
end

-- The click binding only exists while the pointer is over the button, so every
-- other click keeps going to uosc / mpv untouched.
local function set_hover(value)
    if value == hovered then return end
    hovered = value
    if value then
        mp.add_forced_key_binding("MBTN_LEFT", "button-click", function(event)
            if event.event == "up" or event.event == "repeat" then return end
            if button then button.action() end
        end, { complex = true })
    else
        mp.remove_key_binding("button-click")
    end
    render()
end

local function update_hover(pos)
    pos = pos or mp.get_property_native("mouse-pos")
    local inside = button and rect and pos and pos.hover
        and pos.x >= rect.x and pos.x <= rect.x + rect.w
        and pos.y >= rect.y and pos.y <= rect.y + rect.h
    set_hover(inside and true or false)
end

local function refresh()
    local t = mp.get_property_number("time-pos")
    local wanted = t and wanted_button(t) or nil

    local changed = (wanted == nil) ~= (button == nil)
        or (wanted and button and wanted.key ~= button.key)
    if changed then
        button = wanted
        render()
    end
    update_hover()
end

local function skip()
    if button then
        button.action()
        return
    end
    mp.commandv("seek", tostring(FALLBACK_SKIP_SECONDS), "relative")
    mp.osd_message("Skipped " .. FALLBACK_SKIP_SECONDS .. "s", 2)
end

mp.add_forced_key_binding("TAB", "skip", skip)

mp.observe_property("time-pos", "number", refresh)
mp.observe_property("mouse-pos", "native", function(_, pos) update_hover(pos) end)
mp.observe_property("osd-dimensions", "native", function()
    render()
    update_hover()
end)

mp.register_event("file-loaded", fetch_skip_times)
