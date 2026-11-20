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

local function receive(kind, data)
  if closed then return end
  if kind == WebSocketMessageType.TEXT then
    local ok, c = pcall(json.decode, data)
    if not ok or type(c) ~= "table" or not byte(c.r) or not byte(c.g) or not byte(c.b) then return end
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
