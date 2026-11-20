-- Run from File > Scripts in Aseprite 1.3 or later.
local socket, dialog
local listeners = {}
local seen = {}
local closed = false

local function colorAt(slot)
  if slot == 0 then return app.fgColor end
  return app.bgColor
end

local function rgbKey(c)
  return c.red * 65536 + c.green * 256 + c.blue
end

local function send(slot)
  if closed or seen[slot] == nil then return end
  local c = colorAt(slot)
  if rgbKey(c) == seen[slot] then return end
  seen[slot] = rgbKey(c)
  socket:sendText(json.encode{slot=slot, r=c.red, g=c.green, b=c.blue})
end

local function byte(n)
  return type(n) == "number" and n >= 0 and n <= 255 and n == math.floor(n)
end

-- Aseprite's JSON decoder exposes objects and arrays as indexable userdata.
local function jsonObject(value)
  return type(value) == "userdata" or type(value) == "table"
end

local function receive(kind, data)
  if closed then return end
  if kind == WebSocketMessageType.TEXT then
    local ok, c = pcall(json.decode, data)
    if not ok or not jsonObject(c) then return end
    local entries = c.palette
    if jsonObject(entries) then
      if #entries == 0 or #entries > 256 then return end
      for i = 1, #entries do
        local rgb = entries[i]
        if not jsonObject(rgb) or not byte(rgb[1]) or not byte(rgb[2]) or not byte(rgb[3]) then return end
      end
      local sprite = app.sprite
      local palette = sprite and sprite.palettes[1] or app.defaultPalette
      if not palette then return end
      local function append()
        local start = #palette
        palette:resize(start + #entries)
        for i = 1, #entries do
          local rgb = entries[i]
          palette:setColor(start+i-1, Color{r=rgb[1], g=rgb[2], b=rgb[3], a=255})
        end
        if sprite then sprite:setPalette(palette) else app.defaultPalette = palette end
      end
      local added
      if sprite then added = pcall(app.transaction, "Import OkColorPicker palette", append)
      else added = pcall(append) end
      if added then app.refresh()
      else dialog:modify{id="status", text="Cannot append to this palette"} end
      return
    end
    if not byte(c.r) or not byte(c.g) or not byte(c.b) then return end
    local slot = c.slot or 0
    if slot ~= 0 and slot ~= 1 then return end
    local prev = colorAt(slot)
    local color = Color{r=c.r, g=c.g, b=c.b, a=prev.alpha}
    seen[slot] = rgbKey(color)
    if rgbKey(prev) ~= seen[slot] then
      if slot == 0 then app.fgColor = color else app.bgColor = color end
    end
  elseif kind == WebSocketMessageType.OPEN then
    seen = {}
    dialog:modify{id="status", text="Connected"}
  elseif kind == WebSocketMessageType.CLOSE or kind == WebSocketMessageType.ERROR then
    seen = {}
    dialog:modify{id="status", text="Waiting for OkColorPicker"}
  end
end

dialog = Dialog{title="OkColorPicker", onclose=function()
  closed = true
  for _, listener in ipairs(listeners) do app.events:off(listener) end
  socket:close()
end}
dialog:label{id="status", text="Waiting for OkColorPicker"}
dialog:button{text="Disconnect", onclick=function() dialog:close() end}
socket = WebSocket{url="ws://127.0.0.1:9009", deflate=false, onreceive=receive}
listeners[1] = app.events:on("fgcolorchange", function() send(0) end)
listeners[2] = app.events:on("bgcolorchange", function() send(1) end)
dialog:show{wait=false}
socket:connect()
